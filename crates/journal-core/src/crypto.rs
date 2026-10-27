//! Small, safe wrappers over the primitives in docs/SPEC.md ("Keys").
//! Nothing here knows about files; callers own formats.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

pub type Key = Zeroizing<[u8; 32]>;

/// Bytes from the OS CSPRNG. Panics only if the OS has no randomness source,
/// in which case nothing secure is possible anyway.
pub fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::getrandom(&mut out).expect("OS random source unavailable");
    out
}

pub fn random_key() -> Key {
    Zeroizing::new(random())
}

pub struct Sealed {
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

/// XChaCha20-Poly1305 with a fresh random nonce.
pub fn seal(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Sealed {
    let nonce = random::<24>();
    let ciphertext = seal_with_nonce(key, &nonce, aad, plaintext);
    Sealed { nonce, ciphertext }
}

pub fn seal_with_nonce(key: &[u8; 32], nonce: &[u8; 24], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(nonce), Payload { msg: plaintext, aad })
        .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory buffers")
}

/// Returns None when authentication fails (wrong key, tampered data or AAD).
pub fn open(key: &[u8; 32], nonce: &[u8; 24], aad: &[u8], ciphertext: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ciphertext, aad })
        .ok()
        .map(Zeroizing::new)
}

/// Opens a 32-byte wrapped key.
pub fn open_key(key: &[u8; 32], nonce: &[u8; 24], aad: &[u8], ciphertext: &[u8]) -> Option<Key> {
    let plain = open(key, nonce, aad, ciphertext)?;
    let bytes: [u8; 32] = plain.as_slice().try_into().ok()?;
    Some(Zeroizing::new(bytes))
}

pub fn argon2id(password: &[u8], salt: &[u8], m_kib: u32, t: u32, p: u32) -> Result<Key, argon2::Error> {
    let params = Params::new(m_kib, t, p, Some(32))?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(password, salt, out.as_mut())?;
    Ok(out)
}

pub fn hkdf_sha256(ikm: &[u8], salt: &[u8], info: &[u8]) -> Key {
    let mut out = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, out.as_mut())
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    out
}
