use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::openai::Finding;

#[derive(Clone)]
pub struct AppState {
    pub port: u16,
    pub openai_api_key: String,
    pub openai_model: String,
    pub github_token: String,
    pub http: reqwest::Client,
    pub jobs: Arc<RwLock<HashMap<String, ReviewJob>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Analyzing,
    Reviewing,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewJob {
    pub id: String,
    pub status: JobStatus,
    pub repo: String,
    pub pr_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ReviewResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResponse {
    pub findings: Vec<Finding>,
    pub triage: TriageResponse,
    pub timing: TimingInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResponse {
    pub verdict: String,
    pub total_entities: usize,
    pub entities: Vec<serde_json::Value>,
    pub stats: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingInfo {
    pub triage_ms: u64,
    pub review_ms: u64,
    pub total_ms: u64,
}
