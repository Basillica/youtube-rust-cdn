use crate::pkg::cdn::Node;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug)]
pub struct CacheManager {
    pub notifier: broadcast::Sender<String>, // Centralized broadcast channel for invalidation
    pub app_to_nodes: RwLock<DashMap<String, Vec<Arc<Node>>>>, // App-to-node mapping
}

impl CacheManager {
    // Create a new CacheManager with a single global notifier
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        CacheManager {
            notifier: tx,
            app_to_nodes: RwLock::new(DashMap::new()),
        }
    }

    // Register a node to an app
    pub async fn register_node(&self, app_id: &str, node: Arc<Node>) {
        // let mut app_to_nodes = self.app_to_nodes.lock().await;
        self.app_to_nodes
            .write()
            .await
            .entry(app_id.to_string())
            .or_insert_with(Vec::new)
            .push(node);
    }

    // Notify specific nodes (targeted invalidation)
    pub async fn notify_nodes(&self, app_id: &str, key: String) {
        let val = self.app_to_nodes.write().await;
        if let Some(nodes) = val.get(app_id) {
            for node in nodes.iter() {
                node.invalidate_cache(&key).await;
            }
        };
    }

    pub async fn run_listener(&self) {
        let mut receiver = self.notifier.subscribe();
        loop {
            match receiver.recv().await {
                Ok(key) => {
                    self.handle_invalidation(key).await;
                }
                Err(e) => {
                    eprintln!("Error receiving broadcast: {}", e);
                }
            }
        }
    }

    // Handles cache invalidation across all registered nodes
    async fn handle_invalidation(&self, key: String) {
        let val = self.app_to_nodes.write().await;
        for nodes in val.iter() {
            for node in nodes.value().iter() {
                node.invalidate_cache(&key).await;
            }
        }
    }
}
