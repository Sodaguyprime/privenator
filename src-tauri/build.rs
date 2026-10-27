/// Every IPC command. Declaring them here makes Tauri deny any command not
/// explicitly allowed in capabilities/ (the default is allow-all).
const COMMANDS: &[&str] = &[
    "app_info",
    "vault_status",
    "password_strength",
    "create_vault",
    "unlock",
    "recover",
    "change_password",
    "regenerate_recovery_key",
    "lock",
    "activity",
    "release_autolock",
    "save_recovery_key",
    "list_microphones",
    "mic_test_start",
    "mic_test_stop",
    "start_recording",
    "pause_recording",
    "resume_recording",
    "recording_status",
    "stop_recording",
    "save_recording",
    "list_entries",
    "get_entry",
    "create_typed_entry",
    "set_entry_title",
    "set_entry_text",
    "revert_entry",
    "set_entry_mood",
    "set_entry_tags",
    "delete_entry",
    "all_tags",
    "entry_audio",
    "search_entries",
    "get_settings",
    "update_settings",
    "create_backup",
    "restore_backup",
];

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))
        .expect("failed to run tauri-build");
}
