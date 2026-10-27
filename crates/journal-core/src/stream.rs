//! Sealed chunk stream: the on-disk format for kept audio and recording drafts.
//! Byte layout: docs/SPEC.md ("Sealed chunk stream").
//!
//! A writer needs only the vault's *public* recording key, so a recording keeps
//! saving after the app locks. Reading needs an unlocked [`Session`].

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use x25519_dalek::{PublicKey, SharedSecret, StaticSecret};
use zeroize::Zeroizing;

use crate::crypto::{self, Key};
use crate::vault::{RecordingPublicKey, Session};

const MAGIC: &[u8; 8] = b"PVJSTRM1";
const HEADER_LEN: usize = 8 + 32 + 24 + 48 + 16;
const SEAL_INFO: &[u8] = b"privenator/v1/seal";
const TAG_LEN: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("not a journal stream")]
    NotAStream,
    #[error("this stream belongs to a different vault")]
    WrongKey,
    #[error("stream has been modified or corrupted")]
    Tampered,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct StreamWriter {
    file: File,
    ck: Key,
    header: [u8; HEADER_LEN],
    stream_id: [u8; 16],
    counter: u64,
}

impl StreamWriter {
    /// Creates (or truncates) `path` and writes the header, sealing a fresh
    /// content key to `recipient`.
    pub fn create(path: &Path, recipient: &RecordingPublicKey) -> Result<Self, StreamError> {
        let ck = crypto::random_key();
        let eph = StaticSecret::from(crypto::random::<32>());
        let eph_pub = PublicKey::from(&eph);
        let wrap = seal_key(&eph.diffie_hellman(&recipient.0), &eph_pub, &recipient.0);
        let ck_nonce = crypto::random::<24>();
        let ck_sealed = crypto::seal_with_nonce(&wrap, &ck_nonce, &seal_aad(&eph_pub), ck.as_ref());
        let stream_id = crypto::random::<16>();

        let mut header = [0u8; HEADER_LEN];
        let parts: [&[u8]; 5] = [MAGIC, eph_pub.as_bytes(), &ck_nonce, &ck_sealed, &stream_id];
        let mut at = 0;
        for p in parts {
            header[at..at + p.len()].copy_from_slice(p);
            at += p.len();
        }

        let mut file = File::create(path)?;
        file.write_all(&header)?;
        file.sync_data()?;
        Ok(Self { file, ck, header, stream_id, counter: 0 })
    }

    /// Encrypts `data` as one chunk and syncs it, so it survives a crash.
    pub fn append(&mut self, data: &[u8]) -> Result<(), StreamError> {
        self.write_chunk(data, false)
    }

    /// Writes the empty final chunk that marks the stream complete.
    pub fn finish(mut self) -> Result<(), StreamError> {
        self.write_chunk(&[], true)
    }

    fn write_chunk(&mut self, data: &[u8], last: bool) -> Result<(), StreamError> {
        let ct = crypto::seal_with_nonce(&self.ck, &chunk_nonce(&self.stream_id, self.counter), &chunk_aad(&self.header, last), data);
        let len = u32::try_from(ct.len()).expect("chunks are far below 4 GiB");
        let mut buf = Vec::with_capacity(4 + ct.len());
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&ct);
        self.file.write_all(&buf)?;
        self.file.sync_data()?;
        self.counter += 1;
        Ok(())
    }
}

pub struct StreamContents {
    pub chunks: Vec<Zeroizing<Vec<u8>>>,
    /// False when the writer never called `finish` (a crash or a torn last
    /// write). Fine for drafts; kept audio must be finished.
    pub finished: bool,
}

pub fn read(path: &Path, session: &Session) -> Result<StreamContents, StreamError> {
    let bytes = Zeroizing::new(fs::read(path)?);
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err(StreamError::NotAStream);
    }
    let header: [u8; HEADER_LEN] = bytes[..HEADER_LEN].try_into().expect("length checked");
    let eph_pub = PublicKey::from(<[u8; 32]>::try_from(&header[8..40]).expect("32 bytes"));
    let ck_nonce: [u8; 24] = header[40..64].try_into().expect("24 bytes");
    let ck_sealed = &header[64..112];
    let stream_id: [u8; 16] = header[112..128].try_into().expect("16 bytes");

    let secret = session.recording_secret();
    let wrap = seal_key(&secret.diffie_hellman(&eph_pub), &eph_pub, &PublicKey::from(secret));
    let ck = crypto::open_key(&wrap, &ck_nonce, &seal_aad(&eph_pub), ck_sealed).ok_or(StreamError::WrongKey)?;

    let mut chunks = Vec::new();
    let mut at = HEADER_LEN;
    let mut counter = 0u64;
    let mut finished = false;
    while at < bytes.len() {
        if finished {
            return Err(StreamError::Tampered); // data after the final chunk
        }
        // A length or body cut short is a torn write from a crash: stop there.
        let Some(len_bytes) = bytes.get(at..at + 4) else { break };
        let len = u32::from_le_bytes(len_bytes.try_into().expect("4 bytes")) as usize;
        let Some(ct) = bytes.get(at + 4..at + 4 + len) else { break };
        if len < TAG_LEN {
            return Err(StreamError::Tampered);
        }
        let nonce = chunk_nonce(&stream_id, counter);
        let plain = match crypto::open(&ck, &nonce, &chunk_aad(&header, false), ct) {
            Some(p) => p,
            None => {
                let p = crypto::open(&ck, &nonce, &chunk_aad(&header, true), ct).ok_or(StreamError::Tampered)?;
                finished = true;
                p
            }
        };
        if !finished {
            chunks.push(plain);
        }
        at += 4 + len;
        counter += 1;
    }
    Ok(StreamContents { chunks, finished })
}

/// Wrap key for the content key: HKDF over the X25519 shared secret, salted
/// with the ephemeral public key then the recipient public key.
fn seal_key(shared: &SharedSecret, eph_pub: &PublicKey, recipient: &PublicKey) -> Key {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(eph_pub.as_bytes());
    salt[32..].copy_from_slice(recipient.as_bytes());
    crypto::hkdf_sha256(shared.as_bytes(), &salt, SEAL_INFO)
}

fn seal_aad(eph_pub: &PublicKey) -> [u8; 40] {
    let mut aad = [0u8; 40];
    aad[..8].copy_from_slice(MAGIC);
    aad[8..].copy_from_slice(eph_pub.as_bytes());
    aad
}

fn chunk_nonce(stream_id: &[u8; 16], counter: u64) -> [u8; 24] {
    let mut n = [0u8; 24];
    n[..16].copy_from_slice(stream_id);
    n[16..].copy_from_slice(&counter.to_le_bytes());
    n
}

fn chunk_aad(header: &[u8; HEADER_LEN], last: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(HEADER_LEN + 1);
    aad.extend_from_slice(header);
    aad.push(last as u8);
    aad
}
