use std::path::PathBuf;
use std::sync::Arc;

use aidash_core::db::Database;
use aidash_core::events::CoreEvent;
use aidash_core::lifecycle::LifecycleHandle;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

pub struct AppState {
    pub project_root: PathBuf,
    pub db: Arc<Database>,
    pub bench_events_tx: Option<broadcast::Sender<CoreEvent>>,
    pub bench_task: Option<JoinHandle<()>>,
    pub bench_profile_id: Option<String>,
    pub serve_handle: Option<LifecycleHandle>,
    pub serve_events_tx: Option<broadcast::Sender<CoreEvent>>,
    pub serve_profile_id: Option<String>,
    pub download_task: Option<JoinHandle<()>>,
    pub download_cancel_tx: Option<watch::Sender<bool>>,
    pub download_repo_id: Option<String>,
}

impl AppState {
    pub fn new(project_root: PathBuf) -> Result<Self, String> {
        let db = Database::open(None).map_err(|e| e.to_string())?;
        Ok(Self {
            project_root,
            db: Arc::new(db),
            bench_events_tx: None,
            bench_task: None,
            bench_profile_id: None,
            serve_handle: None,
            serve_events_tx: None,
            serve_profile_id: None,
            download_task: None,
            download_cancel_tx: None,
            download_repo_id: None,
        })
    }
}

static APP: OnceCell<Arc<Mutex<AppState>>> = OnceCell::new();

pub fn set_state(state: AppState) -> Result<(), String> {
    APP.set(Arc::new(Mutex::new(state)))
        .map_err(|_| "already initialized".to_string())
}

pub fn with_state<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut AppState) -> Result<T, String>,
{
    let cell = APP.get().ok_or_else(|| "not initialized — call init() first".to_string())?;
    f(&mut cell.lock())
}

pub fn state_arc() -> Result<Arc<Mutex<AppState>>, String> {
    APP.get()
        .cloned()
        .ok_or_else(|| "not initialized — call init() first".to_string())
}