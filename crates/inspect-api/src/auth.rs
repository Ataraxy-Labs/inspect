use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use sha2::{Digest, Sha256};
use tracing::warn;

fn sha256_hex(input: &str) -> String {
    let hash = Sha256::digest(input.as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

use crate::state::AppState;

pub struct ApiKey {
    pub key_id: String,
}

#[derive(Debug)]
pub enum AuthError {
    MissingHeader,
    InvalidKey,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AuthError::MissingHeader => (
                StatusCode::UNAUTHORIZED,
                "Missing Authorization: Bearer <api_key> header",
            ),
            AuthError::InvalidKey => (StatusCode::UNAUTHORIZED, "Invalid API key"),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

impl FromRequestParts<Arc<AppState>> for ApiKey {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::MissingHeader)?;

        if !header.starts_with("Bearer ") {
            return Err(AuthError::MissingHeader);
        }

        let raw_key = &header[7..];
        let hash = sha256_hex(raw_key);

        // Query Supabase REST API for matching key
        let url = format!(
            "{}/rest/v1/api_keys?key_hash=eq.{}&revoked_at=is.null&select=id,request_count",
            state.supabase_url, hash
        );

        let resp = state
            .http
            .get(&url)
            .header("apikey", &state.supabase_key)
            .header("Authorization", format!("Bearer {}", state.supabase_key))
            .send()
            .await
            .map_err(|e| {
                warn!("Supabase request failed: {e}");
                AuthError::InvalidKey
            })?;

        if !resp.status().is_success() {
            warn!("Supabase returned {}", resp.status());
            return Err(AuthError::InvalidKey);
        }

        let rows: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
            warn!("Failed to parse Supabase response: {e}");
            AuthError::InvalidKey
        })?;

        let row = rows.first().ok_or(AuthError::InvalidKey)?;
        let key_id = row["id"]
            .as_str()
            .ok_or(AuthError::InvalidKey)?
            .to_string();
        let request_count = row["request_count"].as_i64().unwrap_or(0);

        // Fire-and-forget: increment usage
        let update_url = format!(
            "{}/rest/v1/api_keys?id=eq.{}",
            state.supabase_url, key_id
        );
        let client = state.http.clone();
        let supa_key = state.supabase_key.clone();
        tokio::spawn(async move {
            let _ = client
                .patch(&update_url)
                .header("apikey", &supa_key)
                .header("Authorization", format!("Bearer {}", supa_key))
                .header("Content-Type", "application/json")
                .json(&serde_json::json!({
                    "last_used_at": chrono::Utc::now().to_rfc3339(),
                    "request_count": request_count + 1,
                }))
                .send()
                .await;
        });

        Ok(ApiKey { key_id })
    }
}
