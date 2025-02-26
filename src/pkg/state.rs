use moka::future::Cache;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<Cache<String, String>>,
    pub invalidation_tx: mpsc::Sender<String>,
}
