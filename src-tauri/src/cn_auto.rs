//! Applies the skin the client's champ-select shows, without a click.
//!
//! The client already knows which champion and skin the local player has picked
//! and publishes both over its own local API. This module follows that session:
//! whenever the pick changes, the overlay the injection host serves is rebuilt
//! for the new skin, so walking into a game carries whatever was picked on the
//! carousel. No click in the manager is needed.
//!
//! Reading the API needs the lockfile the client writes at startup, which the
//! Chinese install does not leave behind. Where it is missing, a small plugin
//! loaded into the client's page reports the same session over its own event
//! stream instead.
//!
//! The poll runs on its own thread because the overlay write copies a champion
//! archive and the window that draws the manager must not wait on it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::bridge;
use crate::error::{AppError, AppResult};
use crate::lcu;
use crate::lcu::Selection;
use crate::mods::ModLibraryState;
use crate::pengu;
use crate::state::SettingsState;
use vinciskin_core::cn_catalog::{fetch_heroes, GtimgCdn, HeroCatalog};
use vinciskin_core::static_swap::overlay_apply;

/// How long between session reads. The client answers in milliseconds; this is
/// the floor, and what keeps a pick that changes every second from queueing
/// more than one write at a time.
const POLL: Duration = Duration::from_secs(2);

/// The client's user agent, as the CDN's own commands send it.
const USER_AGENT: &str = concat!("vinciskin/", env!("CARGO_PKG_VERSION"));

/// One auto-applied skin, as the page reports it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoApplied {
    /// The champion whose overlay was rebuilt.
    pub champion_id: u32,
    /// The skin the client had selected.
    pub skin_id: u32,
    /// The champion archive the overlay was written for, as the page names it.
    pub character: String,
}

/// What the watcher is doing now.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoStatus {
    /// Whether a watcher thread is running.
    pub running: bool,
    /// The champion the client's champ-select shows, so the page can follow it.
    pub current_champion: Option<u32>,
    /// The last skin it applied, while one is still what the client shows.
    pub applied: Option<AutoApplied>,
    /// The last failure, when there was one.
    pub error: Option<String>,
    /// Whether the client has to restart before the follow plugin loads.
    pub needs_client_restart: bool,
}

/// The managed watcher, held by Tauri for the commands.
#[derive(Default)]
pub struct AutoSwapState {
    running: Mutex<Option<Arc<AtomicBool>>>,
    status: Arc<Mutex<AutoStatus>>,
    bridge: Mutex<Option<bridge::Bridge>>,
}

impl AutoSwapState {
    /// Start following the client's champ-select session.
    ///
    /// Starting twice is not an error: the second call is a setting that was
    /// already on, and the thread it would start already runs. Each run gets a
    /// token of its own, so a thread a stop raced cannot outlive the run.
    pub fn start(&self, app_handle: &AppHandle) -> AppResult<()> {
        let token = {
            let mut current = self.running.lock();
            if current
                .as_ref()
                .is_some_and(|token| token.load(Ordering::SeqCst))
            {
                return Ok(());
            }

            let token = Arc::new(AtomicBool::new(true));
            *current = Some(Arc::clone(&token));
            token
        };

        let token_path =
            crate::state::get_app_data_dir(app_handle).map(|dir| dir.join("bridge-token"));

        let config = app_handle.state::<SettingsState>().config();
        let direct = lcu::endpoint(&config).is_some();
        let mut startup_error = None;
        let mut needs_client_restart = false;

        /* A client whose lockfile answers here is read over its own API, so
        nothing is loaded into it and no listener is started. */
        let (state, listener) = if direct {
            (None, None)
        } else {
            match bridge::start(token_path.as_deref()) {
                Ok(bridge) => {
                    /* The plugin is written before the loader check, so a plugin
                    that already loaded keeps working even when the registration is
                    something this app did not write. */
                    match pengu::prepare(app_handle, bridge.port, &bridge.token) {
                        Ok(prepared) => {
                            needs_client_restart = prepared.registration_changed;
                        }
                        Err(error) => {
                            tracing::warn!("client loader not prepared: {error}");
                            startup_error = Some(error.to_string());
                        }
                    }
                    (Some(Arc::clone(&bridge.state)), Some(bridge))
                }
                Err(error) => {
                    tracing::warn!("client bridge listener failed: {error}");
                    startup_error = Some(error.to_string());
                    (None, None)
                }
            }
        };

        /* A stop that ran while the bridge was being prepared leaves nothing to
        follow, and the listener it started has to go with it. */
        if !token.load(Ordering::SeqCst) {
            if let Some(bridge) = listener {
                bridge.stop();
            }
            return Ok(());
        }

        *self.bridge.lock() = listener;

        {
            let mut status = self.status.lock();
            status.error = startup_error;
            status.needs_client_restart = needs_client_restart;
        }

        tracing::info!(direct, needs_client_restart, "auto-swap watcher starting");

        let running = Arc::clone(&token);
        let status = Arc::clone(&self.status);
        let handle = app_handle.clone();

        std::thread::spawn(move || follow(&handle, running, status, state));
        Ok(())
    }

    /// Stop following the session, and stop the client loading the bridge.
    pub fn stop(&self) {
        if let Some(token) = self.running.lock().take() {
            token.store(false, Ordering::SeqCst);
        }

        if let Some(bridge) = self.bridge.lock().take() {
            bridge.stop();
        }

        if let Err(error) = pengu::disable() {
            tracing::warn!("could not remove the client loader: {error}");
        }

        let mut status = self.status.lock();
        status.error = None;
        status.needs_client_restart = false;
        status.current_champion = None;
    }

    /// What the watcher is doing now.
    pub fn status(&self) -> AutoStatus {
        let mut status = self.status.lock();
        status.running = self
            .running
            .lock()
            .as_ref()
            .is_some_and(|token| token.load(Ordering::SeqCst));
        status.clone()
    }
}

/// The watcher's loop: read the session, and rebuild the overlay on a change.
fn follow(
    app_handle: &AppHandle,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<AutoStatus>>,
    bridge: Option<Arc<Mutex<Option<Selection>>>>,
) {
    tracing::info!("auto-swap watcher started");

    let mut last: Option<(u32, u32)> = None;
    while running.load(Ordering::SeqCst) {
        match tick(app_handle, &mut last, bridge.as_ref()) {
            Ok((current, applied)) => set(&status, |s| {
                /* A read with no pick is the client not being up yet, not a
                reason to drop a bridge or loader failure from the status. */
                if current.is_some() {
                    s.error = None;
                    s.needs_client_restart = false;
                }
                s.current_champion = current;
                s.applied = applied.or_else(|| s.applied.take());
            }),
            Err(error) => {
                tracing::debug!("auto-swap tick: {error}");
                set(&status, |s| s.error = Some(error.to_string()));
            }
        }

        std::thread::sleep(POLL);
    }

    tracing::info!("auto-swap watcher stopped");
}

/// One session read: apply the pick if it changed, and say what was applied.
fn tick(
    app_handle: &AppHandle,
    last: &mut Option<(u32, u32)>,
    bridge: Option<&Arc<Mutex<Option<Selection>>>>,
) -> AppResult<(Option<u32>, Option<AutoApplied>)> {
    let settings = app_handle.state::<SettingsState>();
    let config = settings.config();

    let selection = match lcu::endpoint(&config) {
        Some(endpoint) => lcu::selection(endpoint),
        None => bridge.and_then(|state| *state.lock()),
    };

    let Some(selection) = selection else {
        // No client is up, so the last pick is out of date for the next session.
        *last = None;
        return Ok((None, None));
    };

    let current = Some(selection.champion_id);
    if selection.champion_id == 0 || selection.skin_id == 0 {
        return Ok((current, None));
    }

    /* The client reports its base skin as `<champion>000`, not as zero. Writing
    that would delete the overlay a reader applied here, so a base-skin session
    leaves the manager's own choice alone. */
    let base_skin = selection.champion_id.saturating_mul(1000);
    if selection.skin_id == base_skin {
        return Ok((current, None));
    }

    let key = (selection.champion_id, selection.skin_id);
    if *last == Some(key) {
        return Ok((current, None));
    }

    let character = character_for(app_handle, selection.champion_id)?;
    let library = app_handle.state::<ModLibraryState>();
    let storage = library.inner().0.storage_dir(&config)?;

    overlay_apply(
        &config,
        &storage,
        &character,
        selection.skin_id,
        selection.champion_id,
    )?;
    *last = Some(key);

    let applied = AutoApplied {
        champion_id: selection.champion_id,
        skin_id: selection.skin_id,
        character,
    };
    let _ = app_handle.emit("cn-auto-swap-applied", &applied);
    tracing::info!(
        champion = applied.champion_id,
        skin = applied.skin_id,
        "auto-applied a skin"
    );
    Ok((current, Some(applied)))
}

/// The champion archive a champion id is named after, from the CDN's own table.
///
/// The table is small and read once per watcher run: a pick is minutes apart at
/// the least, and the answer does not change while a patch is not installed.
fn character_for(app_handle: &AppHandle, champion_id: u32) -> AppResult<String> {
    let catalog = app_handle.state::<AutoCatalog>();
    let mut heroes = catalog.0.lock();

    if heroes.is_none() {
        let cdn = GtimgCdn::new(USER_AGENT).map_err(|error| AppError::Other(error.to_string()))?;
        *heroes = Some(fetch_heroes(&cdn).map_err(|error| AppError::Other(error.to_string()))?);
    }

    let hero = heroes
        .as_ref()
        .and_then(|catalog| catalog.heroes.iter().find(|hero| hero.id == champion_id))
        .ok_or_else(|| {
            AppError::ValidationFailed(format!("no hero in the table is id {champion_id}"))
        })?;

    Ok(hero.alias.clone())
}

/// The hero table, cached for as long as the app runs.
#[derive(Default)]
pub struct AutoCatalog(pub Mutex<Option<HeroCatalog>>);

/// Run `change` against the shared status.
fn set(status: &Arc<Mutex<AutoStatus>>, change: impl FnOnce(&mut AutoStatus)) {
    change(&mut status.lock());
}
