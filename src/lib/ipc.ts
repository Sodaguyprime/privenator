// Typed wrappers around Tauri commands. One function per command in
// src-tauri/src/commands/; keep the two in sync and documented in
// docs/ARCHITECTURE.md ("Command API").
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type AppInfo = { app_version: string; core_version: string };
export type VaultStatus = "none" | "locked" | "unlocked";
export type Strength = { score: number; acceptable: boolean; feedback: string | null };

/** Every command rejects with this shape (src-tauri/src/error.rs). */
export type CommandError = {
  kind:
    | "wrong_password"
    | "wrong_recovery_key"
    | "invalid_recovery_key"
    | "weak_password"
    | "throttled"
    | "locked"
    | "already_exists"
    | "no_vault"
    | "not_found"
    | "invalid"
    | "no_microphone"
    | "mic_blocked"
    | "mic_error"
    | "already_recording"
    | "not_recording"
    | "wrong_passphrase"
    | "bad_backup"
    | "internal";
  message: string;
  retry_after_ms?: number;
};

export function asCommandError(e: unknown): CommandError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) return e as CommandError;
  return { kind: "internal", message: String(e) };
}

// ── Vault ────────────────────────────────────────────────────────────────
export const appInfo = () => invoke<AppInfo>("app_info");
export const vaultStatus = () => invoke<VaultStatus>("vault_status");
export const passwordStrength = (password: string) => invoke<Strength>("password_strength", { password });
/** Resolves to the recovery key, to be shown once. */
export const createVault = (password: string) => invoke<string>("create_vault", { password });
/** `recovered`: recordings cut short by a crash, turned into entries at this unlock. */
export const unlock = (password: string) => invoke<{ recovered: number }>("unlock", { password });
/** Resolves to the new recovery key, which replaces the used one. */
export const recover = (recoveryKey: string, newPassword: string) =>
  invoke<string>("recover", { recoveryKey, newPassword });
export const changePassword = (currentPassword: string, newPassword: string) =>
  invoke<void>("change_password", { currentPassword, newPassword });
/** Resolves to the new recovery key; the old one stops working. */
export const regenerateRecoveryKey = (currentPassword: string) =>
  invoke<string>("regenerate_recovery_key", { currentPassword });
export const lock = () => invoke<void>("lock");
/** Opens a native save dialog. Resolves false if the user cancelled. */
export const saveRecoveryKey = (recoveryKey: string) => invoke<boolean>("save_recovery_key", { recoveryKey });
/** Keyboard/pointer input happened; restarts the idle auto-lock countdown. */
export const activity = () => invoke<void>("activity");
/** The new recovery key is confirmed; auto-lock (idle and minimize) may resume. */
export const releaseAutolock = () => invoke<void>("release_autolock");

// ── Recording ────────────────────────────────────────────────────────────
export type InputDevice = { id: string; name: string; is_default: boolean };
export type DraftSegment = { t_ms: number; text: string };
export type Stopped = { id: string; duration_s: number; text: string };
export type RecordingStatus = {
  state: "idle" | "recording" | "paused" | "stopped";
  id: string | null;
  elapsed_ms: number;
  finals: DraftSegment[];
  stopped: Stopped | null;
};
export type Level = { level: number; elapsed_ms: number; paused: boolean };

export const listMicrophones = () => invoke<InputDevice[]>("list_microphones");
export const micTestStart = (deviceId: string | null) => invoke<void>("mic_test_start", { deviceId });
export const micTestStop = () => invoke<void>("mic_test_stop");
/** Resolves to the recording's ID (also the future entry's ID). */
export const startRecording = () => invoke<string>("start_recording");
export const pauseRecording = () => invoke<void>("pause_recording");
export const resumeRecording = () => invoke<void>("resume_recording");
export const recordingStatus = () => invoke<RecordingStatus>("recording_status");
export const stopRecording = () => invoke<Stopped>("stop_recording");
/** The save dialog. Resolves to the new entry's ID. */
export const saveRecording = (keepAudio: boolean, mood: number | null, tags: string[]) =>
  invoke<string>("save_recording", { keepAudio, mood, tags });

// ── Entries and search ───────────────────────────────────────────────────
export type EntrySummary = {
  id: string;
  created_at: number;
  updated_at: number;
  title: string | null;
  duration_s: number;
  mood: number | null;
  has_audio: boolean;
  tags: string[];
};
export type Entry = EntrySummary & { original_text: string; edited_text: string | null };
export type SearchQuery = { text: string; from?: number; to?: number; mood?: number; tag?: string };
/** `snippet` marks matches with U+E000 … U+E001 (see highlightSnippet). */
export type SearchHit = { entry: EntrySummary; snippet: string };

export const listEntries = () => invoke<EntrySummary[]>("list_entries");
export const getEntry = (id: string) => invoke<Entry>("get_entry", { id });
export const createTypedEntry = (text: string) => invoke<string>("create_typed_entry", { text });
export const setEntryTitle = (id: string, title: string | null) => invoke<void>("set_entry_title", { id, title });
export const setEntryText = (id: string, text: string) => invoke<void>("set_entry_text", { id, text });
export const revertEntry = (id: string) => invoke<void>("revert_entry", { id });
export const setEntryMood = (id: string, mood: number | null) => invoke<void>("set_entry_mood", { id, mood });
export const setEntryTags = (id: string, tags: string[]) => invoke<void>("set_entry_tags", { id, tags });
export const deleteEntry = (id: string) => invoke<void>("delete_entry", { id });
export const allTags = () => invoke<string[]>("all_tags");
/** Decrypted Ogg Opus bytes, for an in-memory blob: URL. */
export const entryAudio = (id: string) => invoke<ArrayBuffer>("entry_audio", { id });
export const searchEntries = (query: SearchQuery) => invoke<SearchHit[]>("search_entries", { query });

// ── Settings and backup ──────────────────────────────────────────────────
export type Settings = {
  idle_timeout_min: number;
  default_keep_audio: boolean;
  mic_device: string | null;
  backup_reminder_days: number | null;
  last_backup_at: number | null;
};
export type SettingsUpdate = Partial<Omit<Settings, "last_backup_at">>;

export const getSettings = () => invoke<Settings>("get_settings");
export const updateSettings = (update: SettingsUpdate) => invoke<Settings>("update_settings", { update });
/** Native save dialog. Resolves false if cancelled. */
export const createBackup = (usePassword: boolean, secret: string) =>
  invoke<boolean>("create_backup", { usePassword, secret });
/** Native open dialog; replaces the journal and leaves the app locked. Resolves false if cancelled. */
export const restoreBackup = (passphrase: string) => invoke<boolean>("restore_backup", { passphrase });

// ── Events (Rust → UI). Each resolves to an unsubscribe function. ────────
export const onLocked = (handler: () => void) => listen("locked", handler);
export const onLevel = (handler: (l: Level) => void) => listen<Level>("recording:level", (e) => handler(e.payload));
export const onPreview = (handler: (text: string) => void) => listen<string>("transcript:preview", (e) => handler(e.payload));
export const onFinal = (handler: (s: DraftSegment) => void) => listen<DraftSegment>("transcript:final", (e) => handler(e.payload));
