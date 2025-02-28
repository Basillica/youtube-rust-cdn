use moka::future::Cache;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::Duration;

#[derive(Debug)]
pub struct Node {
    pub id: String,                        // Unique identifier for the node
    pub cache: Arc<Cache<String, String>>, // Cache associated with the node
    pub receiver: broadcast::Receiver<(String, String, String)>, // Receiver to listen for invalidation events
    pub address: String,                                         // Address of the node
    pub weight: usize,                                           // Weight for load balancing
    pub is_healthy: Arc<AtomicBool>,                             // Health status of the node
}

impl Node {
    // Creates a new node with default values
    pub fn new(
        id: String,
        address: String,
        weight: usize,
        receiver: broadcast::Receiver<(String, String, String)>,
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
            is_healthy: Arc::new(AtomicBool::new(true)),
        }
    }

    // Simulate cache invalidation for this node
    pub async fn invalidate_cache(&self, key: &str) {
        self.cache.invalidate(key).await;
    }

    pub async fn check_health(&self) -> bool {
        return true;
    }

    pub async fn process_request(&self, request_body: String) -> Result<String, String> {
        // Example: Read from cache or insert new data
        let response = if let Some(value) = self.cache.get(&request_body).await {
            format!("Cache hit: {}", value)
        } else {
            self.cache
                .insert(request_body.clone(), "GeneratedResponse".to_string())
                .await;
            format!("Cache miss: GeneratedResponse")
        };
        Ok(response)
    }
}
