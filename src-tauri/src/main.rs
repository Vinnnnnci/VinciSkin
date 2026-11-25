#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod bridge;
mod cn_auto;
mod commands;
mod deep_link;
mod error;
mod events;
mod hotkeys;
mod ipc;
mod lcu;
#[cfg(debug_assertions)]
mod log_layer;
mod logging;
mod mods;
mod patcher;
mod pengu;
mod protocol;
mod setup;
mod startup;
mod state;
mod tray;
mod workshop;

use tauri::webview::PageLoadEvent;
use tauri::Manager;
use vinciskin_core::bin_document::BinDocuments;

/// The one window the frontend runs in, as `tauri.conf.json` leaves it unlabelled.
const MAIN_WINDOW: &str = "main";

fn main() {
    let logging_guards = logging::init();
    logging::stage("app-start");

    tracing::info!("Starting VinciSkin v{}", env!("CARGO_PKG_VERSION"));
    if let Some(ref p) = logging_guards.log_path {
        tracing::info!("Log directory: {}", p.display());
        logging::cleanup_old_logs(p, 7);
    }

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            deep_link::handle_argv(app, &argv);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build());

    logging::stage("tauri-build-start");

    let app = builder
        /* The preview's pixels come this way rather than over IPC, so an
        `<img>` draws them with the webview's own decoder. */
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // A decode is tens of milliseconds, and this handler is the main thread.
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(protocol::answer(&app, &request));
            });
        })
        .manage(logging_guards)
        .setup(setup::run)
        /* A reload runs no cleanup, so the handles the last page held are dropped here,
        before the new page can open any. */
        .on_page_load(|webview, payload| {
            if payload.event() != PageLoadEvent::Started || webview.label() != MAIN_WINDOW {
                return;
            }
            if let Some(documents) = webview.try_state::<BinDocuments>() {
                documents.close_all();
            }
            if let Some(watches) = webview.try_state::<workshop::LayerWatches>() {
                watches.release_all();
            }
        })
        .invoke_handler(ipc::invoke_handler(tauri::generate_handler![
            // App
            commands::get_app_info,
            commands::log_ui_error,
            commands::get_platform_support,
            commands::show_main_window,
            // Settings
            commands::get_settings,
            commands::save_settings,
            commands::get_default_settings,
            commands::auto_detect_league_path,
            commands::validate_league_path,
            commands::check_setup_required,
            commands::detect_league_run_as_admin,
            // Mods
            commands::get_installed_mods,
            commands::install_mod,
            commands::update_mod,
            commands::install_mods,
            commands::uninstall_mod,
            commands::toggle_mod,
            commands::set_mod_layers,
            commands::enable_mod_with_layers,
            commands::edit_mod_metadata,
            commands::set_mod_storage,
            commands::check_mod_health,
            commands::repair_mod,
            commands::repair_mods,
            commands::get_mod_health_verdicts,
            #[cfg(debug_assertions)]
            commands::time_mod_health,
            commands::cancel_mod_health_run,
            commands::get_health_sweep,
            commands::sweep_mod_health,
            commands::get_health_check_readiness,
            commands::export_mods,
            commands::inspect_modpkg,
            commands::get_mod_thumbnail,
            commands::get_mod_thumbnails,
            commands::get_mod_readme,
            commands::get_mod_license_text,
            commands::get_storage_directory,
            commands::reorder_mods,
            commands::get_mod_wad_report,
            commands::get_all_mod_wad_reports,
            commands::analyze_mod_wads,
            // Folders
            commands::get_folders,
            commands::get_folder_order,
            commands::create_folder,
            commands::rename_folder,
            commands::delete_folder,
            commands::move_mod_to_folder,
            commands::toggle_folder,
            commands::reorder_folder_mods,
            commands::reorder_folders,
            // Migration
            commands::scan_cslol_mods,
            commands::import_cslol_mods,
            commands::get_layout_migration_state,
            // Launcher
            commands::launch_league,
            commands::cancel_launch,
            commands::stop_league,
            commands::get_launch_availability,
            commands::get_league_session,
            // Hotkeys
            commands::pause_hotkeys,
            commands::resume_hotkeys,
            commands::set_hotkey,
            commands::kill_league,
            // Profiles
            commands::list_mod_profiles,
            commands::get_active_mod_profile,
            commands::create_mod_profile,
            commands::delete_mod_profile,
            commands::switch_mod_profile,
            commands::rename_mod_profile,
            // Shell
            commands::reveal_in_explorer,
            commands::minimize_to_tray,
            // Storage
            commands::detect_storage_medium,
            // Workshop
            commands::get_workshop_projects,
            commands::create_workshop_project,
            commands::get_workshop_project,
            commands::get_project_content_tree,
            commands::save_project_config,
            commands::rename_workshop_project,
            commands::delete_workshop_project,
            commands::pack_workshop_project,
            commands::import_from_modpkg,
            commands::peek_fantome,
            commands::import_from_fantome,
            commands::import_from_git_repo,
            commands::validate_project,
            commands::set_project_thumbnail,
            commands::remove_project_thumbnail,
            commands::get_project_thumbnail,
            commands::save_layer_string_overrides,
            commands::search_string_keys,
            commands::lookup_string_values,
            commands::get_layer_content_path,
            commands::get_layer_info,
            commands::create_project_layer,
            commands::rename_project_layer,
            commands::delete_project_layer,
            commands::reorder_project_layers,
            commands::update_layer_description,
            commands::add_files_to_layer,
            commands::delete_layer_content,
            commands::get_project_editor_state,
            commands::save_project_editor_state,
            // Problems
            commands::analyze_project,
            commands::fix_problems,
            // Hashtables
            commands::get_hashtable_cache_status,
            commands::check_hashtable_updates,
            commands::sync_hashtables,
            // Game WADs
            commands::get_game_wads,
            commands::read_game_wad,
            // Game index
            commands::get_game_index,
            commands::read_game_dir,
            commands::refresh_game_index,
            commands::search_game_index,
            commands::find_in_game_index,
            // Extract to disk
            commands::plan_game_extract,
            commands::extract_game_files,
            commands::cancel_extract,
            // Asset preview
            commands::read_asset_info,
            commands::save_asset_copy,
            // Ritobin
            commands::detect_ritobin_integration,
            commands::open_asset_in_ritobin,
            // Deep Link
            commands::deep_link_install_mod,
            commands::take_pending_deep_link,
            // CN patcher (the injection host that redirects the game's WADs)
            commands::start_cn_patcher,
            commands::stop_cn_patcher,
            commands::get_cn_patcher_status,
            commands::force_stop_cn_loader,
            // CN auto-swap (the watcher that reads the client's own session)
            commands::start_cn_auto_swap,
            commands::stop_cn_auto_swap,
            commands::get_cn_auto_status,
            commands::restore_cn_client,
            // Support report (what a user sends instead of four separate files)
            commands::build_support_report,
            commands::save_support_report,
            // for dynamic icons
            tray::set_tray_state,
        ]))
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(error) => {
            tracing::error!(%error, "could not build the application");
            logging::stage_detail("tauri-build-failed", &error.to_string());
            startup::show_build_error(&error.to_string());
            return;
        }
    };

    logging::stage("tauri-build-ok");
    app.run(setup::handle_run_event);
}
