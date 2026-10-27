//! Seam: the sealed chunk stream used for audio and drafts
//! (docs/SPEC.md "Sealed chunk stream"). Writing needs only the vault's
//! public recording key, so it works while the app is locked.

use journal_core::stream::{self, StreamError, StreamWriter};
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn pw() -> SecretString {
    SecretString::from("correct horse battery staple".to_owned())
}

fn plain(contents: &stream::StreamContents) -> Vec<Vec<u8>> {
    contents.chunks.iter().map(|c| c.to_vec()).collect()
}

#[test]
fn chunks_written_while_locked_are_readable_after_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    drop(vault.create(&pw(), FAST).unwrap()); // create, then lock
    let path = dir.path().join("draft.bin");

    let mut w = StreamWriter::create(&path, &vault.recording_public_key().unwrap()).unwrap();
    w.append(b"first segment").unwrap();
    w.append(b"").unwrap();
    w.append(b"third segment").unwrap();
    w.finish().unwrap();

    let session = vault.unlock(&pw()).unwrap();
    let contents = stream::read(&path, &session).unwrap();
    assert!(contents.finished);
    assert_eq!(plain(&contents), vec![b"first segment".to_vec(), b"".to_vec(), b"third segment".to_vec()]);
}

#[test]
fn an_unfinished_stream_keeps_its_complete_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    let (session, _) = vault.create(&pw(), FAST).unwrap();
    let path = dir.path().join("crashed.bin");

    let mut w = StreamWriter::create(&path, &vault.recording_public_key().unwrap()).unwrap();
    w.append(b"saved before the crash").unwrap();
    drop(w); // no finish(): simulates a crash
    // A torn write at the end, as a crash mid-append would leave.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(&[200, 0, 0, 0, 1, 2, 3]);
    std::fs::write(&path, bytes).unwrap();

    let contents = stream::read(&path, &session).unwrap();
    assert!(!contents.finished);
    assert_eq!(plain(&contents), vec![b"saved before the crash".to_vec()]);
}

#[test]
fn tampering_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    let (session, _) = vault.create(&pw(), FAST).unwrap();
    let path = dir.path().join("audio.bin");

    let mut w = StreamWriter::create(&path, &vault.recording_public_key().unwrap()).unwrap();
    w.append(b"one").unwrap();
    w.append(b"two").unwrap();
    w.finish().unwrap();

    let original = std::fs::read(&path).unwrap();
    let mut flipped = original.clone();
    let last = flipped.len() - 21; // inside chunk "two" (the final chunk is the last 20 bytes)
    flipped[last] ^= 1;
    std::fs::write(&path, &flipped).unwrap();
    assert!(matches!(stream::read(&path, &session), Err(StreamError::Tampered)));
}

#[test]
fn another_vault_cannot_read_it() {
    let dir = tempfile::tempdir().unwrap();
    let mine = Vault::at(dir.path().join("mine"));
    let theirs = Vault::at(dir.path().join("theirs"));
    drop(mine.create(&pw(), FAST).unwrap());
    let (their_session, _) = theirs.create(&pw(), FAST).unwrap();
    let path = dir.path().join("audio.bin");

    let mut w = StreamWriter::create(&path, &mine.recording_public_key().unwrap()).unwrap();
    w.append(b"private").unwrap();
    w.finish().unwrap();

    assert!(matches!(stream::read(&path, &their_session), Err(StreamError::WrongKey)));
}
