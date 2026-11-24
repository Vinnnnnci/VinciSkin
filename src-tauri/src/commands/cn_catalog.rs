//! The Chinese client's hero and skin tables, read over IPC.
//!
//! The CDN serves one roster and one table per hero, so both commands are a
//! network round trip and run off the UI thread. Nothing is cached here: a
//! table is small, and a caller that wants it again asks the CDN again.

use serde::Serialize;
use ts_rs::TS;

use super::off_thread;
use crate::error::{AppError, AppResult, IpcResult};
use crate::events::TauriEventSink;
use crate::mods::ModLibraryState;
use crate::state::SettingsState;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use vinciskin_core::cn_catalog::{
    fetch_hero_skins, fetch_heroes, hero_portrait_url, is_trusted_image_url, patch_label, CnError,
    GtimgCdn, Hero, Quality, Skin,
};
use vinciskin_core::config::Config;
use vinciskin_core::static_swap::{ChampionSwap, PackSwap};

use crate::cn_auto::{AutoStatus, AutoSwapState};
use crate::patcher::{PatcherState, PatcherStatus, LOADER_EXE_NAME};

/// Sent with every request, the way the other publishers are told who is asking.
const USER_AGENT: &str = concat!("vinciskin/", env!("CARGO_PKG_VERSION"));

/// A hero as the Chinese client's own table names it.
#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CnHero {
    /// Riot's numeric hero id, which the per-hero table is addressed by.
    pub id: u32,
    /// The English name, `Annie`.
    pub alias: String,
    /// The Chinese title, `黑暗之女`.
    pub name: String,
    /// Every search term the client itself indexes the champion by: nickname, title, and the
    /// full and initial pinyin spellings. This is what lets `亚索`, `疾风剑豪` and `ys` all
    /// reach the same champion in the search box.
    pub keywords: Vec<String>,
    /// The small square the client lists the champion by, or none where the table's alias is not
    /// a plain identifier. Dropped here rather than handed to the webview when it is not.
    pub portrait_url: Option<String>,
}

/// How rare the Chinese client calls a skin, as the page's quality filter names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CnQuality {
    /// No tier word on the entry.
    Common,
    /// `Epic`.
    Epic,
    /// `Legend`.
    Legend,
    /// `Mythic`.
    Mythic,
    /// `Ultimate`.
    Ultimate,
    /// `Prestige`.
    Prestige,
    /// `Exalted`.
    Exalted,
    /// Limited to the run it was sold in.
    Limited,
}

impl From<Quality> for CnQuality {
    fn from(quality: Quality) -> Self {
        match quality {
            Quality::Common => Self::Common,
            Quality::Epic => Self::Epic,
            Quality::Legend => Self::Legend,
            Quality::Mythic => Self::Mythic,
            Quality::Ultimate => Self::Ultimate,
            Quality::Prestige => Self::Prestige,
            Quality::Exalted => Self::Exalted,
            Quality::Limited => Self::Limited,
        }
    }
}

/// A skin as the Chinese client's own table names it.
#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CnSkin {
    /// Riot's numeric skin id, `1000` for a base skin.
    pub skin_id: u32,
    /// The hero this skin belongs to.
    pub hero_id: u32,
    /// The Chinese skin name.
    pub name: String,
    /// Whether this is the hero's base skin.
    pub is_base: bool,
    /// Whether this entry is a chroma of the skin above it rather than a skin.
    pub is_chroma: bool,
    /// The skin this entry sets the look of, which is its own id unless it is a chroma.
    pub parent_id: u32,
    /// The tier the client labels the entry with, which the page filters by.
    pub quality: CnQuality,
    /// The collection the client labels the entry with, `Epic`, `Year of the Ox`.
    ///
    /// Shown beside the tier, because it is what a player recognises a skin by when the tier
    /// alone would group it with hundreds of others.
    pub emblems_name: Option<String>,
    /// The one image the catalog serves for this entry, or none when it has no
    /// art. The table's own `chromaImg` is not sent: it answers 404.
    pub art_url: Option<String>,
    /// The art's UUID, which the image URLs carry in place of the skin id.
    pub instance_id: Option<String>,
}

/// The hero roster, with the patch its table reports.
#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CnHeroRoster {
    /// The patch the table reports, `16.19`.
    pub version: String,
    /// Every hero that patch ships.
    pub heroes: Vec<CnHero>,
}

/// One hero's skins, with the patch its table reports.
#[derive(Debug, Clone, Serialize, TS, specta::Type)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CnHeroSkins {
    /// The patch the table reports, `16.19`.
    pub version: String,
    /// Every skin of the hero, chromas included.
    pub skins: Vec<CnSkin>,
}

/// Every hero the Chinese client ships.
#[tauri::command]
#[specta::specta]
pub async fn get_cn_heroes() -> IpcResult<CnHeroRoster> {
    off_thread(|| {
        let catalog = fetch_heroes(&cdn()?).map_err(unreachable)?;

        Ok(CnHeroRoster {
            version: patch_label(catalog.version),
            heroes: catalog.heroes.into_iter().map(hero).collect(),
        })
    })
    .await
}

/// One hero's skins, chromas included.
#[tauri::command]
#[specta::specta]
pub async fn get_cn_hero_skins(hero_id: u32) -> IpcResult<CnHeroSkins> {
    off_thread(move || {
        let catalog = fetch_hero_skins(&cdn()?, hero_id).map_err(unreachable)?;

        Ok(CnHeroSkins {
            version: patch_label(catalog.version),
            skins: catalog.skins.into_iter().map(skin).collect(),
        })
    })
    .await
}

/// An HTTP client for the Tencent CDN.
fn cdn() -> AppResult<GtimgCdn> {
    GtimgCdn::new(USER_AGENT).map_err(|error| AppError::Other(error.to_string()))
}

/// Every champion this machine has written a skin into, and where each of those archives stands.
#[tauri::command]
#[specta::specta]
pub async fn get_cn_static_swaps(app_handle: AppHandle) -> IpcResult<Vec<ChampionSwap>> {
    off_thread(move || {
        let (config, storage) = swap_context(&app_handle)?;
        vinciskin_core::static_swap::swaps(&config, &storage)
    })
    .await
}

/// Show `character` the hero's catalog skin `skin_id` by writing it into the overlay.
///
/// The game's own archive is left untouched; the injection host serves the
/// overlay. The base skin's id drops the overlay entry, which is the same call as
/// [`restore_cn_static_swap`].
#[tauri::command]
#[specta::specta]
pub async fn apply_cn_static_swap(
    character: String,
    hero_id: u32,
    skin_id: u32,
    app_handle: AppHandle,
) -> IpcResult<ChampionSwap> {
    let host = app_handle.state::<PatcherState>().status();

    off_thread(move || {
        let (config, storage) = swap_context(&app_handle)?;

        match vinciskin_core::static_swap::overlay_apply(
            &config, &storage, &character, skin_id, hero_id,
        ) {
            Ok(applied) => {
                tracing::info!(
                    champion = %applied.character,
                    skin = skin_id,
                    archive = %applied.archive,
                    host_running = host.running,
                    game_running = host.game_running,
                    "cn skin applied"
                );
                Ok(applied)
            }
            Err(error) => {
                tracing::warn!(
                    champion = %character,
                    skin = skin_id,
                    %error,
                    "cn skin apply failed"
                );
                Err(error)
            }
        }
    })
    .await
}

/// Drop `character`'s overlay entry, so the game reads its own archive again.
#[tauri::command]
#[specta::specta]
pub async fn restore_cn_static_swap(
    character: String,
    app_handle: AppHandle,
) -> IpcResult<ChampionSwap> {
    off_thread(move || {
        let (config, storage) = swap_context(&app_handle)?;
        let restored = vinciskin_core::static_swap::overlay_restore(&config, &storage, &character)?;
        tracing::info!(champion = %restored.character, "cn skin restored");
        Ok(restored)
    })
    .await
}

/// Forget a champion's swap record without touching its archive.
///
/// The way out of a swap the record no longer describes, which is where a game patch leaves one.
#[tauri::command]
#[specta::specta]
pub async fn forget_cn_static_swap(character: String, app_handle: AppHandle) -> IpcResult<()> {
    off_thread(move || {
        let (_, storage) = swap_context(&app_handle)?;
        vinciskin_core::static_swap::forget(&storage, &character)
    })
    .await
}

/// Every archive a skin pack has replaced, and where each of those archives stands.
#[tauri::command]
#[specta::specta]
pub async fn get_cn_skin_packs(app_handle: AppHandle) -> IpcResult<Vec<PackSwap>> {
    off_thread(move || {
        let (config, storage) = swap_context(&app_handle)?;
        vinciskin_core::static_swap::pack_swaps(&config, &storage)
    })
    .await
}

/// Write a skin pack the user picked over the install's own champion archives.
///
/// The pack is a `.fantome` or a `.wad.client`; what it carries replaces the archives it names,
/// each backed up first. The returned rows are what now stands written.
#[tauri::command]
#[specta::specta]
pub async fn import_cn_skin_pack(
    file_path: String,
    app_handle: AppHandle,
) -> IpcResult<Vec<PackSwap>> {
    off_thread(move || {
        let (config, storage) = swap_context(&app_handle)?;
        let events = TauriEventSink::new(app_handle.clone());
        vinciskin_core::static_swap::apply_pack(
            &config,
            &storage,
            std::path::Path::new(&file_path),
            &events,
        )
    })
    .await
}

/// Put an archive a pack replaced back to the one the client shipped.
#[tauri::command]
#[specta::specta]
pub async fn restore_cn_skin_pack(archive: String, app_handle: AppHandle) -> IpcResult<PackSwap> {
    off_thread(move || {
        let (config, storage) = swap_context(&app_handle)?;
        vinciskin_core::static_swap::restore_pack(&config, &storage, &archive)
    })
    .await
}

/// Forget a pack-written archive's record without touching the archive.
#[tauri::command]
#[specta::specta]
pub async fn forget_cn_skin_pack(archive: String, app_handle: AppHandle) -> IpcResult<()> {
    off_thread(move || {
        let (_, storage) = swap_context(&app_handle)?;
        vinciskin_core::static_swap::forget_pack(&storage, &archive)
    })
    .await
}

/// Fetch a skin pack from an allowed host and write it over the install's own archives.
///
/// The fetch lands under the app data directory and is removed once the write is over; the
/// address is checked against the hosts a pack is fetched from before anything is requested.
#[tauri::command]
#[specta::specta]
pub async fn download_cn_skin_pack(url: String, app_handle: AppHandle) -> IpcResult<Vec<PackSwap>> {
    off_thread(move || {
        let (config, storage) = swap_context(&app_handle)?;
        let scratch = crate::state::get_app_data_dir(&app_handle)
            .ok_or_else(|| AppError::Other("no application data directory".to_owned()))?
            .join("skin-pack-downloads");
        let events = TauriEventSink::new(app_handle.clone());

        vinciskin_core::static_swap::fetch_and_apply(&config, &storage, &url, &scratch, &events)
    })
    .await
}

/// A swap needs the install to read the skins from and a directory to keep its backups in.
fn swap_context(app_handle: &AppHandle) -> AppResult<(Config, PathBuf)> {
    let config = app_handle.state::<SettingsState>().config();
    let library = app_handle.state::<ModLibraryState>();
    let storage = library.inner().0.storage_dir(&config)?;

    Ok((config, storage))
}

/// The overlay directory the injected DLL redirects the game's WADs into.
fn overlay_dir(storage: &Path) -> PathBuf {
    storage.join("overlay")
}

/// Resolve a bundled resource (the injection host), from the app's resource
/// directory, falling back to next-to-executable and the crate's `resources/`.
fn resolve_resource(app_handle: &AppHandle, file_name: &str) -> AppResult<PathBuf> {
    let candidates = [
        app_handle
            .path()
            .resource_dir()
            .ok()
            .map(|d| d.join(file_name)),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join(file_name))),
        Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join(file_name),
        ),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(AppError::Other(format!(
        "{file_name} was not found in the app's resources"
    )))
}

/// Start the injection host over the built overlay.
///
/// The host scans for the game and injects the hook DLL, which redirects the
/// game's WAD reads into `<storage>/overlay`. Nothing is written into the game.
#[tauri::command]
pub fn start_cn_patcher(app_handle: AppHandle, patcher: State<PatcherState>) -> IpcResult<()> {
    start_cn_patcher_inner(&app_handle, &patcher).into()
}

fn start_cn_patcher_inner(app_handle: &AppHandle, patcher: &State<PatcherState>) -> AppResult<()> {
    let (config, storage) = swap_context(app_handle)?;

    /* An overlay built before a game patch mixes a chunk table with an archive
    the install has since replaced, which crashes the game, so those files are
    dropped before the host can serve them. */
    match vinciskin_core::static_swap::overlay_purge_stale(&config, &storage) {
        Ok(0) => {}
        Ok(dropped) => tracing::info!(dropped, "dropped overlays built from the previous install"),
        Err(error) => tracing::warn!("could not check the overlays: {error}"),
    }

    let overlay = overlay_dir(&storage);

    /* The loader ships beside the app in a bundled build, and inside it in a
    portable one, where it is unpacked under the app's own data directory. */
    let loader = match resolve_resource(app_handle, LOADER_EXE_NAME) {
        Ok(path) => path,
        Err(_) => {
            let dir = crate::state::get_app_data_dir(app_handle)
                .ok_or_else(|| AppError::Other("no app data directory for the loader".into()))?
                .join("loader");
            crate::patcher::unpack_loader(&dir)?
        }
    };

    let result = patcher.start(&loader, &overlay);
    if let Err(error) = &result {
        tracing::warn!(%error, overlay = %overlay.display(), "the injection host did not start");
    }

    result
}

/// Stop the injection host.
#[tauri::command]
pub fn stop_cn_patcher(patcher: State<PatcherState>) -> IpcResult<()> {
    patcher.stop().into()
}

/// What the injection host is doing now.
#[tauri::command]
pub fn get_cn_patcher_status(patcher: State<PatcherState>) -> IpcResult<PatcherStatus> {
    IpcResult::ok(patcher.status())
}

/// Follow the client's champ-select session and apply the skin it shows.
///
/// The watcher reads the client's own local API, so no click here is needed for
/// a picked skin to reach the overlay: walking into a game carries whatever the
/// carousel showed.
#[tauri::command]
pub fn start_cn_auto_swap(app_handle: AppHandle) -> IpcResult<()> {
    let watcher: State<AutoSwapState> = app_handle.state();
    watcher.start(&app_handle).into()
}

/// Stop following the client's champ-select session.
#[tauri::command]
pub fn stop_cn_auto_swap(app_handle: AppHandle) -> IpcResult<()> {
    let watcher: State<AutoSwapState> = app_handle.state();
    watcher.stop();
    IpcResult::ok(())
}

/// What the champ-select watcher is doing now.
#[tauri::command]
pub fn get_cn_auto_status(app_handle: AppHandle) -> IpcResult<AutoStatus> {
    let watcher: State<AutoSwapState> = app_handle.state();
    IpcResult::ok(watcher.status())
}

/// Remove the client loader, so the client starts on its own again.
#[tauri::command]
pub fn restore_cn_client() -> IpcResult<()> {
    crate::pengu::disable().into()
}

/// A CDN failure, as the opaque kind. A new enough catalog to deserve its own
/// remedy can carry one later; today the message is what a report shows.
fn unreachable(error: CnError) -> AppError {
    AppError::Other(error.to_string())
}

fn hero(hero: Hero) -> CnHero {
    let portrait_url = art(hero_portrait_url(&hero.alias));

    CnHero {
        id: hero.id,
        alias: hero.alias,
        name: hero.name,
        keywords: hero.keywords,
        portrait_url,
    }
}

fn skin(skin: Skin) -> CnSkin {
    /* Every read of the entry is taken before `name` moves out of it. */
    let is_chroma = skin.is_chroma();
    let art_url = art(skin.art_url());
    let parent_id = skin.parent_id();
    let quality = skin.quality().into();

    CnSkin {
        skin_id: skin.skin_id,
        hero_id: skin.hero_id,
        name: skin.name,
        is_base: skin.is_base,
        is_chroma,
        parent_id,
        quality,
        emblems_name: skin.emblems_name,
        art_url,
        instance_id: skin.instance_id,
    }
}

/// The art URL the frontend may draw, or none.
///
/// The CDN is the only host the app's images come from, so a URL naming
/// anywhere else is dropped here rather than handed to the webview.
fn art(url: Option<String>) -> Option<String> {
    url.filter(|url| is_trusted_image_url(url))
}
