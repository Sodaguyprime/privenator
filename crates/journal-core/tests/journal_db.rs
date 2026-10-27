//! Seam: opening the SQLCipher journal with the vault's data key.

use std::time::Duration;

use journal_core::journal::{Journal, JournalError};
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;
use serde_json::json;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn pw() -> SecretString {
    SecretString::from("correct horse battery staple".to_owned())
}

#[test]
fn data_survives_lock_and_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    {
        let (session, _) = vault.create(&pw(), FAST).unwrap();
        let journal = Journal::open(dir.path(), &session).unwrap();
        journal.set_setting("idle_timeout_min", &json!(15)).unwrap();
    } // lock

    let session = vault.unlock(&pw()).unwrap();
    let journal = Journal::open(dir.path(), &session).unwrap();
    assert_eq!(journal.get_setting("idle_timeout_min").unwrap(), Some(json!(15)));
    assert_eq!(journal.get_setting("never_set").unwrap(), None);
}

#[test]
fn the_database_file_is_not_readable_without_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    let (session, _) = vault.create(&pw(), FAST).unwrap();
    let journal = Journal::open(dir.path(), &session).unwrap();
    journal.set_setting("mic_device", &json!("A VERY RECOGNISABLE MICROPHONE NAME")).unwrap();
    drop(journal);

    let bytes = std::fs::read(dir.path().join("journal.db")).unwrap();
    assert!(!bytes.starts_with(b"SQLite format 3"));
    assert!(!bytes.windows(20).any(|w| w == b"RECOGNISABLE MICROPH"));
}

#[test]
fn another_vaults_key_cannot_open_it() {
    let dir = tempfile::tempdir().unwrap();
    let mine = Vault::at(dir.path().join("mine"));
    let theirs = Vault::at(dir.path().join("theirs"));
    let (my_session, _) = mine.create(&pw(), FAST).unwrap();
    let (their_session, _) = theirs.create(&pw(), FAST).unwrap();
    drop(Journal::open(mine.dir(), &my_session).unwrap());

    assert!(matches!(Journal::open(mine.dir(), &their_session), Err(JournalError::WrongKey)));
}

fn fresh_journal() -> (tempfile::TempDir, Journal) {
    let dir = tempfile::tempdir().unwrap();
    let (session, _) = Vault::at(dir.path()).create(&pw(), FAST).unwrap();
    let journal = Journal::open(dir.path(), &session).unwrap();
    (dir, journal)
}

#[test]
fn idle_timeout_defaults_to_five_minutes() {
    let (_dir, journal) = fresh_journal();
    assert_eq!(journal.idle_timeout().unwrap(), Duration::from_secs(5 * 60));
}

#[test]
fn idle_timeout_uses_an_allowed_stored_value_and_ignores_anything_else() {
    let (_dir, journal) = fresh_journal();
    let five = Duration::from_secs(5 * 60);

    journal.set_setting("idle_timeout_min", &json!(15)).unwrap();
    assert_eq!(journal.idle_timeout().unwrap(), Duration::from_secs(15 * 60));

    for junk in [json!(0), json!(7), json!(-1), json!(2.5), json!("15"), json!(null)] {
        journal.set_setting("idle_timeout_min", &junk).unwrap();
        assert_eq!(journal.idle_timeout().unwrap(), five, "for {junk}");
    }
}
