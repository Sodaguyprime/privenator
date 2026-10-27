# Decision log

Newest first. Each entry records what was decided, why, and what it affects. When code or the spec changes direction, add an entry here; don't rewrite old ones.

---

### 2026-10-27 · Search box hides the native clear button
WebView2 (Chromium) draws a bright blue "×" in `type="search"` inputs, which ignores the theme tokens. It is hidden with `::-webkit-search-cancel-button`. Clearing still works: Esc clears the field, and the filter panel has a Clear button.

### 2026-10-27 · README screenshots come from a mocked UI
The README screenshots are the real React UI rendered in headless Chromium, with Tauri IPC replaced by `@tauri-apps/api/mocks` and sample entries. Launching the real app would take focus from whatever else is running, and the screenshots must not show a real journal. The harness lives in the gitignored `.scratch/shots/`, so it is not part of the build.

### 2026-10-13 · Android port (step 11) not started on this machine
The build host has no Android NDK, only Java 8 (Tauri Android needs 17+), and about 14 GB of free disk; the NDK, Gradle caches and four Android targets of whisper.cpp + OpenSSL would not fit comfortably. The port plan is in ARCHITECTURE.md; it needs a machine (or a cleanup) with ~20 GB free, the NDK and JDK 17.

### 2026-10-13 · Installers are unsigned; macOS and Linux not built
Signing needs certificates (Windows code signing, Apple Developer ID + notarization) that only the owner can obtain. macOS and Linux packages must be built on those OSes. `npm run tauri build` makes the Windows NSIS and MSI installers.

### 2026-10-13 · Energy-based VAD instead of Silero
The spec named Silero VAD through whisper.cpp. That needs a second model file and a newer whisper.cpp VAD API. An energy detector with an adaptive noise floor (12 dB above the floor, at least −45 dBFS, 0.6 s pause, 25 s cap) does the job the spec asks for (cut at natural pauses) with no extra model, and is fully testable. Revisit if noisy rooms cause bad cuts; the `Segmenter` interface would stay.

### 2026-10-13 · Audio is always written during a recording; keep/discard is decided at save
The save dialog asks "keep audio?" only after the recording, so the audio has to exist until then. It is written to the sealed draft as it is recorded and moved or deleted at save. Encrypted either way, so this doesn't change the threat model.

### 2026-10-13 · `default_keep_audio` defaults to false
A voice recording is more identifying than its transcript and about 10 KB/s. The user turns keeping on per entry, or by default in Settings.

### 2026-10-13 · Crash-left drafts become entries automatically at unlock
The spec said the draft is "offered back". A prompt adds a screen and a way to lose data (dismissing it); creating the entry is reversible (delete it). Audio follows `default_keep_audio`. A damaged draft is skipped and never blocks unlocking. The same path handles a recording that was stopped but not saved before a lock.

### 2026-10-13 · Transcript events only while unlocked
Recording continues while locked, but sending live transcript text to a webview that is supposed to be locked would put plaintext where the lock screen is. The text stays in Rust; `recording_status` hands the finals over after unlock.

### 2026-10-13 · Minimize locks even while recording; only the recovery-key screen blocks it
Step 3 used one hold for both. The spec wants minimize-during-recording to lock the UI while the recording continues, so the recovery-key exemption is now its own flag (`showing_recovery_key`), and the idle timer's hold covers recording.

### 2026-10-13 · Whisper: greedy decoding, no carried context
Beam search (whisper.cpp's default 5) is several times slower for little gain on dictation. `no_context` stops one segment's errors from leaking into the next. Previews scale `audio_ctx` to the audio length (spec "Preview").

### 2026-10-13 · Backup: first-chunk failure is ambiguous by design
The `.vjbak` header has no passphrase check value, so a wrong passphrase and damage in the first megabyte look the same. The UI says so instead of guessing. Adding a check value would change the frozen format for little benefit.

### 2026-10-13 · Backup copies the DB with the online backup API into an encrypted temp file
`Journal` keeps the raw-key pragma string (zeroized on drop) so it can key the destination. `VACUUM INTO` and `sqlcipher_export` were rejected: their behaviour with FTS5 shadow tables and encryption is less certain than a page-level copy. Restore only accepts the three kinds of path the app writes, which also blocks path traversal.

### 2026-10-13 · A separate backup passphrase follows the password rules
Same zxcvbn policy (12+ characters, score 3+). "Use my journal password" re-enters and verifies the password, so a typo can't produce an unopenable backup.

### 2026-10-13 · Regenerating the recovery key asks for the current password
Same reasoning as the password change: being unlocked isn't enough.

### 2026-10-13 · Audio stack: cpal 0.18, rubato 5 (FFT), opus 0.4 (bundled libopus), ogg 0.9
The `opus` crate builds libopus from source through cmake on every platform (including the NDK), like SQLCipher's OpenSSL. Ogg framing (RFC 7845) makes the decrypted file a standard `.opus` the webview can play. Resampling is fixed-ratio, so rubato's FFT resampler.

### 2026-10-13 · Whisper model fetched by script and bundled, never downloaded by the app
`npm run fetch-model` downloads `ggml-base.en.bin` and checks its SHA-256; it is git-ignored and listed in `bundle.resources`. whisper.cpp's own stderr logging is routed into the `log` crate (no logger installed), so nothing is printed.

### 2026-10-13 · Windows toolchain: cmake via uv, libclang from the Python wheel
No LLVM install (about 2.5 GB, disk is tight). The `libclang` wheel's DLL plus MSVC's headers (`BINDGEN_EXTRA_CLANG_ARGS`) satisfy bindgen. A broken Visual Studio 18 folder made cmake pick it, so `CMAKE_GENERATOR=Visual Studio 17 2022` is set for this machine. All of this is in ARCHITECTURE.md "Building".

### 2026-10-13 · Dev builds without dependency debug info
`target/` had reached 8 GB with the disk 99% full. Line tables for our code, none for dependencies.

### 2026-10-13 · Steps 4–10 built in one pass at the user's request
"Continue until you finish." Seams were chosen without a separate agreement round, each listed in ARCHITECTURE.md "Testing"; hardware-bound parts (mic, shell threads, screens) are checked by hand.

### 2026-10-13 · Auto-lock is held while a new recovery key is on screen
The recovery key is shown once and can't be rebuilt from `vault.json`. Locking on that screen (minimize, or 5 minutes spent writing it down) would lose it before the user has kept it. `create_vault` and `recover` start the session with auto-lock already held, and the UI releases it once the key is confirmed. Starting held, instead of having the sheet ask for a hold, leaves no gap for a minimize to land in. Rejected: keeping the key in Rust memory so it could be shown again after unlock, which would be a second place a secret lives.

### 2026-10-13 · The shell owns the idle timer; the UI only reports input
The idle countdown and the lock both live in Rust, so a frozen or misbehaving webview can't keep the journal unlocked. The webview sends `activity` at most every 10 s. The policy (`IdleTimer`) sits in `journal-core` with time passed in, so it is testable and Android can reuse it. Idle input = key press, click, pointer move, scroll. OS screen-lock and sleep detection stay deferred (SPEC "Platform notes").

### 2026-10-13 · `cipher_memory_security` off
With `PRAGMA cipher_memory_security = ON`, this SQLCipher build overflows the stack (even with a 16 MB stack) as soon as the FTS5 schema is created. That was reproduced with only that pragma set. The pragma only wipes SQLite's freed heap memory *while the app runs*. Reading that memory requires access to the live process, which the threat model already excludes. Locking still closes the DB and zeroizes the DEK. Revisit if a later SQLCipher release fixes it.

### 2026-10-13 · Crypto crates pinned to one generation
`argon2 0.5`, `chacha20poly1305 0.10`, `hkdf 0.12`, `sha2 0.10`, `x25519-dalek 2` all share `rand_core 0.6`/`generic-array`. Newer majors of the RustCrypto crates exist; upgrade them together, in a change of their own. Randomness comes from `getrandom` (OS CSPRNG).

### 2026-10-13 · Windows builds need Strawberry Perl
SQLCipher needs OpenSSL, built from source via `bundled-sqlcipher-vendored-openssl` so every platform (including Android) builds the same way with no system OpenSSL. On Windows that needs Strawberry Perl. Alternatives rejected: a system OpenSSL (a per-machine install on every OS), or app-level encryption instead of SQLCipher (would leave the FTS index unencrypted).

### 2026-10-13 · Recovery-key file saved through a Rust-side dialog (`rfd`)
"Save as text file" needs a save dialog. The Tauri dialog plugin would expose file-system pickers to the webview and break the no-plugins rule, so the dialog is opened from Rust with `rfd` and the webview only gets a saved/cancelled boolean. Android will need its own path (Storage Access Framework).

### 2026-10-13 · Password change also asks for the current password
Being unlocked isn't enough, since someone at an unattended unlocked machine could otherwise take the journal over. This is cheap: one extra Argon2id derivation.

### 2026-10-13 · Unlock throttle lives in memory
The delay (1, 2, 4 … 30 s) resets when the app restarts. That's acceptable because the spec defines it as a UX guard; Argon2id cost is the defense against offline guessing, which a persisted counter wouldn't stop anyway.

### 2026-10-13 · Argon2id default kept at 256 MiB, t=3
Measured 0.93 s for create (both the derivation and opening the DB) on the dev machine. That's inside the 0.5–1 s target, but the dev machine is fast; re-measure on the slowest supported hardware in step 10, and set lower params for Android.

### 2026-10-13 · Sealed-stream details fixed during implementation
HKDF salt = `eph_pub ‖ rec_pub`, CK seal AAD = `magic ‖ eph_pub`, completion marked by an empty final chunk, a torn tail treated as "unfinished" rather than corrupt. Written into SPEC.md.

### 2026-10-13 · First-run microphone step deferred to step 4
SPEC first-run step 4 (mic permission + level meter) needs audio capture, so for now first run goes straight from the recovery key to the journal.

### 2026-10-13 · Fonts bundled, not loaded from Google Fonts
The design system calls for Google Fonts, but the app has no network access at runtime. Cormorant Garamond, Crimson Pro and Cinzel are bundled via `@fontsource` (only the weights in use). The CSP allows fonts from `'self'` only.

### 2026-10-13 · Visual style: Academia / Classical
"Library at night": mahogany, parchment, brass, crimson; three serifs. Adopted "for now", so it may change later. All tokens are in one file (`src/styles/theme.css`, mirrored in DESIGN.md) so a restyle touches one place. Marketing-page patterns from the source style guide (pricing ledgers, sepia hero images) are adapted to a journal app; see DESIGN.md.

### 2026-10-13 · Cargo workspace with a platform-neutral `journal-core`
An Android app is planned. Keeping all logic out of the Tauri shell means Android can reuse it via a Tauri Android target, or via UniFFI from Kotlin if needed.

### 2026-10-13 · Command allowlist via `AppManifest`, no `core:default`
Tauri allows every app command by default. Declaring commands in `build.rs` makes them deny-by-default. Dropping `core:default` removes window, path, menu and other APIs that the webview doesn't need.

### 2026-10-13 · Spec review fixes
- **Recording while locked:** the original spec wiped the DEK on lock but kept recording and autosaving encrypted, which is impossible. Fixed with an X25519 recording keypair: recordings are sealed to the public key, and the secret key is wrapped under the DEK.
- **FTS:** the original external-content FTS used an implicit rowid (which `VACUUM` can renumber), had no sync triggers, and left out tags even though the spec says tags are searchable. Fixed: an explicit `rowid INTEGER PRIMARY KEY`, a regular FTS5 table with triggers, and a tags column.
- **No plaintext on disk:** added `temp_store = MEMORY`, crash-dump opt-out per OS, and spellcheck off in the editor.
- **Recovery key KDF:** HKDF instead of Argon2id. The key is 128 random bits, so stretching adds nothing.
- **Backup passphrase:** still defaults to the password, but the backup screen now warns about it.
- **Whisper scheduling:** final transcription jobs take priority; previews are skipped under backlog and use a reduced `audio_ctx`.
- **Byte-level formats** are now written down (vault, chunk streams, backup) so that Android can interoperate.

### 2026-10-13 · Open questions resolved
- **Revision history:** keep the original plus only the latest edit.
- **Idle timeout:** 5 minutes by default, with 1/5/15/30 in Settings.
- **Plaintext Markdown export:** left out. It would undo the threat model.
- **Model:** `base.en` only on desktop v1. `small.en` would mean a ~600 MB installer or a network download.
- **App name:** still open. "Privenator" is the working name.
