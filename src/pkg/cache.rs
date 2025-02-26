use super::state::AppState;
use actix_web::{web, HttpResponse, Responder};

// Retrieve cached item
pub async fn get_cache(state: web::Data<AppState>, key: web::Path<String>) -> impl Responder {
    let key = key.into_inner(); // Extract the key as a String

    if let Some(data) = state.cache.get(&key).await {
        HttpResponse::Ok().body(data)
    } else {
        HttpResponse::NotFound().body("Cache miss")
    }
}

// Insert into cache with automatic TTL expiration
pub async fn insert_cache(
    state: web::Data<AppState>,
    key: web::Path<String>,
    body: String,
) -> impl Responder {
    let key = key.into_inner(); // Extract the key as a String

    // Insert the cache with a TTL of 30 seconds
    state.cache.insert(key.clone(), body.clone()).await;
    HttpResponse::Ok().body(format!("Inserted '{}' into cache", body))
}

// Invalidate cache (via webhook)
pub async fn invalidate_cache(
    state: web::Data<AppState>,
    key: web::Path<String>,
) -> impl Responder {
    let key = key.into_inner(); // Extract the key as a String
    state.cache.invalidate(&key).await;

    // Notify other nodes (simulate distributed invalidation)
    let _ = state.invalidation_tx.send(key).await;

    HttpResponse::Ok().body("Cache invalidated")
}
