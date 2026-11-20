mod app_cache;
mod app_log;
mod archive;
mod association;
mod commands;
mod data_packs;
mod instance;
mod neural_sr;
mod ocr;
mod ocr_assets;
mod open;
mod protocol;
mod state;
mod secrets;
mod system_fonts;
mod translation;
mod update;
mod window_state;

#[cfg(windows)]
mod webview2_check;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Portable zip has no installer bootstrapper — fail before creating a window.
    #[cfg(windows)]
    if !webview2_check::ensure_runtime_or_prompt() {
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState::new())
        .register_asynchronous_uri_scheme_protocol("comic", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let response = protocol::respond(&app, request.uri());
                responder.respond(response);
            });
        })
        .register_asynchronous_uri_scheme_protocol("neural", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let response = protocol::neural_respond(&app, request.uri());
                responder.respond(response);
            });
        })
        .setup(|app| {
            // First live reader process only — secondary launches skip auto-resume.
            let _ = instance::claim_primary_instance();
            app_log::init(app.handle());
            if let Some(path) = open::take_launch_args() {
                *state::lock(&app.state::<AppState>().pending_open) = Some(path);
            }
            // Restore size/position while still hidden, then show (see tauri.conf visible:false).
            window_state::restore_and_show(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_cache::get_cache_dir,
            app_cache::get_default_data_dir,
            app_cache::get_reader_store_path,
            app_cache::set_cache_dir,
            app_cache::migrate_cache_dir,
            app_log::append_log,
            app_log::get_log_history,
            app_log::clear_logs,
            app_log::open_log_window,
            app_log::close_log_window,
            app_log::sync_log_window_ui,
            commands::open_path,
            commands::start_measure,
            commands::load_bookmarks,
            commands::save_bookmarks,
            commands::delete_bookmarks,
            commands::save_page,
            commands::page_file_info,
            commands::system_info,
            commands::take_pending_open,
            commands::is_primary_instance,
            commands::get_shell_integration,
            commands::set_context_menu,
            commands::apply_file_associations,
            commands::path_stamp,
            commands::relaunch_app,
            update::check_app_update,
            neural_sr::models::list_image_assets,
            neural_sr::models::install_image_asset,
            neural_sr::models::remove_image_asset,
            neural_sr::cache::get_neural_cache_dir,
            neural_sr::cache::get_neural_results_dir,
            neural_sr::cache::get_runtime_cache_dir,
            neural_sr::results::neural_page_fingerprints,
            neural_sr::results::neural_results_exists,
            neural_sr::results::neural_results_status,
            neural_sr::results::clear_neural_results,
            neural_sr::runtime::neural_runtime_status,
            neural_sr::runtime::init_neural_runtime,
            neural_sr::download::test_download_proxy,
            data_packs::get_data_manifest,
            data_packs::refresh_data_manifest,
            neural_sr::infer::neural_upscale,
            neural_sr::infer::neural_result_load,
            neural_sr::infer::cancel_neural_upscale,
            ocr_assets::list_ocr_assets,
            ocr_assets::install_ocr_asset,
            ocr_assets::remove_ocr_asset,
            ocr_assets::get_ocr_cache_dir,
            ocr::ocr_translate_page,
            ocr::cancel_ocr_translate,
            translation::translate_text_cmd,
            translation::list_ai_models,
            system_fonts::list_system_fonts,
            secrets::secrets_encryption_status,
            secrets::secrets_translation_get,
            secrets::secrets_translation_set,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(
            #[allow(unused_variables)]
            |app, event| {
                if let tauri::RunEvent::WindowEvent { label, event, .. } = &event {
                    if matches!(event, tauri::WindowEvent::Destroyed)
                        && !app_log::is_log_window_label(label)
                    {
                        let _ = app_log::close_log_window(app.clone(), label.clone());
                    }
                }
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                if let tauri::RunEvent::Opened { urls } = event {
                    let files: Vec<_> = urls
                        .into_iter()
                        .filter_map(|url| url.to_file_path().ok())
                        .collect();
                    if let Some(path) = open::first_openable(&files) {
                        open::queue_pending(app, path);
                    }
                }
            },
        );
}
