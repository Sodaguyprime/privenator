# Progress

The build order is in [SPEC.md](SPEC.md#build-order). Update this file whenever a step starts or finishes, including anything that was deferred.

| # | Step | Status |
| --- | --- | --- |
| 1 | Scaffold, lockdown, design system | **Done** 2026-10-13 |
| 2 | Vault (setup, unlock, recovery, password change, SQLCipher, recording keypair) | **Done** 2026-10-13 |
| 3 | Lock screen and auto-lock | **Done** 2026-10-13 (hand check pending) |
| 4 | Audio capture and encrypted Opus writer | **Done** 2026-10-13 (hand check pending) |
| 5 | Whisper, transcribe after stop | **Done** 2026-10-13, folded into step 6 (stop transcribes only what's left) |
| 6 | Live transcription, drafts, crash recovery | **Done** 2026-10-13 (hand check pending) |
| 7 | Entries UI | **Done** 2026-10-13 (hand check pending) |
| 8 | Search | **Done** 2026-10-13 (hand check pending) |
| 9 | Backup and restore | **Done** 2026-10-13 (hand check pending) |
| 10 | Desktop packaging and signing | **Partly done**: Windows installers build; unsigned; macOS/Linux not built |
| 11 | Android port | **Not started**: blocked on this machine (see DECISIONS.md) |

## Step 1: what exists

- Cargo workspace: `crates/journal-core` and `src-tauri` (shell).
- No Tauri plugins. Command allowlist through `build.rs` + `capabilities/main.json`; strict CSP.
- Design tokens, bundled fonts, ornaments, `Button` and the ornament components; the welcome screen; the diary app icon for desktop and Android.

## Step 2: what exists

- Vault (create, unlock, recover with key rotation, change password), recovery-key format, the recording X25519 keypair, the sealed chunk stream, the SQLCipher journal with schema v1 and settings, the password policy, and process hardening.
- Unlock throttling, Argon2id off the UI thread, a native save dialog for the recovery key.
- First-run flow, unlock with countdown, forgot-password recovery.

## Step 3: what exists

- `IdleTimer` and `Journal::idle_timeout()`; the idle watcher thread, lock on minimize, the `locked` event, `activity` and `release_autolock`.

## Steps 4–10: what exists

- **Audio (4):** cpal capture on its own thread, conversion to 16 kHz mono, Ogg Opus at 24 kbps into a sealed stream (one chunk per second, works while locked), the recorder (mic → worker thread → file + transcription sink), mic list and mic test with a level meter, the first-run microphone step.
- **Transcription (5, 6):** whisper.cpp `base.en` (loaded on first recording), the energy-based segmenter, the live worker (finals first, previews when idle), the encrypted text draft, crash recovery at unlock, transcript events only while unlocked, recording that survives locking and reappears after unlock.
- **Entries (7):** sidebar grouped by local day with Roman numerals and day dividers; entry reading view with drop cap; editing that keeps the original, "View original", "Revert to original"; title, mood, tags with autocomplete; audio playback from an in-memory blob; delete with confirmation; typed entries; the save dialog after recording.
- **Search (8):** FTS5 with stemming and prefix matching, highlighted snippets, filters for date range, mood and tag.
- **Backup (9):** `.vjbak` create (journal password or separate passphrase) and restore (from the welcome screen, or in Settings with a typed confirmation); the backup reminder notice.
- **Settings:** idle timeout, microphone, default audio choice, change password, regenerate recovery key, backup, reminder, restore.
- **Packaging (10):** the model is bundled as a resource; `npm run tauri build` makes NSIS and MSI installers. `npm run fetch-model` fetches the model for development. v0.1.0 (NSIS + MSI, unsigned) was published 2026-10-27 as a GitHub Release on `Sodaguyprime/privenator`; the README links there.
- 43 integration tests at the seams in ARCHITECTURE.md "Testing", all passing. Clippy and ESLint are clean; the frontend type-checks and builds.

## Deferred / known gaps

- **Nothing from steps 3–10 has been clicked through by hand.** The app was never launched during this work: launching could steal focus from whatever else is running, and there is no safe way to drive it without synthetic input. The core logic is covered by tests; the screens, the shell threads and real microphones are not. To check, run `npm run tauri dev` and go through:
  1. First run: password → recovery key (Copy, Save, the two-group check) → microphone test (the bars move) → journal.
  2. Record ~30 s with pauses: text appears while you talk (grey preview, then final); pause/resume; stop → save dialog → entry opens.
  3. Minimize during a recording: the app locks; unlock: the recording is still going, with its text so far.
  4. Kill the app mid-recording (Task Manager), restart, unlock: the recording is saved as an entry and the journal says so.
  5. Entry: edit, view original, revert; mood; tags; play audio (if kept); delete.
  6. Search: words, word forms, filters; highlighted snippets.
  7. Settings: change password, new recovery key, idle timeout (set 1 minute and wait), microphone choice.
  8. Backup with the password, then with a passphrase; restore one on the welcome screen of a fresh profile and in Settings.
  9. Wrong password countdown; forgot-password recovery.
- No automated test yet showing that an *unlisted* command is denied.
- On Windows the welcome screen scrolls slightly at the default 1100×760 window size.
- A write failure in the recorder (e.g. disk full) only surfaces when the recording is stopped.
- Day grouping and the date filters use the current local time zone.
- Installers are unsigned (SmartScreen will warn). macOS (`.dmg`, notarization, `NSMicrophoneUsageDescription`) and Linux (AppImage/`.deb`) are not built or tested.
- Argon2id cost not yet measured on slow hardware (step 10 asks for it).
- Android (step 11): see DECISIONS.md and ARCHITECTURE.md "Android port".
