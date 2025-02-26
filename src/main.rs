use actix_web::{web, App, HttpServer};
use moka::future::Cache;
use pkg::{cache, config, state};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task;
use tokio::time::sleep;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

mod pkg;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::INFO)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    match config::load_config("config.toml") {
        Ok(data) => info!("Config loaded successfully: {}", data),
        Err(e) => error!("Failed to load config: {:?}", e),
    }

    let cache = Arc::new(
        Cache::builder()
            .max_capacity(10_000) // Store up to 10,000 items
            .time_to_live(Duration::from_secs(60)) // Global TTL of 60s
            // Time to idle (TTI):  5 minutes
            .time_to_idle(Duration::from_secs(5 * 60))
            .build(),
    );
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let state = state::AppState {
        cache: cache.clone(),
        invalidation_tx: tx.clone(),
    };

    // Distributed invalidation listener
    let cache_clone = cache.clone();
    task::spawn(async move {
        while let Some(key) = rx.recv().await {
            cache_clone.invalidate(&key).await;
            println!("[Distributed] Invalidated key: {}", key);
        }
    });

    // Test case simulation
    let cache_clone = cache.clone();
    task::spawn(async move {
        sleep(Duration::from_secs(2)).await;
        cache_clone
            .insert("test".to_string(), "Hello, Moka!".to_string())
            .await;
        println!("[Test] Inserted 'test' key");

        sleep(Duration::from_secs(3)).await;
        let value = cache_clone.get("test").await;
        println!("[Test] Retrieved 'test': {:?}", value);

        sleep(Duration::from_secs(60)).await; // Let TTL expire
        let value = cache_clone.get("test").await;
        println!("[Test] Retrieved after expiry: {:?}", value);
    });

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .app_data(web::Data::new(tx.clone()))
            .route("/cache/{key}", web::get().to(cache::get_cache))
            .route("/cache/{key}", web::post().to(cache::insert_cache))
            .route(
                "/invalidate/{key}",
                web::delete().to(cache::invalidate_cache),
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
