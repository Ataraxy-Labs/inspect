mod openai;
mod prompts;
mod routes;
mod state;

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "inspect_api=info,tower_http=info".into()),
        )
        .init();

    let openai_api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required");
    let github_token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN required");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let openai_model =
        std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());

    let state = Arc::new(AppState {
        port,
        openai_api_key,
        openai_model: openai_model.clone(),
        github_token,
        http: reqwest::Client::new(),
        jobs: Arc::new(RwLock::new(HashMap::new())),
    });

    let app = Router::new()
        .route("/v1/review", axum::routing::post(routes::create_review))
        .route("/v1/review/{id}", axum::routing::get(routes::get_review))
        .route("/v1/triage", axum::routing::post(routes::create_triage))
        .route("/health", axum::routing::get(routes::health))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    info!("inspect-api listening on {addr} (model: {openai_model})");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
