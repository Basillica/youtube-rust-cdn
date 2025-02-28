use crate::cdn::Node;
use dashmap::DashMap;
use rand::Rng;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tracing::info;

#[derive(Debug, Clone)]
pub enum LoadBalancingStrategy {
    BasicRoundRobin,
    WeightedRoundRobin,
}

#[derive(Debug)]
pub struct LoadBalancer {
    strategy: LoadBalancingStrategy,
    nodes: Arc<DashMap<String, Vec<Arc<Node>>>>, // Map of app_id to nodes
    current_index: Mutex<DashMap<String, usize>>, // To track the last used node for each app
    counter: DashMap<String, usize>,
}

impl LoadBalancer {
    pub fn new(
        strategy: LoadBalancingStrategy,
        nodes: Arc<DashMap<String, Vec<Arc<Node>>>>,
    ) -> Self {
        Self {
            strategy,
            nodes,
            current_index: Mutex::new(DashMap::new()),
            counter: DashMap::new(),
        }
    }
}

impl LoadBalancer {
    pub async fn select_node(&self, app_id: &str) -> Option<Arc<Node>> {
        let nodes = self.nodes.get(app_id)?;
        let node_list = nodes.value();

        if node_list.is_empty() {
            return None;
        }

        let index_map = self.current_index.lock().await;
        let mut current_index = index_map.entry(app_id.to_string()).or_insert(0);
        // Select node using round-robin
        let node = node_list.get(*current_index).cloned();
        // Update the index for next selection
        *current_index = (*current_index + 1) % node_list.len();

        node
    }

    pub async fn select_weighted_node(&self, app_id: &str) -> Option<Arc<Node>> {
        let nodes = self.nodes.get(app_id)?;
        let node_list = nodes.value();

        if node_list.is_empty() {
            return None;
        }

        // Calculate total weight of healthy nodes
        let total_weight: usize = node_list
            .iter()
            .filter(|node| node.is_healthy.load(Ordering::Relaxed))
            .map(|node| node.weight as usize) // Explicitly convert to usize
            .sum();

        if total_weight == 0 {
            return None; // No healthy nodes
        }

        // Randomized selection based on weight
        let mut rng = rand::rng();
        let mut selection = rng.random_range(0..total_weight);

        for node in node_list {
            if node.is_healthy.load(Ordering::Relaxed) {
                if selection < node.weight {
                    return Some(node.clone());
                }
                selection -= node.weight;
            }
        }

        for node in node_list {
            if node.is_healthy.load(Ordering::Relaxed) {
                if selection < node.weight {
                    return Some(node.clone());
                }
                selection -= node.weight;
            }
        }

        None
    }

    pub async fn run_health_checks(&self) {
        loop {
            for node_list in self.nodes.iter() {
                for node in node_list.value().iter() {
                    let is_healthy = node.check_health().await; // Assume this function checks if the node is alive
                    node.is_healthy.store(is_healthy, Ordering::Relaxed);
                }
            }

            tokio::time::sleep(Duration::from_secs(10)).await; // Check every 10 seconds
        }
    }

    pub async fn select_best_node(&self, app_id: &str, nodes: Vec<Arc<Node>>) -> Option<Arc<Node>> {
        // let app_to_nodes = cache_manager.app_to_nodes.read().await;
        // if let Some(nodes) = self.nodes.get(app_id) {
        match self.strategy {
            LoadBalancingStrategy::BasicRoundRobin => {
                let mut count = self.counter.entry(app_id.to_string()).or_insert(0);
                let index = *count % nodes.len();
                *count += 1;
                return Some(nodes[index].clone());
            }
            LoadBalancingStrategy::WeightedRoundRobin => {
                let mut rng = rand::rng();
                let total_weight: usize = nodes.iter().map(|node| node.weight as usize).sum();
                let mut choice = rng.random_range(0..total_weight);
                for node in nodes.iter() {
                    if choice < node.weight {
                        return Some(node.clone());
                    }
                    choice -= node.weight;
                }
            }
        }
        // }
        None
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod test {
    use crate::cache::CacheManager;
    use crate::pkg::loadbalancer::Node;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn test_dashmap_clone() {
        let cache_manager = Arc::new(CacheManager::new());
        let cloned_cache_manager = cache_manager.clone();
        let (tx, _) = broadcast::channel::<(String, String, String)>(100);
        let r1 = tx.subscribe();

        // Create nodes
        let node1 = Arc::new(Node::new(
            "node1".to_string(),
            "127.0.0.1:8080".to_string(),
            10,
            r1,
        ));

        // Add a node to the original CacheManager
        cache_manager
            .add_node_to_app("app1".to_string(), node1)
            .await;

        // Check if cloned_cache_manager sees the change
        let nodes = cloned_cache_manager
            .get_nodes_for_app("app1")
            .await
            .unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "node1");
    }
}
