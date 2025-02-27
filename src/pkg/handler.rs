use super::{cdn, state::AppState};
use actix_web::{web, HttpResponse, Responder};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeP {
    pub id: String,      // Unique identifier for the node
    pub app_id: String,  // Unique identifier for
    pub address: String, // Address of the node
    pub weight: u32,     // Weight for load balancing
    pub key: String,     //
    pub value: String,   // Value
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeUpdate {
    pub id: String,     // Unique identifier for the node
    pub app_id: String, // Unique identifier for
    pub key: String,    //
    pub value: String,  // Value
}
// Retrieve cached item
pub async fn get_cache(
    state: web::Data<AppState>,
    path: web::Path<(String, String, String)>,
) -> impl Responder {
    let (app_id, node_id, key) = path.into_inner();

    let val = state.cache_manager.app_to_nodes.write().await;

    if let Some(nodes) = val.get_mut(&app_id) {
        if let Some(found) = nodes.iter().find(|node| node.id == node_id) {
            if let Some(c) = found.cache.get(&key).await {
                let resp = json!({
                    "node_id": found.id,
                    "address": found.address,
                    "weight": found.weight,
                    "cached": c,
                });
                return HttpResponse::Ok().json(resp);
            }
            return HttpResponse::BadRequest().body("the key is not a valid");
        }
        return HttpResponse::BadRequest().body("Invalid node_id");
    }

    HttpResponse::BadRequest().body("Invalid app_id")
}

pub async fn insert_new_cache(
    state: web::Data<AppState>,
    payload: web::Json<NodeP>,
) -> impl Responder {
    let payload = payload.into_inner();

    // Insert app_id into app_to_nodes if it doesn’t exist
    state
        .cache_manager
        .app_to_nodes
        .write()
        .await
        .entry(payload.app_id.clone())
        .or_insert_with(|| vec![]);

    // Retrieve or create the node list for this app
    let val = state.cache_manager.app_to_nodes.write().await;
    let mut nodes = val.get_mut(&payload.app_id).unwrap();

    // Check if the node already exists, if not, add it
    if !nodes.iter().any(|node| node.id == payload.app_id) {
        let sender = state.notifier.subscribe();
        let new_node = Arc::new(cdn::Node::new(
            payload.id.clone(),
            payload.address,
            payload.weight,
            sender,
        )); // Assuming Node::new exists
        nodes.push(new_node);
    }

    if let Some(found) = nodes.iter().find(|node| node.id == payload.id.clone()) {
        found.cache.insert(payload.key.clone(), payload.value).await;
    }

    HttpResponse::Ok().body(format!(
        "Inserted cache  for app '{}' and node '{}'",
        payload.app_id, payload.id
    ))
}

// Insert into cache with automatic TTL expiration
pub async fn insert_cache(
    state: web::Data<AppState>,
    payload: web::Json<NodeUpdate>,
) -> impl Responder {
    let payload = payload.into_inner();

    // Check if app_id exists in app_to_nodes
    let val = state.cache_manager.app_to_nodes.write().await;
    if let Some(nodes) = val.get_mut(&payload.app_id) {
        // Check if node_id exists for this app
        if let Some(found) = nodes.iter().find(|node| node.id == payload.id) {
            found.cache.insert(payload.key.clone(), payload.value).await;
            return HttpResponse::Ok().body(format!("Inserted cache key: {}", payload.key));
        }
        return HttpResponse::BadRequest().body("Invalid node_id");
    }
    return HttpResponse::BadRequest().body("Invalid app_id or node_id");
}

// Invalidate cache (via webhook)
pub async fn invalidate_cache(
    state: web::Data<AppState>,
    path: web::Path<(String, String, String)>,
) -> impl Responder {
    let (app_id, node_id, key) = path.into_inner();

    let val = state.cache_manager.app_to_nodes.write().await;
    let nodes = val.get(&app_id);
    if let Some(nodes) = nodes {
        if nodes.iter().any(|node| node.id == node_id) {
            let _ = state
                .notifier
                .send(format!("{}:{}:{}", app_id, node_id, key));
            return HttpResponse::Ok().body(format!("Invalidation request sent for key: {}", key));
        }
    }

    HttpResponse::BadRequest().body("Invalid app_id or node_id")
}

pub async fn start_invalidation_listener(
    mut rx: broadcast::Receiver<String>,
    cache: Arc<Cache<String, String>>,
) {
    while let Ok(key) = rx.recv().await {
        cache.invalidate(&key).await;
        println!("[Node] Invalidated key: {}", key);
    }
}

mod test {
    use super::*;
    use crate::cache::CacheManager;
    use crate::cdn::Node;
    use actix_web::{test, web, App};
    use moka::future::Cache;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tokio::task;
    use tokio::time::sleep;
    use tokio::time::Duration;
    use tokio::time::Instant;

    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache = Arc::new(
            Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
        );
        // Insert some items
        cache
            .insert("test_key".to_string(), "test_value".to_string())
            .await;

        // Retrieve and validate cache
        let cached_value = cache.get("test_key").await;
        assert_eq!(cached_value, Some("test_value".to_string()));

        // Invalidate the cache
        let key_to_invalidate = "test_key".to_string();
        cache.invalidate(&key_to_invalidate).await;

        // Ensure the cache is invalidated (i.e., not found)
        let cached_value_after_invalidation = cache.get("test_key").await;
        assert_eq!(cached_value_after_invalidation, None);
    }

    #[tokio::test]
    async fn test_cache_invalidation_with_mock_server() {
        let (tx, mut rx) = broadcast::channel::<String>(100);
        let cache_manager = Arc::new(CacheManager::new());
        let app_state = web::Data::new(AppState::new(cache_manager.clone(), tx.clone()));

        // Start a mock Actix Web server
        let server = test::init_service({
            App::new()
                .app_data(app_state.clone())
                .route("/cache/insert/new", web::post().to(insert_new_cache))
                .route("/cache/insert", web::post().to(insert_cache))
                .route("/cache/{app_id}/{node_id}/{key}", web::get().to(get_cache))
                .route(
                    "/invalidate/{app_id}/{node_id}/{key}",
                    web::delete().to(invalidate_cache),
                )
        })
        .await;

        let cache_manager_clone = cache_manager.clone();
        task::spawn(async move {
            let _ = cache_manager_clone.run_listener();
        });

        let n1 = tx.subscribe();
        let node1 = Arc::new(Node::new("node-1".into(), "127.0.0.1:8081".into(), 1, n1));
        cache_manager.register_node("app-1", node1).await;

        // Insert a new cache item via HTTP POST
        let value = json!({
            "id": "someboringid",
            "app_id": "soemboringappid",
            "address": "127.0.0.1:8083",
            "weight": 4,
            "key": "somekey",
            "value": "somevalue"
        });

        let req = test::TestRequest::post()
            .uri("/cache/insert/new")
            .set_json(value)
            .to_request();
        let resp = test::call_service(&server, req).await;
        let body = resp.into_body();
        println!("the frigging body {:?}", body);

        // Insert a cache item via HTTP POST for an existing cdn node
        let value = json!({
            "id": "someboringid",
            "app_id": "soemboringappid",
            "key": "someotherkey",
            "value": "someothervalue"
        });
        let req = test::TestRequest::post()
            .uri("/cache/insert")
            .set_json(value)
            .to_request();
        let resp = test::call_service(&server, req).await;
        let body = resp.into_body();
        println!("the frigging body {:?}", body);

        // Now, validate that the cache has the item
        let c_req = test::TestRequest::get()
            .uri("/cache/soemboringappid/someboringid/someotherkey")
            .to_request();
        let response = test::call_service(&server, c_req).await;
        assert_eq!(response.status(), 200);
        let body = response.into_body();
        println!("the frigging body {:?}", body);
        // assert_eq!(body, web::Bytes::from_static(b"test_value"));

        // Invalidate the cache via HTTP DELETE
        let d_req = test::TestRequest::delete()
            .uri("/invalidate/soemboringappid/someboringid/someotherkey")
            .to_request();
        let response = test::call_service(&server, d_req).await;
        assert_eq!(response.status(), 200);
        let body = response.into_body();
        // assert_eq!(body.into(), "Cache for key 'test_key' invalidated");
        println!("the frigging body {:?}", body);

        // Simulate receiving invalidation notification via the channel
        tokio::spawn(async move {
            assert_eq!(rx.recv().await.unwrap(), "test_key".to_string());
        });

        // Verify that the cache is now invalidated (cache miss)
        let f_req = test::TestRequest::delete()
            .uri("/cache/soemboringappid/someboringid/someotherkey")
            .to_request();
        let response = test::call_service(&server, f_req).await;
        assert_eq!(response.status(), 404); // Should return "Cache miss"
    }

    #[tokio::test]
    async fn test_cache_invalidation_with_multiple_listeners() {
        let (tx, rx) = broadcast::channel::<String>(100);
        let cache_manager = Arc::new(CacheManager::new());
        let app_state = web::Data::new(AppState::new(cache_manager.clone(), tx.clone()));

        // Start a mock Actix Web server
        let server = test::init_service({
            App::new()
                .app_data(app_state.clone())
                .route("/cache/insert/new", web::post().to(insert_new_cache))
                .route("/cache/insert", web::post().to(insert_cache))
                .route("/cache/{app_id}/{node_id}/{key}", web::get().to(get_cache))
                .route(
                    "/invalidate/{app_id}/{node_id}/{key}",
                    web::delete().to(invalidate_cache),
                )
        })
        .await;

        // Spawn multiple distributed listeners (simulating multiple nodes)
        let mut listener2_rx = tx.subscribe();
        let mut listener3_rx = tx.subscribe();

        task::spawn(async move {
            distributed_listener(rx, "Node 1".to_string()).await;
        });
        task::spawn(async move { distributed_listener(listener2_rx, "Node 2".to_string()).await });
        task::spawn(async move { distributed_listener(listener3_rx, "Node 3".to_string()).await });

        // Insert a new cache item via HTTP POST
        let value = json!({
            "id": "someboringid",
            "app_id": "soemboringappid",
            "address": "127.0.0.1:8083",
            "weight": 4,
            "key": "somekey",
            "value": "somevalue"
        });

        let req = test::TestRequest::post()
            .uri("/cache/insert/new")
            .set_json(value)
            .to_request();
        let resp = test::call_service(&server, req).await;
        let body = resp.into_body();
        println!("the frigging body {:?}", body);

        // Now, validate that the cache has the item
        let c_req = test::TestRequest::get()
            .uri("/cache/soemboringappid/someboringid/somekey")
            .to_request();
        let response = test::call_service(&server, c_req).await;
        assert_eq!(response.status(), 200);
        let body = response.into_body();
        println!("the frigging body {:?}", body);

        // Invalidate the cache via HTTP DELETE
        let d_req = test::TestRequest::delete()
            .uri("/invalidate/soemboringappid/someboringid/someotherkey")
            .to_request();
        let response = test::call_service(&server, d_req).await;
        assert_eq!(response.status(), 200);
        let body = response.into_body();
        println!("the frigging body {:?}", body);

        // Verify that the cache is now invalidated (cache miss)
        let f_req = test::TestRequest::delete()
            .uri("/cache/soemboringappid/someboringid/someotherkey")
            .to_request();
        let response = test::call_service(&server, f_req).await;
        assert_eq!(response.status(), 404); // Should return "Cache miss"
    }

    async fn distributed_listener(mut rx: broadcast::Receiver<String>, node_id: String) {
        loop {
            let key = rx.recv().await.unwrap();
            println!("[Node {}] Invalidated key: {}", node_id, key)
        }
    }

    async fn benchmark_cache_eviction() {
        let ttl_cache = Cache::builder()
            .max_capacity(100_000)
            .time_to_live(Duration::from_secs(10)) // Items live for 10 sec
            .build();

        let tti_cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_idle(Duration::from_secs(5)) // Items expire if unused for 5 sec
            .build();

        let weighted_cache = Cache::builder()
            .max_capacity(50_000)
            .weigher(|_k: &String, v: &String| v.len() as u32) // Larger values get evicted first
            .build();

        let start_time = Instant::now();

        // Insert 100,000 items into each cache
        for i in 0..100_000 {
            let key = format!("key_{}", i);
            let value = "x".repeat(i % 100); // Larger values for some keys

            ttl_cache.insert(key.clone(), value.clone()).await;
            tti_cache.insert(key.clone(), value.clone()).await;
            weighted_cache.insert(key.clone(), value.clone()).await;

            if i % 10_000 == 0 {
                println!("Inserted {} items...", i);
            }
        }

        println!(
            "Insertion complete! Time elapsed: {:?}",
            start_time.elapsed()
        );

        // Simulate access patterns
        let hit_count = tokio::sync::Mutex::new(0);
        let eviction_count = tokio::sync::Mutex::new(0);

        let start_test = Instant::now();

        for i in 0..100_000 {
            let key = format!("key_{}", i);

            if let Some(_) = ttl_cache.get(&key).await {
                *hit_count.lock().await += 1;
            } else {
                *eviction_count.lock().await += 1;
            }

            if i % 10_000 == 0 {
                sleep(Duration::from_millis(1)).await; // Simulate real-world delay
            }
        }

        println!("Cache access test complete in {:?}", start_test.elapsed());

        let hit_rate = *hit_count.lock().await as f64 / 100_000.0;
        let eviction_rate = *eviction_count.lock().await as f64 / 100_000.0;

        println!(
            "Results:\n - Hit rate: {:.2}%\n - Eviction rate: {:.2}%",
            hit_rate * 100.0,
            eviction_rate * 100.0
        );
    }

    #[tokio::test]
    async fn test_eviction_policies() {
        benchmark_cache_eviction().await;
    }
}
