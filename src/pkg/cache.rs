use crate::pkg::{
    cdn::Node,
    loadbalancer::{LoadBalancer, LoadBalancingStrategy},
};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::info;

#[derive(Debug)]
pub struct CacheManager {
    pub notifier: broadcast::Sender<(String, String, String)>, // Centralized broadcast channel for invalidation
    pub app_to_nodes: RwLock<DashMap<String, Vec<Arc<Node>>>>, // App-to-node mapping
    pub load_balancer: LoadBalancer,                           // Load balancer for routing traffic
}

impl CacheManager {
    // Create a new CacheManager with a single global notifier
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        let app_to_nodes = DashMap::new();
        CacheManager {
            notifier: tx,
            app_to_nodes: RwLock::new(app_to_nodes.clone()),
            load_balancer: LoadBalancer::new(
                LoadBalancingStrategy::WeightedRoundRobin,
                Arc::new(app_to_nodes),
            ), // Default to weighted
        }
    }

    /// Selects the best node for handling a request using the load balancer.
    pub async fn get_best_node(
        &self,
        app_id: &str,
        nodes: Option<Vec<Arc<Node>>>,
    ) -> Option<Arc<Node>> {
        if let Some(n) = nodes {
            return self.load_balancer.select_best_node(app_id, n).await;
        }
        return None;
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
                Ok((app_id, node_id, key)) => {
                    self.notify_nodes(&app_id, key.clone()).await; // all nodes
                    self.handle_invalidation(app_id, node_id, key).await; // just from one node
                }
                Err(e) => {
                    eprintln!("Error receiving broadcast: {}", e);
                }
            }
        }
    }

    // Handles cache invalidation across all registered nodes
    async fn handle_invalidation(&self, app_id: String, node_id: String, key: String) {
        let val = self.app_to_nodes.write().await;
        if let Some(nodes) = val.iter().find(|node| *node.key() == app_id) {
            let node_op = nodes.iter().find(|n| n.id == node_id);
            match node_op {
                Some(node) => {
                    node.invalidate_cache(&key).await;
                }
                None => return,
            }
        };
    }

    pub async fn add_node_to_app(&self, app_id: String, node: Arc<Node>) {
        let app_to_nodes = self.app_to_nodes.write().await;
        app_to_nodes
            .entry(app_id)
            .or_insert_with(Vec::new)
            .push(node);
    }

    pub async fn get_nodes_for_app(&self, app_id: &str) -> Option<Vec<Arc<Node>>> {
        let app_to_nodes = self.app_to_nodes.read().await;
        app_to_nodes.get(app_id).map(|nodes| nodes.clone())
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod test {
    use crate::cache::CacheManager;
    use crate::cdn::Node;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tokio::task;

    #[tokio::test]
    async fn test_invalidation() {
        // Create CacheManager
        let cache_manager = Arc::new(CacheManager::new());
        let (tx, _) = broadcast::channel::<(String, String, String)>(100);
        let r1 = tx.subscribe();
        let r2 = tx.subscribe();

        // Run listener task in background
        let cache_manager_clone = cache_manager.clone();
        task::spawn(async move {
            cache_manager_clone.run_listener().await;
        });

        // Create nodes
        let node1 = Arc::new(Node::new(
            "Node 1".to_string(),
            "127.0.0.1:8080".to_string(),
            10,
            r1,
        ));
        let node2 = Arc::new(Node::new(
            "Node 2".to_string(),
            "127.0.0.1:8081".to_string(),
            5,
            r2,
        ));

        // Register nodes to an app
        cache_manager.register_node("App1", node1.clone()).await;
        cache_manager.register_node("App1", node2.clone()).await;

        // Insert some data into the cache
        node1
            .cache
            .insert("test_key".to_string(), "value1".to_string())
            .await;
        node2
            .cache
            .insert("test_key".to_string(), "value2".to_string())
            .await;

        // Validate cache insertion
        assert_eq!(
            node1.cache.get(&"test_key".to_string()).await,
            Some("value1".to_string())
        );
        assert_eq!(
            node2.cache.get(&"test_key".to_string()).await,
            Some("value2".to_string())
        );

        // Notify all nodes for invalidation
        cache_manager
            .notify_nodes("App1", "test_key".to_string())
            .await;

        // Ensure cache is invalidated
        assert_eq!(node1.cache.get(&"test_key".to_string()).await, None);
        assert_eq!(node2.cache.get(&"test_key".to_string()).await, None);
    }
}
