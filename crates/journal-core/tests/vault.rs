//! Seam: the vault lifecycle (create, unlock, recover, change password),
//! exercised against a real temporary app-data folder.

use journal_core::vault::{KdfParams, RecoveryKey, Vault, VaultError};
use secrecy::SecretString;

const PASSWORD: &str = "correct horse battery staple";

/// Cheap Argon2 cost so the suite stays fast. Production uses KdfParams::default().
const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn pw(s: &str) -> SecretString {
    SecretString::from(s.to_owned())
}

#[test]
fn unlocks_with_the_password_it_was_created_with() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    vault.create(&pw(PASSWORD), FAST).unwrap();

    assert!(vault.unlock(&pw(PASSWORD)).is_ok());
}

#[test]
fn rejects_a_wrong_password() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    vault.create(&pw(PASSWORD), FAST).unwrap();

    let err = vault.unlock(&pw("correct horse battery stapler")).err().unwrap();
    assert!(matches!(err, VaultError::WrongPassword));
}

#[test]
fn refuses_weak_passwords_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());

    // Long enough, but a common phrase (zxcvbn score < 3).
    assert!(matches!(vault.create(&pw("passwordpassword"), FAST), Err(VaultError::WeakPassword(_))));
    // Strong-looking, but under 12 characters.
    assert!(matches!(vault.create(&pw("Xq7#vT2!kp"), FAST), Err(VaultError::WeakPassword(_))));
    assert!(!vault.exists());
}

const NEW_PASSWORD: &str = "lantern orchard velvet quarry";

#[test]
fn recovery_key_sets_a_new_password_and_is_then_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    let (_, key) = vault.create(&pw(PASSWORD), FAST).unwrap();

    // The user types the key back as shown on screen.
    let typed = RecoveryKey::parse(&key.to_string()).unwrap();
    let (_, new_key) = vault.recover(&typed, &pw(NEW_PASSWORD)).unwrap();

    assert!(vault.unlock(&pw(NEW_PASSWORD)).is_ok());
    assert!(matches!(vault.unlock(&pw(PASSWORD)), Err(VaultError::WrongPassword)));
    assert!(matches!(vault.recover(&typed, &pw(NEW_PASSWORD)), Err(VaultError::WrongRecoveryKey)));
    assert!(vault.recover(&new_key, &pw(PASSWORD)).is_ok());
}

#[test]
fn recovery_key_is_forgiving_about_how_it_is_typed() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    let (_, key) = vault.create(&pw(PASSWORD), FAST).unwrap();

    let shown = key.to_string();
    assert_eq!(shown.len(), 26 + 6, "26 Crockford chars in groups of 4: {shown}");
    let sloppy = shown.replace('-', " ").to_lowercase().replace('0', "o").replace('1', "l");

    assert!(vault.recover(&RecoveryKey::parse(&sloppy).unwrap(), &pw(NEW_PASSWORD)).is_ok());
    assert!(RecoveryKey::parse("not a key").is_err());
}

#[test]
fn changing_the_password_keeps_the_recovery_key_working() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    let (session, key) = vault.create(&pw(PASSWORD), FAST).unwrap();

    vault.change_password(&session, &pw(NEW_PASSWORD)).unwrap();

    assert!(vault.unlock(&pw(NEW_PASSWORD)).is_ok());
    assert!(matches!(vault.unlock(&pw(PASSWORD)), Err(VaultError::WrongPassword)));
    assert!(matches!(vault.change_password(&session, &pw("tooshort")), Err(VaultError::WeakPassword(_))));
    assert!(vault.recover(&key, &pw(PASSWORD)).is_ok());
}

#[test]
fn regenerating_the_recovery_key_retires_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    let (session, old_key) = vault.create(&pw(PASSWORD), FAST).unwrap();

    let new_key = vault.regenerate_recovery_key(&session).unwrap();

    assert!(matches!(vault.recover(&old_key, &pw(NEW_PASSWORD)), Err(VaultError::WrongRecoveryKey)));
    assert!(vault.recover(&new_key, &pw(NEW_PASSWORD)).is_ok());
}
