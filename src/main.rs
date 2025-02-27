use actix_web::{web, App, HttpServer};
use pkg::{cache, cdn, config, handler, state};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task;
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

    let cache_manager = Arc::new(cache::CacheManager::new());
    let (tx, _) = broadcast::channel::<String>(100);

    let cache_manager_clone = cache_manager.clone();
    task::spawn(async move {
        cache_manager_clone.run_listener().await;
    });

    let n1 = tx.subscribe();
    let n2 = tx.subscribe();
    let node1 = Arc::new(cdn::Node::new(
        "node-1".into(),
        "127.0.0.1:8081".into(),
        1,
        n1,
    ));
    let node2 = Arc::new(cdn::Node::new(
        "node-2".into(),
        "127.0.0.1:8082".into(),
        2,
        n2,
    ));

    cache_manager.register_node("app-1", node1).await;
    cache_manager.register_node("app-1", node2).await;

    let app_state = web::Data::new(state::AppState::new(cache_manager.clone(), tx.clone()));

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/cache/{key}", web::get().to(handler::get_cache))
            .route("/cache/{key}", web::post().to(handler::insert_cache))
            .route("/cache/insert", web::post().to(handler::insert_new_cache))
            // basic invalidation
            .route(
                "/invalidate/{key}",
                web::delete().to(handler::invalidate_cache),
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
