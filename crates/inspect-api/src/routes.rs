use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use tracing::{error, info};
use uuid::Uuid;

use inspect_core::analyze::analyze_remote;
use inspect_core::github::GitHubClient;
use inspect_core::noise::is_noise_file;
use inspect_core::risk::suggest_verdict;

use crate::openai;
use crate::prompts;
use crate::state::*;

#[derive(Deserialize)]
pub struct ReviewRequest {
    pub repo: String,
    pub pr_number: u64,
}

#[derive(Deserialize)]
pub struct TriageRequest {
    pub repo: String,
    pub pr_number: u64,
    pub min_risk: Option<String>,
}

// POST /v1/review
pub async fn create_review(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReviewRequest>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();

    let job = ReviewJob {
        id: id.clone(),
        status: JobStatus::Pending,
        repo: req.repo.clone(),
        pr_number: req.pr_number,
        result: None,
        error: None,
        created_at: chrono::Utc::now(),
    };

    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(id.clone(), job.clone());
    }

    // Spawn background review
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        run_review(state_clone, id_clone).await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "id": id,
            "status": "pending",
        })),
    )
}

// GET /v1/review/:id
pub async fn get_review(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let jobs = state.jobs.read().await;
    match jobs.get(&id) {
        Some(job) => (StatusCode::OK, Json(serde_json::to_value(job).unwrap())),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "job not found"})),
        ),
    }
}

// POST /v1/triage
pub async fn create_triage(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<TriageRequest>,
) -> impl IntoResponse {
    let start = Instant::now();

    let client = match GitHubClient::new() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("GitHub auth failed: {e}")})),
            );
        }
    };

    let pr = match client.get_pr(&req.repo, req.pr_number).await {
        Ok(pr) => pr,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Failed to fetch PR: {e}")})),
            );
        }
    };

    let visible_files: Vec<_> = pr
        .files
        .iter()
        .filter(|f| !is_noise_file(&f.filename))
        .cloned()
        .collect();

    let file_pairs = client
        .get_file_pairs(&req.repo, &visible_files, &pr.base_ref, &pr.head_ref)
        .await;

    let result = match analyze_remote(&file_pairs) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Analysis failed: {e}")})),
            );
        }
    };

    let verdict = suggest_verdict(&result);
    let entities = build_entity_json(&result, req.min_risk.as_deref());
    let elapsed = start.elapsed().as_millis() as u64;

    let resp = serde_json::json!({
        "verdict": format!("{}", verdict),
        "total_entities": result.stats.total_entities,
        "entities": entities,
        "stats": {
            "critical": result.stats.by_risk.critical,
            "high": result.stats.by_risk.high,
            "medium": result.stats.by_risk.medium,
            "low": result.stats.by_risk.low,
        },
        "timing_ms": elapsed,
    });

    (StatusCode::OK, Json(resp))
}

// GET /health
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

fn parse_risk_level(s: &str) -> inspect_core::types::RiskLevel {
    match s.to_lowercase().as_str() {
        "critical" => inspect_core::types::RiskLevel::Critical,
        "high" => inspect_core::types::RiskLevel::High,
        "medium" => inspect_core::types::RiskLevel::Medium,
        _ => inspect_core::types::RiskLevel::Low,
    }
}

fn build_entity_json(
    result: &inspect_core::types::ReviewResult,
    min_risk: Option<&str>,
) -> Vec<serde_json::Value> {
    result
        .entity_reviews
        .iter()
        .filter(|r| {
            if let Some(min) = min_risk {
                r.risk_level >= parse_risk_level(min)
            } else {
                true
            }
        })
        .map(|r| {
            serde_json::json!({
                "name": r.entity_name,
                "type": r.entity_type,
                "file": r.file_path,
                "risk": format!("{}", r.risk_level),
                "score": format!("{:.2}", r.risk_score),
                "classification": format!("{}", r.classification),
                "change_type": format!("{:?}", r.change_type).to_lowercase(),
                "public_api": r.is_public_api,
                "cosmetic": r.structural_change == Some(false),
                "group_id": r.group_id,
            })
        })
        .collect()
}

/// Background job: run full review pipeline.
async fn run_review(state: Arc<AppState>, job_id: String) {
    let total_start = Instant::now();

    // Update status to analyzing
    update_status(&state, &job_id, JobStatus::Analyzing).await;

    // Step 1: Fetch PR and run triage
    let triage_start = Instant::now();
    let client = match GitHubClient::new() {
        Ok(c) => c,
        Err(e) => {
            fail_job(&state, &job_id, format!("GitHub auth failed: {e}")).await;
            return;
        }
    };

    let (repo, pr_number) = {
        let jobs = state.jobs.read().await;
        let job = jobs.get(&job_id).unwrap();
        (job.repo.clone(), job.pr_number)
    };

    let pr = match client.get_pr(&repo, pr_number).await {
        Ok(pr) => pr,
        Err(e) => {
            fail_job(&state, &job_id, format!("Failed to fetch PR: {e}")).await;
            return;
        }
    };

    info!(
        "PR #{}: {} ({} files, +{}/-{})",
        pr.number, pr.title, pr.changed_files, pr.additions, pr.deletions
    );

    let visible_files: Vec<_> = pr
        .files
        .iter()
        .filter(|f| !is_noise_file(&f.filename))
        .cloned()
        .collect();

    let file_pairs = client
        .get_file_pairs(&repo, &visible_files, &pr.base_ref, &pr.head_ref)
        .await;

    let result = match analyze_remote(&file_pairs) {
        Ok(r) => r,
        Err(e) => {
            fail_job(&state, &job_id, format!("Analysis failed: {e}")).await;
            return;
        }
    };

    let verdict = suggest_verdict(&result);
    let triage_ms = triage_start.elapsed().as_millis() as u64;
    info!("Triage complete in {}ms: {} entities", triage_ms, result.stats.total_entities);

    // Step 2: Fetch raw diff for LLM review
    update_status(&state, &job_id, JobStatus::Reviewing).await;

    let review_start = Instant::now();
    let diff = match fetch_pr_diff(&state, &repo, pr_number).await {
        Ok(d) => d,
        Err(e) => {
            fail_job(&state, &job_id, format!("Failed to fetch diff: {e}")).await;
            return;
        }
    };

    // Build triage context
    let triage_section = prompts::build_rich_triage(&result.entity_reviews);

    // Step 3: LLM review with deep_v2 strategy
    let findings = openai::review_deep_v2(&state, &pr.title, &diff, &triage_section, 15).await;
    let review_ms = review_start.elapsed().as_millis() as u64;
    info!("Review complete in {}ms: {} findings", review_ms, findings.len());

    // Build response
    let entities = build_entity_json(&result, None);
    let total_ms = total_start.elapsed().as_millis() as u64;

    let response = ReviewResponse {
        findings,
        triage: TriageResponse {
            verdict: format!("{}", verdict),
            total_entities: result.stats.total_entities,
            entities,
            stats: serde_json::json!({
                "critical": result.stats.by_risk.critical,
                "high": result.stats.by_risk.high,
                "medium": result.stats.by_risk.medium,
                "low": result.stats.by_risk.low,
            }),
        },
        timing: TimingInfo {
            triage_ms,
            review_ms,
            total_ms,
        },
    };

    // Store result
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = JobStatus::Complete;
            job.result = Some(response);
        }
    }

    info!("Job {} complete in {}ms", job_id, total_ms);
}

async fn fetch_pr_diff(state: &AppState, repo: &str, pr_number: u64) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{repo}/pulls/{pr_number}");
    let resp = state
        .http
        .get(&url)
        .header("Authorization", format!("token {}", state.github_token))
        .header("Accept", "application/vnd.github.v3.diff")
        .header("User-Agent", "inspect-api")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub API error {status}: {text}"));
    }

    resp.text().await.map_err(|e| format!("read failed: {e}"))
}

async fn update_status(state: &AppState, job_id: &str, status: JobStatus) {
    let mut jobs = state.jobs.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        job.status = status;
    }
}

async fn fail_job(state: &AppState, job_id: &str, error: String) {
    error!("Job {} failed: {}", job_id, error);
    let mut jobs = state.jobs.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        job.status = JobStatus::Failed;
        job.error = Some(error);
    }
}
