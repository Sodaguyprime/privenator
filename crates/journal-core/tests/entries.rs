//! Seam: journal entries in the encrypted DB (docs/SPEC.md "Entries").

use journal_core::journal::{Journal, NewEntry, new_entry_id};
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn journal() -> (tempfile::TempDir, Journal) {
    let dir = tempfile::tempdir().unwrap();
    let pw = SecretString::from("correct horse battery staple".to_owned());
    let (session, _) = Vault::at(dir.path()).create(&pw, FAST).unwrap();
    let journal = Journal::open(dir.path(), &session).unwrap();
    (dir, journal)
}

fn spoken(text: &str, created_at: i64) -> NewEntry {
    NewEntry {
        id: new_entry_id(),
        created_at,
        text: text.into(),
        duration_s: 60,
        has_audio: false,
        mood: None,
        tags: vec![],
    }
}

#[test]
fn a_new_entry_is_titled_by_its_first_six_words_and_listed_newest_first() {
    let (_dir, j) = journal();
    let older = spoken("Slept badly but the morning walk helped a lot today.", 1_000);
    let newer = spoken("Short one.", 2_000);
    j.create_entry(&older).unwrap();
    j.create_entry(&newer).unwrap();

    let list = j.list_entries().unwrap();
    assert_eq!(list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), [newer.id.as_str(), older.id.as_str()]);
    assert_eq!(list[1].title.as_deref(), Some("Slept badly but the morning walk"));
    assert_eq!(list[0].title.as_deref(), Some("Short one."));

    let entry = j.get_entry(&older.id).unwrap().unwrap();
    assert_eq!(entry.original_text, older.text);
    assert_eq!(entry.edited_text, None);
}

#[test]
fn editing_keeps_the_original_and_revert_restores_it() {
    let (_dir, j) = journal();
    let e = spoken("I went to the the shop.", 1_000);
    j.create_entry(&e).unwrap();

    j.set_edited_text(&e.id, "I went to the shop.").unwrap();
    let edited = j.get_entry(&e.id).unwrap().unwrap();
    assert_eq!(edited.original_text, "I went to the the shop.");
    assert_eq!(edited.edited_text.as_deref(), Some("I went to the shop."));

    // Saving text identical to the original is the same as no edit.
    j.set_edited_text(&e.id, "I went to the the shop.").unwrap();
    assert_eq!(j.get_entry(&e.id).unwrap().unwrap().edited_text, None);

    j.set_edited_text(&e.id, "Second version.").unwrap();
    j.revert_entry(&e.id).unwrap();
    assert_eq!(j.get_entry(&e.id).unwrap().unwrap().edited_text, None);
}

#[test]
fn tags_are_normalized_and_mood_is_one_to_five() {
    let (_dir, j) = journal();
    let e = spoken("Tagged.", 1_000);
    j.create_entry(&e).unwrap();

    j.set_tags(&e.id, &["  Work ".into(), "work".into(), "Family".into(), " ".into()]).unwrap();
    assert_eq!(j.get_entry(&e.id).unwrap().unwrap().summary.tags, ["family", "work"]);
    assert_eq!(j.all_tags().unwrap(), ["family", "work"]);

    j.set_tags(&e.id, &["family".into()]).unwrap();
    assert_eq!(j.get_entry(&e.id).unwrap().unwrap().summary.tags, ["family"]);
    assert_eq!(j.all_tags().unwrap(), ["family"], "unused tags are dropped");

    j.set_mood(&e.id, Some(4)).unwrap();
    assert_eq!(j.get_entry(&e.id).unwrap().unwrap().summary.mood, Some(4));
    assert!(j.set_mood(&e.id, Some(6)).is_err());
    j.set_mood(&e.id, None).unwrap();
    assert_eq!(j.get_entry(&e.id).unwrap().unwrap().summary.mood, None);
}

#[test]
fn a_deleted_entry_is_gone() {
    let (_dir, j) = journal();
    let e = spoken("Delete me.", 1_000);
    j.create_entry(&e).unwrap();
    j.set_tags(&e.id, &["temp".into()]).unwrap();

    assert!(j.delete_entry(&e.id).unwrap());
    assert!(j.get_entry(&e.id).unwrap().is_none());
    assert!(j.list_entries().unwrap().is_empty());
    assert!(j.all_tags().unwrap().is_empty());
    assert!(!j.delete_entry(&e.id).unwrap());
}
