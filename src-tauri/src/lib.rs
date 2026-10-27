mod autolock;
mod commands;
mod error;
mod state;

use journal_core::vault::Vault;
use tauri::Manager;
use tauri::path::BaseDirectory;

use commands::{backup, entries, recording, settings, vault};

/// Bundled with the app (tauri.conf.json → bundle.resources); fetched for
/// development with `npm run fetch-model`.
const MODEL: &str = "resources/models/ggml-base.en.bin";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before any key exists in this process (docs/SPEC.md "Rules").
    for failure in journal_core::harden_process() {
        eprintln!("hardening: {failure}");
    }

    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let model = app.path().resolve(MODEL, BaseDirectory::Resource)?;
            app.manage(state::AppState::new(Vault::at(dir), model));
            autolock::spawn_idle_watcher(app.handle().clone());
            recording::spawn_level_meter(app.handle().clone());
            Ok(())
        })
        .on_window_event(autolock::on_window_event)
        .invoke_handler(tauri::generate_handler![
            vault::app_info,
            vault::vault_status,
            vault::password_strength,
            vault::create_vault,
            vault::unlock,
            vault::recover,
            vault::change_password,
            vault::regenerate_recovery_key,
            vault::lock,
            vault::activity,
            vault::release_autolock,
            vault::save_recovery_key,
            recording::list_microphones,
            recording::mic_test_start,
            recording::mic_test_stop,
            recording::start_recording,
            recording::pause_recording,
            recording::resume_recording,
            recording::recording_status,
            recording::stop_recording,
            recording::save_recording,
            entries::list_entries,
            entries::get_entry,
            entries::create_typed_entry,
            entries::set_entry_title,
            entries::set_entry_text,
            entries::revert_entry,
            entries::set_entry_mood,
            entries::set_entry_tags,
            entries::delete_entry,
            entries::all_tags,
            entries::entry_audio,
            entries::search_entries,
            settings::get_settings,
            settings::update_settings,
            backup::create_backup,
            backup::restore_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
