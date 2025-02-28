use crate::pkg::cache::CacheManager;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug)]
pub struct AppState {
    pub cache_manager: Arc<CacheManager>,
    pub notifier: broadcast::Sender<(String, String, String)>,
    // pub load_balancer: Arc<LoadBalancer>,
}

impl AppState {
    pub fn new(
        cache_manager: Arc<CacheManager>,
        notifier: broadcast::Sender<(String, String, String)>,
    ) -> Self {
        Self {
            cache_manager,
            notifier,
        }
    }
}
