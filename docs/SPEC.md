# Private Voice Journal — Spec

Working name: **Privenator** (final name still open).
Originally written 2026-10-13. Revised 2026-10-13 to fix the problems found in review, and again during build steps 2–10 (see [DECISIONS.md](DECISIONS.md)).

This is the **product contract**. It describes behavior, data formats and security rules independently of any platform, so that the desktop app (Tauri) and the planned Android app produce **identical, interchangeable data**. If code and this spec disagree, one of them is a bug: fix it, or update the spec and log the change in DECISIONS.md.

## Overview

A private journal you speak into. You talk for 15 to 30 minutes, and the app transcribes what you say live into an encrypted entry. Everything stays on the device. There are no accounts, no sync, and no network access at runtime.

**In scope (v1, desktop):** one master password with a recovery key, live offline English speech-to-text, multiple entries per day, editable transcripts that keep the original, optional audio per entry, mood and tags, search, and encrypted backup and restore.

**Out of scope (v1):** cloud sync, AI summaries or analysis, multiple users, languages other than English, and plaintext export (Markdown or otherwise).

**Next:** an Android app. It must read desktop backups and the desktop must read its backups. See [ARCHITECTURE.md § Android port](ARCHITECTURE.md#android-port).

## Decisions

| Area | Decision |
| --- | --- |
| Platforms | Windows, macOS and Linux now; Android next |
| Framework | Tauri v2 (it also targets Android) |
| Network | None at runtime; the speech model is bundled with the installer |
| Access | One master password, set at first run |
| Recovery | One-time recovery key shown at setup; the password can be changed |
| Transcription | Live, on-device, English-only Whisper `base.en` (desktop) |
| Audio | Chosen per entry: keep (encrypted) or discard |
| Entries | Any number of separate entries per day |
| Editing | Editable. The original transcript is kept read-only, plus **only the latest edit** (no full history) |
| Auto-lock | On minimize, or after 5 minutes idle (configurable) |
| Recording while locked | Supported: recording continues, writing to a public key (see Security model) |
| Extras | Search, mood and tags, encrypted backup and restore |
| Export | None in v1. Plaintext export would undo the threat model |

## Tech stack

All sensitive work (crypto, audio, transcription, storage) runs in Rust, in the platform-neutral crate `journal-core`. The webview only renders the UI. It never holds the data key, and it only ever holds the plaintext currently on screen.

| Layer | Choice | Why |
| --- | --- | --- |
| App shell | Tauri v2 (Rust core + system webview) | Small binary, strict capability system, Android target |
| Frontend | React + TypeScript + Vite, Tailwind | Fast UI work in a familiar stack |
| Audio capture | `cpal`, downmixed and resampled to 16 kHz mono with `rubato` (FFT resampler) | Cross-platform mic input in Rust (cpal supports Android via AAudio/Oboe) |
| Speech-to-text | `whisper-rs` (whisper.cpp), model `base.en` | Offline and fast enough for real time on a modern CPU |
| Voice activity | Energy-based VAD with an adaptive noise floor (`journal_core::live::Segmenter`) | Splits speech at natural pauses; no extra model file (see DECISIONS.md) |
| Database | SQLite + SQLCipher via `rusqlite` (`bundled-sqlcipher-vendored-openssl`) | Whole DB encrypted, search index included; vendored build works on every target |
| Password KDF | Argon2id (`argon2`) | Memory-hard, resists GPU brute force |
| Recovery-key KDF | HKDF-SHA256 (`hkdf`) | The key is 128 random bits already, so stretching adds nothing |
| Symmetric encryption | XChaCha20-Poly1305 (`chacha20poly1305`) | Authenticated; 192-bit nonces are safe to pick at random |
| Recording key exchange | X25519 (`x25519-dalek`) + HKDF-SHA256 | Lets recording continue while the DEK is wiped |
| Memory hygiene | `zeroize`, `secrecy` | Wipes keys from RAM on lock |
| Password strength | `zxcvbn` | Rejects weak passwords at setup |
| Audio format | Opus (`opus` crate, bundled libopus), 24 kbps, 16 kHz mono, 20 ms frames, in Ogg | 30 minutes is about 5 MB |

Model sizes (approximate): `tiny.en` 75 MB, `base.en` 142 MB, `small.en` 466 MB. **v1 desktop ships `base.en` only.** Offering `small.en` would mean either a roughly 600 MB installer or a runtime download, and a download breaks the no-network rule. Android will probably need `tiny.en` (see ARCHITECTURE.md).

**Network lockdown:** no HTTP, updater, shell or opener plugins. A strict CSP is set (see ARCHITECTURE.md). Tauri capabilities expose only the app's own commands.

## Security model

### Keys

| Key | What it is | Where it lives |
| --- | --- | --- |
| **DEK** | Random 256-bit data key. Opens SQLCipher and unwraps the recording secret key | Only in Rust memory while unlocked; on disk only wrapped |
| **PWK** | Password key: Argon2id(password, salt) | Never stored |
| **RK** | Recovery key: 128 random bits shown to the user once | The user's paper or file; never stored by the app |
| **RWK** | Recovery wrap key: HKDF-SHA256(RK, salt, info `"privenator/v1/recovery-wrap"`) | Never stored |
| **Recording keypair** | X25519 keypair. The public key encrypts drafts and audio; the secret key decrypts them | Public key in `vault.json`; secret key wrapped under the DEK in `vault.json` |
| **CK** | Per-recording 256-bit content key. Encrypts that recording's audio and draft chunks | Sealed to the recording public key in each file header; in memory only while recording |

### First run

1. Generate the DEK with the OS CSPRNG.
2. Derive the PWK with Argon2id. Start at m = 256 MiB, t = 3, p = 1, and tune so that it takes 0.5 to 1 s on the slowest target machine. The parameters are stored, so they can differ per install, and Android can use lower memory.
3. Wrap the DEK under the PWK with XChaCha20-Poly1305.
4. Generate the RK (16 random bytes) and show it once as Crockford base32 (alphabet `0123456789ABCDEFGHJKMNPQRSTVWXYZ`): 26 characters, big-endian, the last character carrying the final 3 bits plus 2 zero bits, in groups of 4 (`K7QF-…-WM`). Derive the RWK and wrap the DEK again.
5. Generate the recording X25519 keypair. Wrap the secret key under the DEK.
6. Write `vault.json` atomically: write a temp file, fsync it, then rename. None of its contents are secret.

### Unlock

Derive the PWK and unwrap the DEK. An authentication failure means the password is wrong. The DEK then opens SQLCipher and unwraps the recording secret key.

### Lock

Zeroize the DEK, the PWK, the recording secret key and any decrypted text buffers. Close the database connection. Tell the UI to clear itself. If a recording is in progress, it keeps going (see below).

### Password change

Unlock, derive a new PWK with a fresh salt, re-wrap the DEK, then atomically replace `vault.json`. Nothing else is re-encrypted.

### Recovery

The RK unwraps the DEK. The user must set a new password straight away. A **new** RK is then generated, shown and wrapped, so a used key can never work again.

### Recording while locked

Locking wipes the DEK, but a recording that is running must keep saving. So recordings never use the DEK directly.

- When a recording starts, generate a random CK. Seal it to the recording public key: ephemeral X25519 keypair, shared secret, then HKDF-SHA256 to get a wrap key, then XChaCha20-Poly1305 (exact parameters under "Sealed chunk stream"). Write the sealed CK into the header of each draft and audio file.
- Audio and finalized transcript segments are written as encrypted chunks under the CK.
- The CK and the live transcript stay in RAM until the recording stops. The secret key that would open **other** recordings stays wiped.
- On the next unlock, the secret key opens the CK, and the draft is turned into a normal entry.

This keeps the threat model: someone who gets the device while it is locked can read nothing at rest. They also cannot read past recordings, even from memory.

### Rules

- The DEK and the recording secret key live only in Rust memory, in zeroizing buffers, and are wiped on lock.
- No plaintext ever touches disk: no temp files, no logs containing journal text, no crash dumps.
  - SQLite: `PRAGMA temp_store = MEMORY`. (`cipher_memory_security` is deliberately **off**; see DECISIONS.md.)
  - Crash dumps are disabled per platform. Windows: exclude the process from WER with `WerAddExcludedApplication` or the equivalent, plus `SetErrorMode`. Linux/Android: `prctl(PR_SET_DUMPABLE, 0)`. macOS: `setrlimit(RLIMIT_CORE, 0)`.
  - The editor sets `spellcheck="false"`, `autocomplete="off"` and `autocorrect="off"`, so OS spell services do not cache text.
  - Logs never contain journal text, passwords or keys. Log events and IDs only.
- Passwords must be at least 12 characters and pass a zxcvbn score of 3 or higher.
- Failed unlock attempts add an increasing delay (1 s, 2 s, 4 s … capped at 30 s). The delay is UX only; the Argon2id cost is the real defense.

### Threat model

This protects journal contents from anyone who gets the device or its files while the app is locked or closed. It does **not** protect against malware or a keylogger on a machine that is already compromised, or against someone at the keyboard while the app is unlocked.

## User flows

### First run

1. A welcome screen explains that everything stays local, and that losing both the password and the recovery key means the data is gone. It also offers "Restore from backup" (passphrase, then a native file picker; afterwards the unlock screen, with the backup's password).
2. Create a password. It is entered twice, with a strength meter.
3. Show the recovery key as 7 numbered groups (I–VII), with "Copy" and "Save as text file" (a native save dialog). To continue, the user types back 2 randomly chosen full groups, e.g. "Group II" and "Group V". Entry is forgiving the same way the parser is: any case, O for 0, I or L for 1.
4. Microphone choice, with a live level meter. Nothing is recorded. If access is blocked, the screen says where the OS setting is. (Skipped after recovery.)
5. Land on an empty journal.

### Unlock

A single password field. Wrong attempts add a delay. A "Forgot password?" link opens recovery.

### Recovery

Enter the recovery key, set a new password, then save the newly generated recovery key, confirming it the same way as at setup.

### Auto-lock

The app locks when the window is minimized (on Android: when it goes to the background) or after an idle timeout. The default is 5 minutes, and Settings offers 1, 5, 15 or 30 minutes. While recording, the idle timer is paused. Minimizing during a recording keeps recording and locks the UI, and the transcript keeps autosaving behind the password.

- **Idle** means no keyboard or pointer input (key press, click, pointer move, scroll) in the app window. The timeout is read from `idle_timeout_min` at unlock; an unset or unsupported value means 5 minutes.
- **Locking** works the same whatever triggers it (idle, minimize, "Lock now"): the keys are wiped and the DB closed first, then the UI is told to drop what it shows and return to the unlock screen.
- **While a new recovery key is on screen** (after setup, recovery, or regenerating it in Settings, until it is confirmed), neither idle nor minimize locks. The key is shown only once and can't be rebuilt, so locking there would lose it.
- **While recording**, idle never locks. Minimize still does: the UI locks and the recording carries on behind it.

### Settings (when unlocked)

Change password (asks for the current password too, so someone at an unattended unlocked machine can't take the journal over), regenerate recovery key (also asks for the current password; the old key stops working and the new one is shown and confirmed as at setup), idle timeout, default audio choice (keep/discard, default discard), microphone selection with a level meter, backup, backup reminder (never, or every 7, 14, 30 or 90 days), and restore. When a reminder is due, the journal shows a notice.

## Recording and transcription

Transcription runs in short segments while you talk. Text appears within a couple of seconds, and a 30-minute session never needs one huge pass at the end.

1. Audio is captured, resampled to 16 kHz mono and held in a rolling in-memory buffer. It is also Opus-encoded and appended to the encrypted draft audio file as it is recorded. Whether to keep the audio is decided at save time; discarded audio is deleted then.
2. **Preview:** every 1 to 2 seconds, Whisper re-transcribes the unfinalized tail (a few seconds of audio) with a reduced `audio_ctx`. That text appears in grey and may still change.
3. **Final:** when VAD detects a pause of about 0.6 s, or a segment reaches about 25 s, that segment is transcribed in full. The result becomes final text and is appended to the encrypted draft.
4. **Priority:** final jobs always run before previews. If final jobs are queued, previews are skipped. On a slow machine, previews update less often, but no final text is ever skipped.
5. **Stop:** remaining audio is finalized, and a save dialog appears (keep audio? mood? tags?). "Keep audio" starts at the `default_keep_audio` setting. The entry is written to the DB with the recording's ID, the draft is deleted, and the audio file is either moved to `audio/` or deleted.

Recording supports pause and resume (the microphone stream is paused, not just ignored). Whisper runs on a background thread, so the UI never freezes.

Details:
- **Segments:** 20 ms frames are speech when they are more than 12 dB above a slowly tracked noise floor and above −45 dBFS. 0.3 s of audio before the first speech frame is kept. A pause of 0.6 s ends a segment; a segment is cut at 25 s regardless. Audio with no speech in it is never transcribed.
- **Previews:** at most every 1.5 s, on the last 8 s of the unfinished segment, once it holds at least 1 s of speech, with `audio_ctx` scaled to the audio length. A final always clears the preview.
- **Whisper:** greedy decoding, English, no context carried between segments. Non-speech markers such as `[BLANK_AUDIO]` are dropped.
- **Transcript text reaches the UI only while unlocked.** While locked it stays in Rust memory; after unlock the UI asks for the finals so far.
- **Crash recovery:** at every unlock, drafts left by a crash (any draft except the recording still running) become entries automatically: the text from the draft's segments, the audio kept or discarded per `default_keep_audio`, `created_at` from the UUID v7 timestamp. A crash-truncated audio stream is rewritten as a finished one before it is kept. The journal shows how many were recovered. A damaged draft is skipped, never blocking the unlock.
- **Stopped but not saved:** if the app locks while the save dialog is open, the in-memory transcript is dropped and the draft is recovered at the next unlock, as above.

## Entries

Each recording session becomes its own entry, and a day can hold any number of them. The sidebar groups entries by local date, newest first, showing time, title, duration and mood.

- **Title:** defaults to the first 6 words of the transcript, and can be edited. A blank title is stored as NULL and shown as "Untitled".
- **Original and edited text:** the original transcript is stored once and never changes. Editing stores the latest edited version only (`edited_text`); saving text identical to the original stores NULL. The editor shows the edited text (or the original, if never edited), with a "View original" toggle and a "Revert to original" action that sets `edited_text` back to NULL.
- **Typed entries:** you can create an entry by typing alone. Its `original_text` is the text as first saved, and `duration_s` is 0.
- **Audio:** kept audio is playable from the entry. It is decrypted in Rust and streamed to the player in memory, never written as a plaintext file.
- **Mood:** optional 5-point scale (1 = very low … 5 = very good), picked at save time or later.
- **Tags:** free text, lowercased and trimmed, with autocomplete from existing tags. An entry can have several. Tags that no entry uses any more are deleted.
- **Delete:** asks for confirmation, then removes the text, tags, audio file and search index row.
- **IDs:** an entry made from a recording has the recording's UUID v7, so its draft and audio files share the name.

## Search and backup

**Search** uses SQLite FTS5 over titles, original text, edited text and tags. The index lives inside the SQLCipher database, so it is encrypted at rest. Results highlight matches and can be filtered by date range, mood and tag.
- Query: the input is split into words (letters and digits only; FTS syntax in the input is ignored). Every word must match, as a prefix, in its stemmed form, so "walking" finds "walked" and "sta" finds "stars".
- Results are newest first. Each has a snippet around the best match, with matches wrapped in U+E000 … U+E001, which the UI turns into highlights.
- Filters: `from` (inclusive) and `to` (exclusive) in unix ms on `created_at`; `mood` exact; `tag` exact (after lowercasing). With no text, the filters alone list entries.

**Encrypted backup** writes one `.vjbak` file containing the database, kept audio and `vault.json` (format below). It is encrypted under a key derived with Argon2id from a backup passphrase. The passphrase defaults to the current password (re-entered and verified), and the screen warns that anyone who knows the password can then open the backup too. A separate passphrase must pass the same strength rules as a password. You choose where to save it, for example a USB drive, through a native save dialog. `last_backup_at` is updated afterwards.

**Restore** is offered on the first-run screen and in Settings. In Settings it replaces the current journal after a typed confirmation ("replace"), and is refused while recording. The app locks first. After restoring, the restored vault's password and recovery key apply. Drafts from the replaced vault are deleted, since its keys are gone.

An optional reminder can nudge you to back up every N days. It runs locally and is off by default.

## Data formats

These formats are **cross-platform contracts**. Android must read and write them byte for byte.

### Files

Everything lives in the OS app-data folder (Tauri `app_data_dir`). The speech model ships in the app's resources folder.

| Path | Contents | Encrypted |
| --- | --- | --- |
| `vault.json` | Salts, KDF params, nonces, wrapped keys, recording public key | Wrapped keys only |
| `journal.db` | Entries, tags, settings, search index | Yes (SQLCipher, key = DEK) |
| `audio/<entry_id>.opus.enc` | Kept recordings | Yes (sealed-CK chunk stream) |
| `drafts/<draft_id>.draft` | In-progress transcript segments | Yes (sealed-CK chunk stream) |
| `drafts/<draft_id>.opus.enc` | In-progress audio, when it is being kept | Yes (sealed-CK chunk stream) |

### `vault.json`

All binary values are standard base64 with padding.

```json
{
  "version": 1,
  "dek_by_password": {
    "kdf": { "alg": "argon2id", "m_kib": 262144, "t": 3, "p": 1, "salt": "<16 bytes>" },
    "nonce": "<24 bytes>",
    "ciphertext": "<32-byte DEK + 16-byte tag>"
  },
  "dek_by_recovery": {
    "kdf": { "alg": "hkdf-sha256", "salt": "<16 bytes>", "info": "privenator/v1/recovery-wrap" },
    "nonce": "<24 bytes>",
    "ciphertext": "<48 bytes>"
  },
  "recording_key": {
    "public": "<32 bytes X25519>",
    "secret_wrapped": { "nonce": "<24 bytes>", "ciphertext": "<32 + 16 bytes, key = DEK>" }
  }
}
```

The AAD for each wrap is the UTF-8 of its JSON path (`"dek_by_password"`, `"dek_by_recovery"`, `"recording_key"`). This stops a wrapped blob from being swapped into a different slot.

### Sealed chunk stream (audio and drafts)

```
magic        8 bytes   "PVJSTRM1"
eph_pub     32 bytes   ephemeral X25519 public key
ck_nonce    24 bytes
ck_sealed   48 bytes   CK (32) + tag (16)
stream_id   16 bytes   random; used in chunk nonces
then repeated:
  len        4 bytes   little-endian ciphertext length
  ciphertext len bytes XChaCha20-Poly1305(CK, nonce = stream_id ‖ counter_u64_le, aad = header ‖ final_flag_u8)
```

Sealing the CK:
- wrap key = HKDF-SHA256(ikm = X25519(eph_secret, rec_pub), salt = `eph_pub ‖ rec_pub`, info = `"privenator/v1/seal"`)
- ck_sealed = XChaCha20-Poly1305(wrap key, ck_nonce, aad = `magic ‖ eph_pub`, CK)
- The reader computes the same shared secret as X25519(rec_secret, eph_pub).

Chunks:
- The header (all 128 bytes before the first chunk) is authenticated as AAD on every chunk, followed by a one-byte `final_flag`.
- `finish` writes an **empty** chunk with `final_flag = 1`. Any bytes after it are an error.
- Each chunk is fsynced as it is written, so a crash loses at most the chunk being written.
- Readers: a chunk that fails authentication means the file was tampered with (error). A length prefix or body cut short at the end of the file is a torn write from a crash: reading stops there and the stream is reported as *unfinished*. Drafts may be unfinished; kept audio must be finished.

- Audio chunk plaintext: an Ogg Opus stream (RFC 7845) split at page boundaries. Chunk 0 holds the `OpusHead` page (version 1, 1 channel, pre-skip = encoder lookahead in 48 kHz samples, input rate 16000, gain 0, mapping family 0) and the `OpusTags` page (vendor `privenator`, no comments). Each later chunk holds one page of up to 50 packets (20 ms each, so about 1 s). The last packet carries the end-of-stream flag, and its granule position trims the zero padding of the final frame. Granule positions count 48 kHz samples, as RFC 7845 requires. Ogg stream serial `0x50564A41`. Concatenating the chunks gives a standard `.opus` file.
- Draft chunk plaintext: one UTF-8 JSON object per finalized segment: `{"t_ms": <start>, "text": "..."}`. Empty segments are not written.

### `.vjbak` backup

```
magic        8 bytes   "PVJBAK01"
header_len   4 bytes   little-endian
header       JSON      { "version": 1, "kdf": {argon2id params + salt}, "stream_id": "<16 bytes b64>", "created_at": <unix ms> }
then the chunk stream (same chunk format as above, key = Argon2id(passphrase), AAD = magic ‖ header_len ‖ header ‖ final_flag)
  plaintext = a POSIX ustar archive of: vault.json, journal.db, audio/*
```

- Header JSON: `{"version":1,"kdf":{"alg":"argon2id","m_kib":…,"t":…,"p":…,"salt":"<16 bytes b64>"},"stream_id":"<16 bytes b64>","created_at":<unix ms>}`. Desktop uses the same Argon2id cost as the vault default.
- Chunks hold 1 MiB of plaintext (the last data chunk may be shorter), then the empty final chunk. Nonce = `stream_id ‖ counter_u64_le`.
- The archive may contain only `vault.json`, `journal.db` and `audio/<name>.opus.enc`. Anything else (including absolute paths or `..`) makes the backup invalid.
- A failure in the first chunk is reported as "wrong passphrase (or damaged file)", since the two look identical; a later failure, or a missing final chunk, as "damaged".
- Restoring extracts into `.restoring/` inside the data folder first, checks that the stream finished and that `vault.json` and `journal.db` are both present, then swaps the files in, `vault.json` last.

The database is copied for backup with the SQLite online backup API into a temporary file encrypted with the same key, never by copying the live file.

### Database schema (`journal.db`, schema version 1)

```sql
PRAGMA user_version = 1;

CREATE TABLE entries (
  rowid         INTEGER PRIMARY KEY,   -- stable integer key for FTS
  id            TEXT NOT NULL UNIQUE,  -- UUID v7, used everywhere outside the DB
  created_at    INTEGER NOT NULL,      -- unix ms, UTC
  updated_at    INTEGER NOT NULL,
  title         TEXT,
  original_text TEXT NOT NULL,         -- never modified after insert
  edited_text   TEXT,                  -- latest edit; NULL = unchanged
  duration_s    INTEGER NOT NULL,
  mood          INTEGER CHECK (mood BETWEEN 1 AND 5),
  has_audio     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX entries_created_at ON entries(created_at DESC);

CREATE TABLE tags (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE entry_tags (
  entry_rowid INTEGER NOT NULL REFERENCES entries(rowid) ON DELETE CASCADE,
  tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (entry_rowid, tag_id)
);

-- Regular (not external-content) FTS table: the duplication is small and
-- avoids the rowid-drift and sync bugs of external content tables.
CREATE VIRTUAL TABLE entries_fts USING fts5(
  title, original_text, edited_text, tags,
  tokenize = 'porter unicode61 remove_diacritics 2'
);

CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
  INSERT INTO entries_fts(rowid, title, original_text, edited_text, tags)
  VALUES (new.rowid, new.title, new.original_text, new.edited_text, '');
END;

CREATE TRIGGER entries_au AFTER UPDATE OF title, edited_text ON entries BEGIN
  UPDATE entries_fts SET title = new.title, edited_text = new.edited_text
  WHERE rowid = new.rowid;
END;

CREATE TRIGGER entries_ad AFTER DELETE ON entries BEGIN
  DELETE FROM entries_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER entry_tags_ai AFTER INSERT ON entry_tags BEGIN
  UPDATE entries_fts SET tags = (
    SELECT group_concat(t.name, ' ') FROM entry_tags et JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_rowid = new.entry_rowid)
  WHERE rowid = new.entry_rowid;
END;

CREATE TRIGGER entry_tags_ad AFTER DELETE ON entry_tags BEGIN
  UPDATE entries_fts SET tags = coalesce((
    SELECT group_concat(t.name, ' ') FROM entry_tags et JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_rowid = old.entry_rowid), '')
  WHERE rowid = old.entry_rowid;
END;

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL      -- JSON-encoded value
);
```

Connection setup, in order: `PRAGMA key` (raw key form `x'<hex DEK>'`), a test read of `sqlite_master` ("file is not a database" there means the wrong key), `PRAGMA temp_store = MEMORY`, `PRAGMA foreign_keys = ON`, `PRAGMA journal_mode = WAL`. Migrations live in `crates/journal-core/src/migrations/`, step `user_version` forward one transaction each, and are never edited once released.

**Settings keys:** `idle_timeout_min` (number: 1, 5, 15 or 30; default 5), `default_keep_audio` (bool; default false), `mic_device` (cpal device ID string, or null for the system default), `backup_reminder_days` (1–365, or null = off; default off), `last_backup_at` (unix ms or null). Missing or invalid values read as the default.

## Cross-platform notes

The main per-OS work is microphone permission and packaging. Bundling `base.en` makes each desktop installer roughly 160 MB.

| OS | Microphone | Packaging | Speed-up |
| --- | --- | --- | --- |
| macOS | `NSMicrophoneUsageDescription` in Info.plist; `audio-input` entitlement under hardened runtime | `.dmg`, signed and notarized | Metal |
| Windows | User must allow desktop apps in Privacy > Microphone; show guidance if blocked | MSI or NSIS; signing avoids SmartScreen warnings | CPU (AVX2); optional Vulkan or CUDA |
| Linux | PipeWire or PulseAudio through ALSA via `cpal` | AppImage and `.deb` (WebKitGTK required) | CPU; optional Vulkan |
| Android (next) | `RECORD_AUDIO` runtime permission; foreground service of type `microphone` to record while backgrounded | APK/AAB via Tauri Android | ARM NEON; `tiny.en` likely |

Lock-on-minimize uses Tauri window events, which behave the same on all desktop OSes. Detecting OS screen lock or sleep is per-platform and is deferred past v1.

## Build order

Security and recording come first because every other feature depends on them. Progress is tracked in [PROGRESS.md](PROGRESS.md).

1. Scaffold Tauri v2 with React and TypeScript, as a Cargo workspace with `journal-core`. Lock down capabilities and CSP.
2. Vault: setup, unlock, recovery key, password change; open SQLCipher with the DEK; recording keypair.
3. Lock screen and auto-lock (minimize and idle).
4. Audio capture, resampling, and the encrypted Opus chunk-stream writer.
5. Whisper integration: transcribe the full recording after stop.
6. Live transcription: VAD segments, partial previews, encrypted drafts, crash recovery.
7. Entries UI: day list, editor with original and edited views, mood, tags, typed entries.
8. Search with FTS5 and filters.
9. Encrypted backup and restore.
10. Packaging and signing for all three desktop OSes; test on real hardware.
11. Android port (see ARCHITECTURE.md).

## Open questions

- [ ] App name. ("Privenator" is the working name.)
- [ ] Android minimum hardware: is `tiny.en` accurate enough, or does Android need `base.en` with slower previews?

Resolved questions are recorded in [DECISIONS.md](DECISIONS.md).
