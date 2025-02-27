use moka::future::Cache;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::Duration;

#[derive(Debug)]
pub struct Node {
    pub id: String,                            // Unique identifier for the node
    pub cache: Arc<Cache<String, String>>,     // Cache associated with the node
    pub receiver: broadcast::Receiver<String>, // Receiver to listen for invalidation events
    pub address: String,                       // Address of the node
    pub weight: u32,                           // Weight for load balancing
}

impl Node {
    // Creates a new node with default values
    pub fn new(
        id: String,
        address: String,
        weight: u32,
        receiver: broadcast::Receiver<String>,
    ) -> Self {
        let cache = Cache::builder()
            .max_capacity(10_000) // Store up to 10,000 items
            .time_to_live(Duration::from_secs(60)) // Remove items after 60 seconds
            .time_to_idle(Duration::from_secs(30)) // Remove if unused for 30 seconds
            .weigher(|_key: &String, value: &String| value.len() as u32) // Weight based on string length
            .build();
        Node {
            id,
            cache: Arc::new(cache),
            receiver,
            address,
            weight,
        }
    }

    // Simulate cache invalidation for this node
    pub async fn invalidate_cache(&self, key: &str) {
        self.cache.invalidate(key).await;
    }
}
