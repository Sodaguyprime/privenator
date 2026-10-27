//! The vault: one random data key (DEK), stored only wrapped, in `vault.json`.
//! Format and rules: docs/SPEC.md ("Security model", "vault.json").

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::crypto::{self, Key};
pub use crate::recovery::{InvalidRecoveryKey, RecoveryKey};

const VAULT_FILE: &str = "vault.json";
const AAD_PASSWORD: &[u8] = b"dek_by_password";
const AAD_RECOVERY: &[u8] = b"dek_by_recovery";
const AAD_RECORDING: &[u8] = b"recording_key";
const RECOVERY_INFO: &str = "privenator/v1/recovery-wrap";

/// Argon2id cost. Stored in vault.json, so each vault can use its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for KdfParams {
    /// Starting point from the spec; tune per platform (0.5–1 s on the slowest target).
    fn default() -> Self {
        Self { m_kib: 256 * 1024, t: 3, p: 1 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("wrong password")]
    WrongPassword,
    #[error("wrong recovery key")]
    WrongRecoveryKey,
    #[error("password too weak: {0}")]
    WeakPassword(String),
    #[error("a vault already exists here")]
    AlreadyExists,
    #[error("no vault exists here")]
    NotFound,
    #[error("vault file is corrupt: {0}")]
    Corrupt(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// An unlocked vault. Holds the DEK and the recording secret key in zeroizing
/// memory; dropping the session is locking.
pub struct Session {
    dek: Key,
    recording_secret: StaticSecret,
}

impl Session {
    fn open(dek: Key, file: &VaultFile) -> Result<Self, VaultError> {
        let slot = &file.recording_key.secret_wrapped;
        let secret = crypto::open_key(&dek, &nonce24(&slot.nonce)?, AAD_RECORDING, &slot.ciphertext)
            .ok_or_else(|| VaultError::Corrupt("recording key does not match the data key".into()))?;
        Ok(Self { dek, recording_secret: StaticSecret::from(*secret) })
    }

    pub(crate) fn dek(&self) -> &Key {
        &self.dek
    }

    pub(crate) fn recording_secret(&self) -> &StaticSecret {
        &self.recording_secret
    }
}

/// Public half of the recording keypair. Not secret: it is what lets a
/// recording keep saving while the vault is locked.
#[derive(Clone)]
pub struct RecordingPublicKey(pub(crate) PublicKey);

pub struct Vault {
    dir: PathBuf,
}

impl Vault {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn exists(&self) -> bool {
        self.file().exists()
    }

    /// First run. Returns the open session and the recovery key to show once.
    pub fn create(&self, password: &SecretString, kdf: KdfParams) -> Result<(Session, RecoveryKey), VaultError> {
        if self.exists() {
            return Err(VaultError::AlreadyExists);
        }
        ensure_strong(password)?;
        let dek = crypto::random_key();
        let recovery = RecoveryKey::generate();
        let file = VaultFile {
            version: 1,
            dek_by_password: wrap_with_password(&dek, password, kdf)?,
            dek_by_recovery: wrap_with_recovery(&dek, &recovery),
            recording_key: new_recording_key(&dek),
        };
        self.write(&file)?;
        Ok((Session::open(dek, &file)?, recovery))
    }

    pub fn unlock(&self, password: &SecretString) -> Result<Session, VaultError> {
        let file = self.read()?;
        let slot = &file.dek_by_password;
        let pwk = derive_password_key(password, &slot.kdf)?;
        let dek = crypto::open_key(&pwk, &nonce24(&slot.nonce)?, AAD_PASSWORD, &slot.ciphertext)
            .ok_or(VaultError::WrongPassword)?;
        Session::open(dek, &file)
    }

    /// Forgot-password path: the recovery key opens the vault, a new password
    /// is set, and a new recovery key replaces the used one.
    pub fn recover(&self, key: &RecoveryKey, new_password: &SecretString) -> Result<(Session, RecoveryKey), VaultError> {
        let mut file = self.read()?;
        let slot = &file.dek_by_recovery;
        let rwk = derive_recovery_key(key, &slot.kdf)?;
        let dek = crypto::open_key(&rwk, &nonce24(&slot.nonce)?, AAD_RECOVERY, &slot.ciphertext)
            .ok_or(VaultError::WrongRecoveryKey)?;
        ensure_strong(new_password)?;

        let kdf = file.dek_by_password.kdf.params();
        let new_key = RecoveryKey::generate();
        file.dek_by_password = wrap_with_password(&dek, new_password, kdf)?;
        file.dek_by_recovery = wrap_with_recovery(&dek, &new_key);
        self.write(&file)?;
        Ok((Session::open(dek, &file)?, new_key))
    }

    /// Re-wraps the DEK under a new password (fresh salt, same cost).
    /// Nothing else is re-encrypted; the recovery key is untouched.
    pub fn change_password(&self, session: &Session, new_password: &SecretString) -> Result<(), VaultError> {
        ensure_strong(new_password)?;
        let mut file = self.read()?;
        let kdf = file.dek_by_password.kdf.params();
        file.dek_by_password = wrap_with_password(&session.dek, new_password, kdf)?;
        self.write(&file)
    }

    /// Settings → "Regenerate recovery key": a new key replaces the old one,
    /// which stops working. Show the returned key once.
    pub fn regenerate_recovery_key(&self, session: &Session) -> Result<RecoveryKey, VaultError> {
        let mut file = self.read()?;
        let key = RecoveryKey::generate();
        file.dek_by_recovery = wrap_with_recovery(&session.dek, &key);
        self.write(&file)?;
        Ok(key)
    }

    /// Readable while locked, so recordings can continue after a lock.
    pub fn recording_public_key(&self) -> Result<RecordingPublicKey, VaultError> {
        let bytes: [u8; 32] = self.read()?.recording_key.public.as_slice().try_into()
            .map_err(|_| VaultError::Corrupt("recording public key must be 32 bytes".into()))?;
        Ok(RecordingPublicKey(PublicKey::from(bytes)))
    }

    fn file(&self) -> PathBuf {
        self.dir.join(VAULT_FILE)
    }

    fn read(&self) -> Result<VaultFile, VaultError> {
        let bytes = match fs::read(self.file()) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(VaultError::NotFound),
            Err(e) => return Err(e.into()),
        };
        let file: VaultFile = serde_json::from_slice(&bytes).map_err(|e| VaultError::Corrupt(e.to_string()))?;
        if file.version != 1 {
            return Err(VaultError::Corrupt(format!("unsupported version {}", file.version)));
        }
        Ok(file)
    }

    /// Atomic replace: temp file in the same folder, fsync, rename.
    fn write(&self, file: &VaultFile) -> Result<(), VaultError> {
        fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!("{VAULT_FILE}.tmp"));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&serde_json::to_vec_pretty(file).expect("vault serializes"))?;
            f.sync_all()?;
        }
        fs::rename(&tmp, self.file())?;
        Ok(())
    }
}

fn ensure_strong(password: &SecretString) -> Result<(), VaultError> {
    let s = crate::password::strength(password.expose_secret());
    if s.acceptable { Ok(()) } else { Err(VaultError::WeakPassword(s.feedback.unwrap_or_default())) }
}

fn wrap_with_password(dek: &Key, password: &SecretString, kdf: KdfParams) -> Result<PasswordSlot, VaultError> {
    let kdf = PasswordKdf { alg: "argon2id".into(), m_kib: kdf.m_kib, t: kdf.t, p: kdf.p, salt: crypto::random::<16>().to_vec() };
    let pwk = derive_password_key(password, &kdf)?;
    let sealed = crypto::seal(&pwk, AAD_PASSWORD, dek.as_ref());
    Ok(PasswordSlot { kdf, nonce: sealed.nonce.to_vec(), ciphertext: sealed.ciphertext })
}

fn derive_password_key(password: &SecretString, kdf: &PasswordKdf) -> Result<Key, VaultError> {
    if kdf.alg != "argon2id" {
        return Err(VaultError::Corrupt(format!("unknown kdf {}", kdf.alg)));
    }
    crypto::argon2id(password.expose_secret().as_bytes(), &kdf.salt, kdf.m_kib, kdf.t, kdf.p)
        .map_err(|e| VaultError::Corrupt(format!("argon2 params: {e}")))
}

fn wrap_with_recovery(dek: &Key, key: &RecoveryKey) -> RecoverySlot {
    let kdf = RecoveryKdf { alg: "hkdf-sha256".into(), salt: crypto::random::<16>().to_vec(), info: RECOVERY_INFO.into() };
    let rwk = crypto::hkdf_sha256(key.bytes(), &kdf.salt, kdf.info.as_bytes());
    let sealed = crypto::seal(&rwk, AAD_RECOVERY, dek.as_ref());
    RecoverySlot { kdf, nonce: sealed.nonce.to_vec(), ciphertext: sealed.ciphertext }
}

fn derive_recovery_key(key: &RecoveryKey, kdf: &RecoveryKdf) -> Result<Key, VaultError> {
    if kdf.alg != "hkdf-sha256" {
        return Err(VaultError::Corrupt(format!("unknown kdf {}", kdf.alg)));
    }
    Ok(crypto::hkdf_sha256(key.bytes(), &kdf.salt, kdf.info.as_bytes()))
}

fn new_recording_key(dek: &Key) -> RecordingKeySlot {
    let secret = StaticSecret::from(crypto::random::<32>());
    let public = PublicKey::from(&secret);
    let sealed = crypto::seal(dek, AAD_RECORDING, secret.as_bytes());
    RecordingKeySlot {
        public: public.as_bytes().to_vec(),
        secret_wrapped: Wrapped { nonce: sealed.nonce.to_vec(), ciphertext: sealed.ciphertext },
    }
}

fn nonce24(bytes: &[u8]) -> Result<[u8; 24], VaultError> {
    bytes.try_into().map_err(|_| VaultError::Corrupt("nonce must be 24 bytes".into()))
}

// ---- vault.json (docs/SPEC.md "vault.json"); binary fields are base64 ----

#[derive(Serialize, Deserialize)]
struct VaultFile {
    version: u32,
    dek_by_password: PasswordSlot,
    dek_by_recovery: RecoverySlot,
    recording_key: RecordingKeySlot,
}

#[derive(Serialize, Deserialize)]
struct PasswordSlot {
    kdf: PasswordKdf,
    #[serde(with = "b64")]
    nonce: Vec<u8>,
    #[serde(with = "b64")]
    ciphertext: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct PasswordKdf {
    alg: String,
    m_kib: u32,
    t: u32,
    p: u32,
    #[serde(with = "b64")]
    salt: Vec<u8>,
}

impl PasswordKdf {
    fn params(&self) -> KdfParams {
        KdfParams { m_kib: self.m_kib, t: self.t, p: self.p }
    }
}

#[derive(Serialize, Deserialize)]
struct RecoverySlot {
    kdf: RecoveryKdf,
    #[serde(with = "b64")]
    nonce: Vec<u8>,
    #[serde(with = "b64")]
    ciphertext: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct RecoveryKdf {
    alg: String,
    #[serde(with = "b64")]
    salt: Vec<u8>,
    info: String,
}

#[derive(Serialize, Deserialize)]
struct RecordingKeySlot {
    #[serde(with = "b64")]
    public: Vec<u8>,
    secret_wrapped: Wrapped,
}

#[derive(Serialize, Deserialize)]
struct Wrapped {
    #[serde(with = "b64")]
    nonce: Vec<u8>,
    #[serde(with = "b64")]
    ciphertext: Vec<u8>,
}

pub(crate) mod b64 {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}
