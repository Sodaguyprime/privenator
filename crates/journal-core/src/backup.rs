//! Encrypted backup and restore: one `.vjbak` file (docs/SPEC.md ".vjbak backup").
//!
//! The archive holds files that are already encrypted at rest (vault.json's
//! wrapped keys, the SQLCipher DB, sealed audio); the backup encryption adds a
//! second layer under a passphrase, so the file is safe on a USB drive.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::{self, Key};
use crate::journal::{Journal, JournalError};
use crate::vault::KdfParams;

const MAGIC: &[u8; 8] = b"PVJBAK01";
const CHUNK: usize = 1 << 20; // 1 MiB of plaintext per chunk
/// The only paths an archive may contain.
const VAULT: &str = "vault.json";
const DB: &str = "journal.db";
const AUDIO: &str = "audio";

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("this file is not a Privenator backup")]
    NotABackup,
    /// The first chunk didn't authenticate. Usually the passphrase; a file
    /// damaged in its first megabyte looks the same.
    #[error("the passphrase doesn't open this backup (or the file is damaged)")]
    WrongPassphrase,
    #[error("the backup is damaged or has been altered")]
    Tampered,
    #[error("the backup is from a newer version of the app")]
    TooNew,
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error("i/o: {0}")]
    Io(io::Error),
}

impl From<io::Error> for BackupError {
    fn from(e: io::Error) -> Self {
        // Chunk errors travel through io::Read as custom io errors, possibly
        // wrapped again by the tar reader.
        let mut source: Option<&(dyn std::error::Error + 'static)> = e.get_ref().map(|inner| inner as _);
        while let Some(err) = source {
            if let Some(failure) = err.downcast_ref::<ChunkFailure>() {
                return match failure {
                    ChunkFailure::First => Self::WrongPassphrase,
                    ChunkFailure::Later => Self::Tampered,
                };
            }
            source = err.source().or_else(|| err.downcast_ref::<io::Error>().and_then(|io| io.get_ref().map(|inner| inner as _)));
        }
        Self::Io(e)
    }
}

#[derive(Debug, thiserror::Error)]
enum ChunkFailure {
    #[error("first chunk failed authentication")]
    First,
    #[error("chunk failed authentication or the stream is truncated")]
    Later,
}

#[derive(Serialize, Deserialize)]
struct Header {
    version: u32,
    kdf: Kdf,
    #[serde(with = "crate::vault::b64")]
    stream_id: Vec<u8>,
    created_at: i64,
}

#[derive(Serialize, Deserialize)]
struct Kdf {
    alg: String,
    m_kib: u32,
    t: u32,
    p: u32,
    #[serde(with = "crate::vault::b64")]
    salt: Vec<u8>,
}

/// Writes a backup of the journal in `data_dir` to `out`.
pub fn create(data_dir: &Path, journal: &Journal, out: &Path, passphrase: &SecretString, kdf: KdfParams) -> Result<(), BackupError> {
    let salt = crypto::random::<16>();
    let header = Header {
        version: 1,
        kdf: Kdf { alg: "argon2id".into(), m_kib: kdf.m_kib, t: kdf.t, p: kdf.p, salt: salt.to_vec() },
        stream_id: crypto::random::<16>().to_vec(),
        created_at: crate::entries::now_ms(),
    };
    let key = derive(passphrase, &header.kdf)?;
    let header_json = serde_json::to_vec(&header).expect("header serializes");

    // A consistent, still-encrypted copy of the live DB (never the raw file).
    let db_copy = out.with_extension("db.tmp");
    journal.backup_to(&db_copy)?;
    let result = (|| {
        let tmp = out.with_extension("vjbak.tmp");
        let mut file = BufWriter::new(File::create(&tmp)?);
        file.write_all(MAGIC)?;
        file.write_all(&(header_json.len() as u32).to_le_bytes())?;
        file.write_all(&header_json)?;

        let mut chunks = ChunkWriter::new(file, key, aad_prefix(&header_json), &header.stream_id);
        {
            let mut tar = tar::Builder::new(&mut chunks);
            tar.mode(tar::HeaderMode::Deterministic);
            tar.append_path_with_name(data_dir.join(VAULT), VAULT)?;
            tar.append_path_with_name(&db_copy, DB)?;
            if let Ok(dir) = fs::read_dir(data_dir.join(AUDIO)) {
                for entry in dir {
                    let entry = entry?;
                    let name = entry.file_name().into_string().map_err(|_| io::Error::other("non-UTF-8 audio file name"))?;
                    tar.append_path_with_name(entry.path(), format!("{AUDIO}/{name}"))?;
                }
            }
            tar.finish()?;
        }
        let mut file = chunks.finish()?;
        file.flush()?;
        file.get_ref().sync_all()?;
        drop(file);
        fs::rename(&tmp, out)?;
        Ok(())
    })();
    let _ = fs::remove_file(&db_copy);
    result
}

/// Replaces the journal in `data_dir` with the backup's. The caller makes sure
/// the journal is locked (DB closed) and nothing is recording. Drafts left
/// by the replaced vault can't be opened afterwards, so they are removed.
pub fn restore(file: &Path, passphrase: &SecretString, data_dir: &Path) -> Result<(), BackupError> {
    let mut input = BufReader::new(File::open(file)?);
    let mut magic = [0u8; 8];
    let mut len = [0u8; 4];
    if input.read_exact(&mut magic).is_err() || &magic != MAGIC || input.read_exact(&mut len).is_err() {
        return Err(BackupError::NotABackup);
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > 64 * 1024 {
        return Err(BackupError::NotABackup);
    }
    let mut header_json = vec![0u8; len];
    input.read_exact(&mut header_json).map_err(|_| BackupError::NotABackup)?;
    let header: Header = serde_json::from_slice(&header_json).map_err(|_| BackupError::NotABackup)?;
    if header.version != 1 {
        return Err(BackupError::TooNew);
    }
    if header.kdf.alg != "argon2id" || header.stream_id.len() != 16 {
        return Err(BackupError::NotABackup);
    }
    let key = derive(passphrase, &header.kdf)?;

    fs::create_dir_all(data_dir)?;
    let staging = data_dir.join(".restoring");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(staging.join(AUDIO))?;
    let unpacked = (|| {
        let mut chunks = ChunkReader::new(input, key, aad_prefix(&header_json), &header.stream_id);
        let mut tar = tar::Archive::new(&mut chunks);
        for entry in tar.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            let dest = allowed(&path).ok_or(BackupError::Tampered)?;
            entry.unpack(staging.join(dest))?;
        }
        // Everything after the archive must still authenticate, through the final chunk.
        io::copy(&mut chunks, &mut io::sink())?;
        if !chunks.finished {
            return Err(BackupError::Tampered);
        }
        if !staging.join(VAULT).exists() || !staging.join(DB).exists() {
            return Err(BackupError::Tampered);
        }
        Ok(())
    })();
    if let Err(e) = unpacked {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    // Swap: old journal out, restored one in.
    for name in [VAULT, DB, "journal.db-wal", "journal.db-shm"] {
        remove_path(&data_dir.join(name))?;
    }
    remove_path(&data_dir.join(AUDIO))?;
    remove_path(&data_dir.join("drafts"))?;
    fs::rename(staging.join(DB), data_dir.join(DB))?;
    fs::rename(staging.join(AUDIO), data_dir.join(AUDIO))?;
    // vault.json last: until it is in place, the app sees no vault at all
    // rather than a vault that doesn't match its DB.
    fs::rename(staging.join(VAULT), data_dir.join(VAULT))?;
    fs::remove_dir_all(&staging)?;
    Ok(())
}

/// Maps an archive path to its place in the data dir, or None if it isn't
/// one the app writes (this also blocks path traversal).
fn allowed(path: &Path) -> Option<PathBuf> {
    let parts: Vec<&str> = path
        .components()
        .map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [VAULT] | [DB] => Some(PathBuf::from(parts[0])),
        [AUDIO, name] if name.ends_with(".opus.enc") => Some(Path::new(AUDIO).join(name)),
        _ => None,
    }
}

fn remove_path(path: &Path) -> io::Result<()> {
    let result = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
    match result {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn derive(passphrase: &SecretString, kdf: &Kdf) -> Result<Key, BackupError> {
    crypto::argon2id(passphrase.expose_secret().as_bytes(), &kdf.salt, kdf.m_kib, kdf.t, kdf.p).map_err(|_| BackupError::NotABackup)
}

/// AAD = magic ‖ header_len ‖ header, followed per chunk by the final flag.
fn aad_prefix(header_json: &[u8]) -> Vec<u8> {
    let mut aad = MAGIC.to_vec();
    aad.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
    aad.extend_from_slice(header_json);
    aad
}

fn nonce(stream_id: &[u8], counter: u64) -> [u8; 24] {
    let mut n = [0u8; 24];
    n[..16].copy_from_slice(stream_id);
    n[16..].copy_from_slice(&counter.to_le_bytes());
    n
}

struct ChunkWriter<W: Write> {
    out: W,
    key: Key,
    aad: Vec<u8>,
    stream_id: Vec<u8>,
    counter: u64,
    buf: Zeroizing<Vec<u8>>,
}

impl<W: Write> ChunkWriter<W> {
    fn new(out: W, key: Key, aad: Vec<u8>, stream_id: &[u8]) -> Self {
        Self { out, key, aad, stream_id: stream_id.to_vec(), counter: 0, buf: Zeroizing::new(Vec::with_capacity(CHUNK)) }
    }

    fn seal(&mut self, last: bool) -> io::Result<()> {
        let mut aad = self.aad.clone();
        aad.push(last as u8);
        let ct = crypto::seal_with_nonce(&self.key, &nonce(&self.stream_id, self.counter), &aad, &self.buf);
        self.out.write_all(&(ct.len() as u32).to_le_bytes())?;
        self.out.write_all(&ct)?;
        self.buf.clear();
        self.counter += 1;
        Ok(())
    }

    fn finish(mut self) -> io::Result<W> {
        if !self.buf.is_empty() {
            self.seal(false)?;
        }
        self.seal(true)?; // empty final chunk
        Ok(self.out)
    }
}

impl<W: Write> Write for ChunkWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let take = (CHUNK - self.buf.len()).min(data.len());
        self.buf.extend_from_slice(&data[..take]);
        if self.buf.len() == CHUNK {
            self.seal(false)?;
        }
        Ok(take)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

struct ChunkReader<R: Read> {
    input: R,
    key: Key,
    aad: Vec<u8>,
    stream_id: Vec<u8>,
    counter: u64,
    plain: Zeroizing<Vec<u8>>,
    at: usize,
    finished: bool,
}

impl<R: Read> ChunkReader<R> {
    fn new(input: R, key: Key, aad: Vec<u8>, stream_id: &[u8]) -> Self {
        Self { input, key, aad, stream_id: stream_id.to_vec(), counter: 0, plain: Zeroizing::new(Vec::new()), at: 0, finished: false }
    }

    fn failure(&self) -> io::Error {
        let kind = if self.counter == 0 { ChunkFailure::First } else { ChunkFailure::Later };
        io::Error::new(io::ErrorKind::InvalidData, kind)
    }

    /// Reads and opens the next chunk. Returns false at the final chunk.
    fn next_chunk(&mut self) -> io::Result<bool> {
        let mut len = [0u8; 4];
        if self.input.read_exact(&mut len).is_err() {
            return Err(self.failure()); // truncated: no final chunk
        }
        let len = u32::from_le_bytes(len) as usize;
        if len > CHUNK + 16 {
            return Err(self.failure());
        }
        let mut ct = vec![0u8; len];
        self.input.read_exact(&mut ct).map_err(|_| self.failure())?;
        let n = nonce(&self.stream_id, self.counter);
        for last in [false, true] {
            let mut aad = self.aad.clone();
            aad.push(last as u8);
            if let Some(plain) = crypto::open(&self.key, &n, &aad, &ct) {
                self.counter += 1;
                if last {
                    let mut rest = [0u8; 1];
                    if !plain.is_empty() || self.input.read(&mut rest)? != 0 {
                        return Err(self.failure()); // data after the final chunk
                    }
                    self.finished = true;
                    return Ok(false);
                }
                self.plain = plain;
                self.at = 0;
                return Ok(true);
            }
        }
        Err(self.failure())
    }
}

impl<R: Read> Read for ChunkReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.at == self.plain.len() {
            if self.finished || !self.next_chunk()? {
                return Ok(0);
            }
        }
        let n = out.len().min(self.plain.len() - self.at);
        out[..n].copy_from_slice(&self.plain[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}
