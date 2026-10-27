# Architecture

How the code is organized, and why. The product behavior is in [SPEC.md](SPEC.md); this file covers structure, boundaries, the IPC contract, building, and the Android port.

## Layout

```
Cargo.toml                 Cargo workspace root (profiles live here)
crates/journal-core/       Platform-neutral Rust: everything sensitive
  src/vault.rs             vault.json, create/unlock/recover/change password/regenerate key, Session (= unlocked)
  src/recovery.rs          Recovery key format (Crockford base32)
  src/stream.rs            Sealed chunk stream (audio + drafts)
  src/journal.rs           SQLCipher connection, migrations, settings, DB backup copy
  src/entries.rs           Entries: create, list, edit/revert, mood, tags, delete
  src/search.rs            FTS5 search with filters and snippets
  src/drafts.rs            Draft files, saving a recording as an entry, crash recovery
  src/audio/               capture.rs (cpal mic), convert.rs (→16 kHz mono),
                           file.rs (Ogg Opus in a sealed stream), recorder.rs (mic → file + sink)
  src/live/                segmenter.rs (VAD), pipeline.rs (live transcription worker)
  src/transcribe.rs        whisper.cpp wrapper (final and preview passes)
  src/backup.rs            .vjbak create/restore
  src/autolock.rs          Idle auto-lock policy (IdleTimer; pure, time passed in)
  src/password.rs          Strength policy (zxcvbn)
  src/hardening.rs         Per-OS crash-dump opt-out
  src/crypto.rs            Thin wrappers over the primitives (internal)
  src/migrations/          Schema migrations (never edit a released one)
  tests/                   Integration tests at the agreed seams (see "Testing"); fixtures/speech.wav
src-tauri/                 Thin Tauri v2 shell: IPC commands, window events, timers, capabilities
  build.rs                 Declares the app's commands (anything not listed is denied)
  capabilities/main.json   What the webview may call
  src/commands/            The IPC surface, by area (see "Command API")
  src/state.rs             AppState: vault, unlocked session, recording, pending save, mic test, Whisper
  src/autolock.rs          Idle watcher thread, lock-on-minimize hook, `locked` event
  src/error.rs             CommandError { kind, message, retry_after_ms }
  resources/models/        ggml-base.en.bin (git-ignored; `npm run fetch-model`), bundled into installers
  icons/source.svg         Source for every app icon (desktop + Android)
src/                       React + TypeScript UI (renders only; never sees keys)
  styles/                  Design tokens (theme.css), ornaments, base styles, bundled fonts
  ui/                      Primitives: Button, Field, Frame, ornaments, LevelMeter, MoodPicker, TagInput
  screens/                 Welcome, CreatePassword, RecoveryKeySheet, Unlock, MicCheck, Journal
  screens/journal/         Sidebar, RecordPanel (+ save dialog), EntryView, TypedEntry, SettingsPanel
  lib/ipc.ts               Typed wrappers, one per Tauri command, plus events
  lib/hooks.ts             Shared hooks (password strength, countdown, activity pings, events)
  lib/format.ts            Dates, durations, mood labels
scripts/fetch-model.mjs    Downloads and checksums the Whisper model (dev/build only)
docs/                      Spec, architecture, design, decisions, progress
```

## The core/shell boundary

`journal-core` holds **all** logic that has to behave the same on every platform: key handling, file formats, the database schema and queries, the audio and transcription pipeline, and backup. It does not depend on Tauri, the webview or any OS UI. The rules:

- **Core** takes plain inputs (paths, bytes, settings) and returns plain results.
- **Shell** (`src-tauri`) owns the lifecycle: which window is focused, the idle timer thread, when to call `lock()`, file dialogs, and turning core results into IPC responses and events.
- **UI** (`src/`) renders state and sends user intent. It never holds the DEK. It holds journal plaintext only for what is on screen, and drops it on the `locked` event (the journal screen unmounts).

This split is what makes the Android port cheap (below).

### Threads while recording

```
cpal audio thread ──(16 kHz blocks)──► recorder thread: Opus → sealed audio draft, level
                                            │ sink
                                            ▼
                                  transcribe thread: Segmenter → Whisper (finals first, then previews)
                                            │ finals → sealed text draft
                                            ▼
                              events → UI (only while unlocked)
meter thread (shell): `recording:level` every 100 ms
```

The audio callback never blocks or touches the disk; the recorder thread does the fsyncs. `cpal::Stream` isn't `Send` on every platform, so `Mic` owns it on a dedicated thread.

## Security posture of the shell

- **No plugins.** No http, updater, shell, opener, fs or dialog plugins. File dialogs are opened from Rust with `rfd`. Adding any plugin needs an entry in [DECISIONS.md](DECISIONS.md).
- **Command allowlist.** `build.rs` passes `AppManifest::commands(&[...])`, which makes Tauri generate an `allow-<command>` permission for each command and deny anything not granted. `capabilities/main.json` grants only those plus `core:event:default`. `core:default` is **not** granted, so the webview has no window, path, menu, tray, image or resource APIs.
- **CSP** (production), in `tauri.conf.json`:
  `default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'`.
  The dev CSP additionally allows inline styles (Vite injects them) and the HMR websocket.
  - `data:` images are only for the CSS paper-grain texture. `blob:` media is for decrypted audio playback, made from bytes returned by `entry_audio`.
- `freezePrototype: true` and `withGlobalTauri: false`.
- Vite's `assetsInlineLimit: 0` keeps fonts as files, so `font-src 'self'` holds.
- Fonts are bundled through `@fontsource`. Nothing is fetched at runtime.

## Command API

Every Tauri command must be (1) implemented in `src-tauri/src/commands/`, (2) registered in `lib.rs`, (3) listed in `build.rs`, (4) granted in `capabilities/main.json`, (5) wrapped in `src/lib/ipc.ts`, and (6) listed here. Android uses the same commands if it stays on Tauri. Arguments are camelCase on the JS side.

### Vault (`commands/vault.rs`)

| Command | Args | Returns | Since | Notes |
| --- | --- | --- | --- | --- |
| `app_info` | none | `{ app_version, core_version }` | 1 | |
| `vault_status` | none | `"none" \| "locked" \| "unlocked"` | 2 | Picks the first screen |
| `password_strength` | `password` | `{ score 0–4, acceptable, feedback }` | 2 | For the meter; same policy the vault enforces |
| `create_vault` | `password` | recovery key string | 2 | Leaves the app unlocked with auto-lock held |
| `unlock` | `password` | `{ recovered }` | 2 (6: returns) | Throttled after failures. Turns crash-left drafts into entries; `recovered` counts them |
| `recover` | `recoveryKey`, `newPassword` | **new** recovery key string | 2 | Throttled; old key stops working; auto-lock held |
| `change_password` | `currentPassword`, `newPassword` | nothing | 2 | Requires unlocked **and** the current password |
| `regenerate_recovery_key` | `currentPassword` | **new** recovery key string | 7 | Old key stops working; auto-lock held until `release_autolock` |
| `lock` | none | nothing | 2 | Drops keys, closes the DB, drops an unsaved stopped recording's text, emits `locked` |
| `activity` | none | nothing | 3 | Keyboard/pointer input; restarts the idle countdown. Sent at most every 10 s. No-op when locked |
| `release_autolock` | none | nothing | 3 | The new recovery key is confirmed. Idle stays paused if recording |
| `save_recovery_key` | `recoveryKey` | `true` saved / `false` cancelled | 2 | Native save dialog (rfd), opened from Rust |

### Recording (`commands/recording.rs`)

| Command | Args | Returns | Since | Notes |
| --- | --- | --- | --- | --- |
| `list_microphones` | none | `[{ id, name, is_default }]` | 4 | `id` is what `mic_device` stores |
| `mic_test_start` | `deviceId \| null` | nothing | 4 | Level meter only; nothing stored. Levels arrive as `recording:level` |
| `mic_test_stop` | none | nothing | 4 | |
| `start_recording` | none | recording ID | 4 | Requires unlocked. Loads Whisper on first use. Holds the idle timer |
| `pause_recording` / `resume_recording` | none | nothing | 4 | Pauses the mic stream itself |
| `recording_status` | none | `{ state, id, elapsed_ms, finals, stopped }` | 6 | `state`: `idle \| recording \| paused \| stopped`. Rebuilds the screen after an unlock. Requires unlocked |
| `stop_recording` | none | `{ id, duration_s, text }` | 5 | Waits for the remaining transcription; the draft stays until saved |
| `save_recording` | `keepAudio`, `mood`, `tags` | entry ID | 5 | The save dialog; creates the entry from the stopped recording |

### Entries and search (`commands/entries.rs`)

| Command | Args | Returns | Since | Notes |
| --- | --- | --- | --- | --- |
| `list_entries` | none | `EntrySummary[]` newest first | 7 | `{ id, created_at, updated_at, title, duration_s, mood, has_audio, tags }` |
| `get_entry` | `id` | `Entry` | 7 | Summary plus `original_text`, `edited_text` |
| `create_typed_entry` | `text` | entry ID | 7 | `duration_s` 0, no audio |
| `set_entry_title` | `id`, `title \| null` | nothing | 7 | Blank clears |
| `set_entry_text` | `id`, `text` | nothing | 7 | Latest edit; equal to the original clears it |
| `revert_entry` | `id` | nothing | 7 | `edited_text` = NULL |
| `set_entry_mood` | `id`, `mood 1–5 \| null` | nothing | 7 | |
| `set_entry_tags` | `id`, `tags` | nothing | 7 | Replaces; normalized |
| `delete_entry` | `id` | nothing | 7 | Also deletes the audio file |
| `all_tags` | none | `string[]` | 7 | For autocomplete and filters |
| `entry_audio` | `id` | raw bytes (`ArrayBuffer`) | 7 | Decrypted Ogg Opus, in memory only |
| `search_entries` | `query: { text, from?, to?, mood?, tag? }` | `[{ entry, snippet }]` | 8 | See SPEC "Search" |

### Settings and backup (`commands/settings.rs`, `commands/backup.rs`)

| Command | Args | Returns | Since | Notes |
| --- | --- | --- | --- | --- |
| `get_settings` | none | `Settings` | 7 | All keys with defaults applied |
| `update_settings` | `update` (partial; `null` clears a nullable key) | `Settings` | 7 | Validated in core. A new idle timeout applies at once |
| `create_backup` | `usePassword`, `secret` | `true` saved / `false` cancelled | 9 | Verifies the password, or checks a separate passphrase's strength; then a native save dialog; sets `last_backup_at` |
| `restore_backup` | `passphrase` | `true` restored / `false` cancelled | 9 | Native open dialog. Refused while recording. Locks first; the app is locked afterwards |

Slow commands (Argon2id, Whisper, file I/O) run on a blocking thread (`spawn_blocking`), so the UI never freezes.

**Errors.** Every command rejects with `{ kind, message, retry_after_ms? }`. The UI branches on `kind`, never on `message`:
`wrong_password`, `wrong_recovery_key`, `invalid_recovery_key` (malformed), `weak_password`, `throttled`, `locked`, `already_exists`, `no_vault`, `not_found`, `invalid`, `no_microphone`, `mic_blocked`, `mic_error`, `already_recording`, `not_recording`, `wrong_passphrase`, `bad_backup`, `internal`.
`retry_after_ms` comes with `throttled`, and with wrong password or recovery-key failures, so the UI can count down.

### Events (Rust → UI)

| Event | Payload | Since | Meaning |
| --- | --- | --- | --- |
| `locked` | none | 3 | Keys are already gone. The UI unmounts the journal (dropping its plaintext) and shows the unlock screen |
| `recording:level` | `{ level 0–1, elapsed_ms, paused }` | 4 | ~10/s while recording or testing the mic. Not secret, so it is sent while locked too |
| `transcript:preview` | string | 6 | Provisional text since the last final. **Only sent while unlocked** |
| `transcript:final` | `{ t_ms, text }` | 6 | A finished segment, already in the draft. **Only sent while unlocked** |

**Auto-lock.** The shell owns it, so a stalled webview can't keep the journal open. A thread checks `IdleTimer::expired` once a second; `WindowEvent::Resized` plus `is_minimized()` catches minimize. Both lock through `AppState::lock_if` (one mutex, so a lock can't race an unlock) and then emit `locked`. The idle timer lives inside the unlocked state; it is held while recording or while a new recovery key is shown. Minimize ignores the recording hold (the recording continues behind the lock) but respects `showing_recovery_key`.

## Testing

Tests sit only at agreed seams, as integration tests in `crates/journal-core/tests/` that use real files in a temp folder. 43 tests, all passing:

| Seam | File | Tests | Covers |
| --- | --- | --- | --- |
| Vault lifecycle | `vault.rs` | 7 | create/unlock, wrong password, weak passwords, recovery and key rotation, forgiving key entry, password change, key regeneration |
| Sealed chunk stream | `stream.rs` | 4 | write while locked / read after unlock, crash-truncated drafts, tampering, other vault's key |
| Encrypted journal DB | `journal_db.rs` | 5 | data survives lock/unlock, no plaintext on disk, other vault's key rejected, idle-timeout setting |
| Idle auto-lock policy | `autolock.rs` | 3 | expiry, activity resets, hold/release |
| Audio | `audio.rs` | 3 | any device format → 16 kHz mono at the same pitch; Opus file written while locked plays back after unlock; a crash loses at most the last second |
| Transcription | `transcribe.rs` | 1 | speech fixture → expected words (needs the model) |
| Segmenter (VAD) | `segmenter.rs` | 4 | pauses end segments, 25 s cap without loss, silence yields nothing, tail and flush |
| Live pipeline | `live.rs` | 1 | speech with pauses → final segments, events, same segments in the draft (needs the model) |
| Drafts | `drafts.rs` | 3 | crash-left draft becomes an entry (text, duration, audio kept as a finished stream), audio discarded by default, running recording left alone |
| Entries | `entries.rs` | 4 | default title and order, edit/original/revert, tags and mood, delete |
| Search | `search.rs` | 4 | stemming/prefix across text, edits and tags with highlights; filters; odd input; deletions leave the index |
| Backup | `backup.rs` | 4 | round trip to another data dir, wrong passphrase, tampering, restore replaces the current journal |

Tests use a cheap Argon2 cost (`m_kib: 1024, t: 1`); production uses `KdfParams::default()`. The speech fixture was generated with Windows' offline text-to-speech. Run everything with `cargo test --workspace` (the model must be present: `npm run fetch-model`).

Not automated, and checked by hand only: the shell (commands, threads, minimize hook), real microphones (`Mic`, `Recorder`), and the React screens.

## Building

Prerequisites beyond Node and Rust:

- **Whisper model:** `npm run fetch-model` downloads `ggml-base.en.bin` (148 MB) into `src-tauri/resources/models/` and checks its SHA-256. It is git-ignored and bundled into installers via `bundle.resources`. The app never downloads anything itself.
- **CMake** (whisper.cpp and libopus are C/C++ built through cmake). Any install works; on the dev machine it is `uv tool install cmake ninja`.
- **libclang** for bindgen (whisper-rs-sys's pre-generated bindings are for Linux and fail on Windows). A full LLVM install works, or the `libclang` Python wheel's DLL: set `LIBCLANG_PATH` to its folder.
- **Windows only:**
  - **Strawberry Perl** for SQLCipher's vendored OpenSSL (`winget install StrawberryPerl.StrawberryPerl`). Git Bash's MSYS Perl does not work. If Git Bash's Perl comes first on your PATH, set `OPENSSL_SRC_PERL=C:\Strawberry\perl\bin\perl.exe`.
  - With the libclang wheel (no full LLVM), bindgen can't find C headers; point it at MSVC's: `BINDGEN_EXTRA_CLANG_ARGS=-I"<MSVC>\include" -I"<Windows Kits>\Include\<ver>\ucrt"`.
  - If a non-working Visual Studio version is also present, cmake may pick it; force the right one, e.g. `CMAKE_GENERATOR=Visual Studio 17 2022`.
- **macOS/Linux/Android NDK:** the system Perl and clang work.

Profiles:
- The dev profile builds `argon2`, `blake2` and `chacha20poly1305` at opt-level 3 (fast unlocks), and `whisper-rs-sys` at opt-level 3 (the cmake crate then builds whisper.cpp in Release; Debug whisper is many times slower).
- Dev builds keep line tables for our code and no debug info for dependencies (full debug info made `target/` about 8 GB).
- Release: LTO, one codegen unit, `panic = "abort"`, stripped.

`npm run tauri build` produces the Windows installers (NSIS 129 MB, MSI 135 MB, model included) in `target/release/bundle/` in about 10 minutes from clean. They are unsigned; see PROGRESS.md. For macOS, `src-tauri/Info.plist` (microphone usage text) and `Entitlements.plist` (hardened-runtime `audio-input`) are already in place.

## Android port

Tauri v2 builds for Android, so the planned route is: **same `journal-core`, same React UI, a Tauri Android target, plus Android-specific glue.** If that proves wrong (for example, background recording turns out to be too awkward), the fallback is a native Kotlin app that calls `journal-core` through UniFFI. That fallback is only possible because the core is free of Tauri.

What carries over unchanged:
- All file formats (`vault.json`, sealed chunk streams, `.vjbak`, the DB schema, the Ogg Opus layout, draft segments). A backup made on one platform restores on the other. **Treat these as frozen contracts**; change them only through a version bump plus migration, logged in DECISIONS.md.
- Crypto, DB, entries, search, drafts and crash recovery, the segmenter and live pipeline, backup, settings validation, the idle policy, and the design tokens (`src/styles/theme.css`, also listed in DESIGN.md).

What Android needs on top:

| Concern | Desktop | Android |
| --- | --- | --- |
| Mic permission | OS prompt / settings guidance | `RECORD_AUDIO` runtime permission, requested before `mic_test_start`/`start_recording` |
| Recording in background | Window minimized, process keeps running | Foreground service, type `microphone`, with an ongoing notification (Android 14+ requires the type) |
| Auto-lock trigger | Window minimize / idle | App backgrounded (`onStop`) / idle. Reuse `IdleTimer` and `Journal::idle_timeout()`; keep the "held while a new recovery key is shown" rule for backgrounding too |
| Audio capture | `cpal` (WASAPI/CoreAudio/ALSA) | `cpal` with the AAudio backend; `Mic`'s own-thread design already fits |
| Whisper | `base.en`, AVX2/Metal | Probably `tiny.en`, ARM NEON; benchmark on a mid-range phone first. Ship the model as an asset; copy it to app storage on first run if whisper.cpp needs a file path |
| Native builds | cmake + MSVC/clang | whisper-rs-sys, opusic-sys (honours `ANDROID_NDK_HOME`) and SQLCipher's OpenSSL all cross-compile with the NDK toolchain; bindgen needs libclang on the build host |
| Argon2id cost | 256 MiB / t=3 (0.93 s on the dev machine, including opening the DB) | Lower memory (e.g. 64 MiB, more iterations). Params are stored per vault and per backup, so this is compatible |
| SQLCipher | `bundled-sqlcipher-vendored-openssl` | Same feature |
| Crash dumps | WER exclusion / rlimit (implemented: `journal_core::harden_process`) | `prctl(PR_SET_DUMPABLE, 0)` (same function, already has the Android branch) |
| Screenshots | n/a | `FLAG_SECURE` on the activity so the recents screen and screenshots show nothing |
| File dialogs (recovery key, backup, restore) | `rfd` opened from Rust | Storage Access Framework (`ACTION_CREATE_DOCUMENT` / `ACTION_OPEN_DOCUMENT`); rfd has no Android backend. `backup::create`/`restore` take paths, so write to a temp file and copy through the SAF stream |
| Audio playback | `blob:` URL in the webview | Same (Android WebView plays Ogg Opus) |
| Layout | Sidebar + main pane | Single column below 768 px, already implemented in `Journal.tsx` |

The Android launcher icons were generated from `src-tauri/icons/source.svg` and are in `src-tauri/icons/android/`.
