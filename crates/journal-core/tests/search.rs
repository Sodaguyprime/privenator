//! Seam: full-text search with filters (docs/SPEC.md "Search and backup").

use journal_core::journal::{Journal, NewEntry, SearchQuery, new_entry_id};
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };
const DAY: i64 = 86_400_000;

fn journal() -> (tempfile::TempDir, Journal) {
    let dir = tempfile::tempdir().unwrap();
    let pw = SecretString::from("correct horse battery staple".to_owned());
    let (session, _) = Vault::at(dir.path()).create(&pw, FAST).unwrap();
    let journal = Journal::open(dir.path(), &session).unwrap();
    (dir, journal)
}

fn add(j: &Journal, text: &str, day: i64, mood: Option<u8>, tags: &[&str]) -> String {
    let id = new_entry_id();
    j.create_entry(&NewEntry {
        id: id.clone(),
        created_at: day * DAY,
        text: text.into(),
        duration_s: 10,
        has_audio: false,
        mood,
        tags: tags.iter().map(|t| t.to_string()).collect(),
    })
    .unwrap();
    id
}

fn ids(j: &Journal, q: &SearchQuery) -> Vec<String> {
    j.search(q).unwrap().into_iter().map(|h| h.entry.id).collect()
}

fn text(q: &str) -> SearchQuery {
    SearchQuery { text: q.into(), ..Default::default() }
}

#[test]
fn finds_word_forms_in_text_edits_and_tags_and_highlights_them() {
    let (_dir, j) = journal();
    let walk = add(&j, "I walked to the harbour after lunch.", 1, None, &[]);
    let edited = add(&j, "Something about nothing.", 2, None, &[]);
    j.set_edited_text(&edited, "A long walk in the rain.").unwrap();
    let tagged = add(&j, "No match in the text here.", 3, None, &["walking"]);
    add(&j, "Unrelated entry about cooking.", 4, None, &[]);

    let mut found = ids(&j, &text("walking"));
    found.sort();
    let mut expected = vec![walk.clone(), edited, tagged];
    expected.sort();
    assert_eq!(found, expected);

    let hit = j.search(&text("harbour")).unwrap().remove(0);
    assert_eq!(hit.entry.id, walk);
    assert!(hit.snippet.contains("\u{E000}harbour\u{E001}"), "{:?}", hit.snippet);
}

#[test]
fn filters_by_date_range_mood_and_tag_with_or_without_text() {
    let (_dir, j) = journal();
    let a = add(&j, "Good day at work.", 10, Some(5), &["work"]);
    let b = add(&j, "Hard day at work.", 11, Some(2), &["work"]);
    let c = add(&j, "Good day at home.", 20, Some(5), &["home"]);

    let in_range = SearchQuery { from: Some(10 * DAY), to: Some(12 * DAY), ..Default::default() };
    assert_eq!(ids(&j, &in_range), [b.clone(), a.clone()]);

    let happy = SearchQuery { mood: Some(5), ..Default::default() };
    assert_eq!(ids(&j, &happy), [c.clone(), a.clone()]);

    let work_good = SearchQuery { text: "good".into(), tag: Some("Work".into()), ..Default::default() };
    assert_eq!(ids(&j, &work_good), [a]);
}

#[test]
fn odd_input_never_breaks_the_query() {
    let (_dir, j) = journal();
    let id = add(&j, "Quotes \"and\" stars * and NEAR(things).", 1, None, &[]);
    for q in ["\"", "*", "NEAR(", "and OR not", "stars*", "-", "(", "   "] {
        j.search(&text(q)).unwrap_or_else(|e| panic!("{q:?}: {e}"));
    }
    assert_eq!(ids(&j, &text("sta")), [id], "prefix match while typing");
}

#[test]
fn deleted_entries_leave_the_index() {
    let (_dir, j) = journal();
    let id = add(&j, "Ephemeral thought.", 1, None, &["fleeting"]);
    j.delete_entry(&id).unwrap();
    assert!(ids(&j, &text("ephemeral")).is_empty());
    assert!(ids(&j, &text("fleeting")).is_empty());
}
