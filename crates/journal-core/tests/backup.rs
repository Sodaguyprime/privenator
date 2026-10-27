//! Seam: the encrypted `.vjbak` backup (docs/SPEC.md ".vjbak backup").

use std::path::Path;

use journal_core::backup::{self, BackupError};
use journal_core::journal::{Journal, NewEntry, new_entry_id};
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn secret(s: &str) -> SecretString {
    SecretString::from(s.to_owned())
}

/// A journal with one entry and one (fake) kept audio file.
fn journal_with_entry(dir: &Path, password: &str, text: &str) -> String {
    let (session, _) = Vault::at(dir).create(&secret(password), FAST).unwrap();
    let journal = Journal::open(dir, &session).unwrap();
    let id = new_entry_id();
    journal
        .create_entry(&NewEntry {
            id: id.clone(),
            created_at: 1,
            text: text.into(),
            duration_s: 1,
            has_audio: true,
            mood: None,
            tags: vec!["kept".into()],
        })
        .unwrap();
    std::fs::create_dir_all(dir.join("audio")).unwrap();
    std::fs::write(dir.join("audio").join(format!("{id}.opus.enc")), b"sealed audio bytes").unwrap();
    id
}

fn backup_of(dir: &Path, password: &str, passphrase: &str, out: &Path) {
    let session = Vault::at(dir).unlock(&secret(password)).unwrap();
    let journal = Journal::open(dir, &session).unwrap();
    backup::create(dir, &journal, out, &secret(passphrase), FAST).unwrap();
}

#[test]
fn a_backup_restores_the_whole_journal_on_another_device() {
    let tmp = tempfile::tempdir().unwrap();
    let (home, phone) = (tmp.path().join("home"), tmp.path().join("phone"));
    let id = journal_with_entry(&home, "correct horse battery staple", "Backed up thought.");
    let file = tmp.path().join("journal.vjbak");
    backup_of(&home, "correct horse battery staple", "backup passphrase here", &file);
    assert!(!std::fs::read(&file).unwrap().windows(8).any(|w| w == b"Backed u"));

    backup::restore(&file, &secret("backup passphrase here"), &phone).unwrap();

    // The restored vault's password applies.
    let session = Vault::at(&phone).unlock(&secret("correct horse battery staple")).unwrap();
    let journal = Journal::open(&phone, &session).unwrap();
    let entry = journal.get_entry(&id).unwrap().unwrap();
    assert_eq!(entry.original_text, "Backed up thought.");
    assert_eq!(entry.summary.tags, ["kept"]);
    assert_eq!(std::fs::read(phone.join("audio").join(format!("{id}.opus.enc"))).unwrap(), b"sealed audio bytes");
}

#[test]
fn the_wrong_passphrase_restores_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (home, target) = (tmp.path().join("home"), tmp.path().join("target"));
    journal_with_entry(&home, "correct horse battery staple", "Secret.");
    let file = tmp.path().join("journal.vjbak");
    backup_of(&home, "correct horse battery staple", "the right passphrase", &file);

    let err = backup::restore(&file, &secret("the wrong passphrase"), &target).unwrap_err();
    assert!(matches!(err, BackupError::WrongPassphrase), "{err}");
    assert!(!target.join("vault.json").exists());
}

#[test]
fn a_tampered_backup_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let id = journal_with_entry(&home, "correct horse battery staple", "Secret.");
    // Big enough to span several 1 MiB chunks, so the damage is past the first.
    std::fs::write(home.join("audio").join(format!("{id}.opus.enc")), vec![7u8; 1_500_000]).unwrap();
    let file = tmp.path().join("journal.vjbak");
    backup_of(&home, "correct horse battery staple", "the passphrase", &file);
    let mut bytes = std::fs::read(&file).unwrap();
    let last = bytes.len() - 100;
    bytes[last] ^= 1;
    std::fs::write(&file, bytes).unwrap();

    let err = backup::restore(&file, &secret("the passphrase"), &tmp.path().join("target")).unwrap_err();
    assert!(matches!(err, BackupError::Tampered), "{err}");
}

#[test]
fn restoring_replaces_the_current_journal() {
    let tmp = tempfile::tempdir().unwrap();
    let (old, current) = (tmp.path().join("old"), tmp.path().join("current"));
    let old_id = journal_with_entry(&old, "the old password!!", "From the backup.");
    let file = tmp.path().join("old.vjbak");
    backup_of(&old, "the old password!!", "the passphrase", &file);
    let current_id = journal_with_entry(&current, "the current password", "Written since.");
    std::fs::create_dir_all(current.join("drafts")).unwrap();
    std::fs::write(current.join("drafts").join("x.draft"), b"old vault draft").unwrap();

    backup::restore(&file, &secret("the passphrase"), &current).unwrap();

    assert!(Vault::at(&current).unlock(&secret("the current password")).is_err());
    let session = Vault::at(&current).unlock(&secret("the old password!!")).unwrap();
    let journal = Journal::open(&current, &session).unwrap();
    assert!(journal.get_entry(&old_id).unwrap().is_some());
    assert!(journal.get_entry(&current_id).unwrap().is_none());
    assert!(!current.join("audio").join(format!("{current_id}.opus.enc")).exists());
    assert!(!current.join("drafts").exists());
}
