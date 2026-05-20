use async_trait::async_trait;
use reqwest::header::{HeaderValue, RETRY_AFTER};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::types::{DependentEntity, EntityReview, SourceContext};

const DEFAULT_MAX_INPUT_TOKENS: u64 = 120_000;
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_INITIAL_RETRY_DELAY: Duration = Duration::from_secs(1);
const DEFAULT_MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
const APPROX_CHARS_PER_TOKEN: u64 = 4;

#[derive(Debug, Clone)]
pub struct LlmReviewOptions {
    pub max_input_tokens: u64,
    pub max_retries: u32,
    pub initial_retry_delay: Duration,
    pub max_retry_delay: Duration,
}

impl Default for LlmReviewOptions {
    fn default() -> Self {
        Self {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_retry_delay: DEFAULT_INITIAL_RETRY_DELAY,
            max_retry_delay: DEFAULT_MAX_RETRY_DELAY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmReviewStatus {
    Reviewed,
    Skipped,
    Failed,
}

impl Default for LlmReviewStatus {
    fn default() -> Self {
        Self::Reviewed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityLlmReview {
    pub entity_name: String,
    pub file_path: String,
    #[serde(default)]
    pub status: LlmReviewStatus,
    pub verdict: LlmVerdict,
    pub issues: Vec<LlmIssue>,
    pub summary: String,
    pub tokens_used: u64,
    #[serde(default)]
    pub estimated_input_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

impl EntityLlmReview {
    pub fn skipped(
        entity: &EntityReview,
        reason: impl Into<String>,
        estimated_input_tokens: u64,
    ) -> Self {
        Self::unreviewed(
            entity,
            LlmReviewStatus::Skipped,
            "warning",
            "LLM review skipped",
            reason,
            estimated_input_tokens,
        )
    }

    pub fn failed(
        entity: &EntityReview,
        reason: impl Into<String>,
        estimated_input_tokens: u64,
    ) -> Self {
        Self::unreviewed(
            entity,
            LlmReviewStatus::Failed,
            "error",
            "LLM review failed",
            reason,
            estimated_input_tokens,
        )
    }

    pub fn is_reviewed(&self) -> bool {
        self.status == LlmReviewStatus::Reviewed
    }

    fn unreviewed(
        entity: &EntityReview,
        status: LlmReviewStatus,
        severity: &str,
        summary: &str,
        reason: impl Into<String>,
        estimated_input_tokens: u64,
    ) -> Self {
        let reason = reason.into();
        Self {
            entity_name: entity.entity_name.clone(),
            file_path: entity.file_path.clone(),
            status,
            verdict: LlmVerdict::Comment,
            issues: vec![LlmIssue {
                severity: severity.to_string(),
                description: reason.clone(),
            }],
            summary: summary.to_string(),
            tokens_used: 0,
            estimated_input_tokens,
            failure_reason: Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmVerdict {
    Approve,
    Comment,
    RequestChanges,
}

impl std::fmt::Display for LlmVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Approve => write!(f, "approve"),
            Self::Comment => write!(f, "comment"),
            Self::RequestChanges => write!(f, "request_changes"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmIssue {
    pub severity: String,
    pub description: String,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn review_entity(&self, entity: &EntityReview) -> Result<EntityLlmReview, String>;
}

// --- Anthropic structs ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    usage: AnthropicUsage,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicContentBlock {
    text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

// --- OpenAI structs ---

#[derive(Debug, Clone, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAIMessage {
    content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

// --- Shared ---

#[derive(Debug, Clone, Deserialize)]
struct LlmOutput {
    verdict: LlmVerdict,
    #[serde(default)]
    issues: Vec<LlmIssue>,
    #[serde(default)]
    summary: String,
}

fn parse_llm_output(
    text: &str,
    entity: &EntityReview,
    tokens: u64,
) -> Result<EntityLlmReview, String> {
    let json_str = text
        .trim()
        .strip_prefix("```json")
        .or_else(|| text.trim().strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(text)
        .trim();

    if json_str.is_empty() {
        return Err("LLM response was empty".to_string());
    }

    let output: LlmOutput = serde_json::from_str(json_str).map_err(|e| {
        format!(
            "Failed to parse structured LLM response: {}; response: {}",
            e,
            truncate_for_error(text)
        )
    })?;

    Ok(EntityLlmReview {
        entity_name: entity.entity_name.clone(),
        file_path: entity.file_path.clone(),
        status: LlmReviewStatus::Reviewed,
        verdict: output.verdict,
        issues: output.issues,
        summary: output.summary,
        tokens_used: tokens,
        estimated_input_tokens: estimate_entity_input_tokens(entity),
        failure_reason: None,
    })
}

// --- AnthropicClient ---

pub struct AnthropicClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
    options: LlmReviewOptions,
}

impl AnthropicClient {
    pub fn new(model: &str, api_key: Option<&str>) -> Result<Self, String> {
        Self::new_with_options(model, api_key, LlmReviewOptions::default())
    }

    pub fn new_with_options(
        model: &str,
        api_key: Option<&str>,
        options: LlmReviewOptions,
    ) -> Result<Self, String> {
        let api_key = api_key
            .map(|k| k.to_string())
            .or_else(|| {
                std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
            })
            .ok_or_else(|| "ANTHROPIC_API_KEY not set. Set it to use LLM review.".to_string())?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.to_string(),
            options,
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicClient {
    async fn review_entity(&self, entity: &EntityReview) -> Result<EntityLlmReview, String> {
        let prompt = build_prompt(entity);
        let estimated_input_tokens = estimate_request_input_tokens(&prompt);

        if estimated_input_tokens > self.options.max_input_tokens {
            return Ok(EntityLlmReview::skipped(
                entity,
                format!(
                    "estimated prompt size {} tokens exceeds max input budget {}",
                    estimated_input_tokens, self.options.max_input_tokens
                ),
                estimated_input_tokens,
            ));
        }

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            system: SYSTEM_PROMPT.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let resp = send_with_retries(&self.options, || {
            self.client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request)
        })
        .await?;

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse API response: {}", e))?;

        let text = api_resp
            .content
            .first()
            .and_then(|b| b.text.as_deref())
            .unwrap_or("");

        let tokens = api_resp.usage.input_tokens + api_resp.usage.output_tokens;

        parse_llm_output(text, entity, tokens)
    }
}

// --- OpenAIClient ---

pub struct OpenAIClient {
    client: reqwest::Client,
    api_key: Option<String>,
    api_base: String,
    model: String,
    options: LlmReviewOptions,
}

impl OpenAIClient {
    pub fn new(model: &str, api_base: Option<&str>, api_key: Option<&str>) -> Result<Self, String> {
        Self::new_with_options(model, api_base, api_key, LlmReviewOptions::default())
    }

    pub fn new_with_options(
        model: &str,
        api_base: Option<&str>,
        api_key: Option<&str>,
        options: LlmReviewOptions,
    ) -> Result<Self, String> {
        let api_key = api_key.map(|k| k.to_string()).or_else(|| {
            std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        });

        let api_base = api_base
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            api_base,
            model: model.to_string(),
            options,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAIClient {
    async fn review_entity(&self, entity: &EntityReview) -> Result<EntityLlmReview, String> {
        let prompt = build_prompt(entity);
        let estimated_input_tokens = estimate_request_input_tokens(&prompt);

        if estimated_input_tokens > self.options.max_input_tokens {
            return Ok(EntityLlmReview::skipped(
                entity,
                format!(
                    "estimated prompt size {} tokens exceeds max input budget {}",
                    estimated_input_tokens, self.options.max_input_tokens
                ),
                estimated_input_tokens,
            ));
        }

        let request = OpenAIRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: SYSTEM_PROMPT.to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            max_tokens: 1024,
        };

        let url = format!("{}/chat/completions", self.api_base);

        let resp = send_with_retries(&self.options, || {
            let mut req = self
                .client
                .post(&url)
                .header("content-type", "application/json");

            if let Some(ref key) = self.api_key {
                req = req.header("authorization", format!("Bearer {}", key));
            }

            req.json(&request)
        })
        .await?;

        let api_resp: OpenAIResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse API response: {}", e))?;

        let text = api_resp
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("");

        let tokens = api_resp
            .usage
            .map(|u| u.prompt_tokens + u.completion_tokens)
            .unwrap_or(0);

        parse_llm_output(text, entity, tokens)
    }
}

// --- Shared helpers ---

async fn send_with_retries<F>(
    options: &LlmReviewOptions,
    mut build_request: F,
) -> Result<reqwest::Response, String>
where
    F: FnMut() -> reqwest::RequestBuilder,
{
    let mut retries = 0;

    loop {
        let resp = build_request()
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        if resp.status().is_success() {
            return Ok(resp);
        }

        let status = resp.status();
        let retry_after =
            retry_after_delay(resp.headers().get(RETRY_AFTER), options.max_retry_delay);
        let body = resp.text().await.unwrap_or_default();

        if !is_retryable_api_error(status, &body) || retries >= options.max_retries {
            let attempts = retries + 1;
            return Err(format!(
                "API error {} after {} attempt{}: {}",
                status,
                attempts,
                if attempts == 1 { "" } else { "s" },
                body
            ));
        }

        let delay = retry_after.unwrap_or_else(|| backoff_delay(options, retries));
        retries += 1;
        tokio::time::sleep(delay).await;
    }
}

fn is_retryable_api_error(status: StatusCode, body: &str) -> bool {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return true;
    }

    let body = body.to_ascii_lowercase();
    body.contains("rate limit") || body.contains("too many requests")
}

fn retry_after_delay(value: Option<&HeaderValue>, max_delay: Duration) -> Option<Duration> {
    let seconds = value?.to_str().ok()?.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds).min(max_delay))
}

fn backoff_delay(options: &LlmReviewOptions, retry_number: u32) -> Duration {
    let factor = 1_u128 << retry_number.min(10);
    let millis = options
        .initial_retry_delay
        .as_millis()
        .saturating_mul(factor)
        .min(options.max_retry_delay.as_millis());

    Duration::from_millis(millis as u64)
}

pub fn estimate_entity_input_tokens(entity: &EntityReview) -> u64 {
    let prompt = build_prompt(entity);
    estimate_request_input_tokens(&prompt)
}

fn estimate_request_input_tokens(prompt: &str) -> u64 {
    estimate_tokens(SYSTEM_PROMPT) + estimate_tokens(prompt)
}

pub fn estimate_tokens(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    if chars == 0 {
        0
    } else {
        (chars + APPROX_CHARS_PER_TOKEN - 1) / APPROX_CHARS_PER_TOKEN
    }
}

fn truncate_for_error(text: &str) -> String {
    const MAX_CHARS: usize = 500;
    let mut output: String = text.chars().take(MAX_CHARS).collect();
    if text.chars().count() > MAX_CHARS {
        output.push_str("...");
    }
    output
}

const SYSTEM_PROMPT: &str = "\
You are a code reviewer. Review the entity for bugs, security issues, and correctness problems. \
Respond with JSON only, no explanation outside the JSON. Format:
{\"verdict\": \"approve\" | \"comment\" | \"request_changes\", \"issues\": [{\"severity\": \"error\" | \"warning\" | \"info\", \"description\": \"...\"}], \"summary\": \"one sentence\"}";

fn build_prompt(entity: &EntityReview) -> String {
    let mut parts = vec![
        format!("Entity: {} ({})", entity.entity_name, entity.entity_type),
        format!("File: {}", entity.file_path),
        format!("Change: {}", entity.change_type),
        format!("Classification: {}", entity.classification),
        format!(
            "Risk: {} (score {:.2})",
            entity.risk_level, entity.risk_score
        ),
        format!(
            "Blast radius: {}, Dependents: {}",
            entity.blast_radius, entity.dependent_count
        ),
        format!(
            "Context semantics:\n\
             - BEFORE and AFTER snippets are complete {} entity bodies, not full-file context.\n\
             - File context windows are line-numbered excerpts around this entity.\n\
             - Treat added entities/blocks as additive; omitted surrounding code was not removed unless Change is deleted.\n\
             - Dependency/helper source shows direct callees referenced by this entity; dependent/caller source shows consumers of this entity.",
            entity.entity_type
        ),
    ];

    if entity.is_public_api {
        parts.push("Public API: yes".to_string());
    }

    if !entity.dependent_names.is_empty() {
        let deps: Vec<String> = entity
            .dependent_names
            .iter()
            .take(10)
            .map(|(name, file)| format!("  {} ({})", name, file))
            .collect();
        parts.push(format!("Dependents:\n{}", deps.join("\n")));
    }

    if !entity.dependency_names.is_empty() {
        let deps: Vec<String> = entity
            .dependency_names
            .iter()
            .take(10)
            .map(|(name, file)| format!("  {} ({})", name, file))
            .collect();
        parts.push(format!("Dependencies / helpers:\n{}", deps.join("\n")));
    }

    if let Some(ref context) = entity.before_file_context {
        parts.push(format_source_context("BEFORE FILE CONTEXT", context));
    }

    if let Some(ref before) = entity.before_content {
        parts.push(format!("BEFORE:\n```\n{}\n```", before));
    }

    if let Some(ref context) = entity.after_file_context {
        parts.push(format_source_context("AFTER FILE CONTEXT", context));
    }

    if let Some(ref after) = entity.after_content {
        parts.push(format!("AFTER:\n```\n{}\n```", after));
    }

    if !entity.dependency_entities.is_empty() {
        parts.push(format_related_sources(
            "DEPENDENCY / HELPER SOURCE",
            &entity.dependency_entities,
        ));
    }

    if !entity.dependent_entities.is_empty() {
        parts.push(format_related_sources(
            "DEPENDENT / CALLER SOURCE",
            &entity.dependent_entities,
        ));
    }

    parts.join("\n\n")
}

fn format_source_context(label: &str, context: &SourceContext) -> String {
    format!(
        "{} ({}:{}-{}):\n```\n{}\n```",
        label, context.file_path, context.start_line, context.end_line, context.content
    )
}

fn format_related_sources(label: &str, entities: &[DependentEntity]) -> String {
    entities
        .iter()
        .map(|entity| {
            let relation = entity.relation.as_deref().unwrap_or("current");
            format!(
                "{} [{}]: {} {} ({}:{}-{}, public_api={}, dependents={})\n```\n{}\n```",
                label,
                relation,
                entity.entity_type,
                entity.entity_name,
                entity.file_path,
                entity.start_line,
                entity.end_line,
                entity.is_public_api,
                entity.own_dependent_count,
                entity.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChangeClassification, RiskLevel};
    use sem_core::model::change::ChangeType;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn make_review(after_content: &str) -> EntityReview {
        EntityReview {
            entity_id: "src/lib.rs::function::handle".to_string(),
            entity_name: "handle".to_string(),
            entity_type: "function".to_string(),
            file_path: "src/lib.rs".to_string(),
            change_type: ChangeType::Modified,
            classification: ChangeClassification::Functional,
            risk_score: 0.8,
            risk_level: RiskLevel::High,
            blast_radius: 0,
            dependent_count: 0,
            dependency_count: 0,
            is_public_api: false,
            structural_change: Some(true),
            group_id: 0,
            start_line: 1,
            end_line: 3,
            before_content: None,
            after_content: Some(after_content.to_string()),
            dependent_names: vec![],
            dependency_names: vec![],
            dependent_entities: vec![],
            dependency_entities: vec![],
            before_file_context: None,
            after_file_context: None,
        }
    }

    fn related_entity(name: &str, content: &str) -> DependentEntity {
        DependentEntity {
            entity_name: name.to_string(),
            entity_type: "function".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            end_line: 3,
            content: content.to_string(),
            own_dependent_count: 1,
            is_public_api: false,
            relation: Some("after".to_string()),
        }
    }

    #[test]
    fn estimates_tokens_by_rounding_up_character_count() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abc"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn retry_after_delay_uses_delta_seconds() {
        let value = HeaderValue::from_static("7");
        assert_eq!(
            retry_after_delay(Some(&value), Duration::from_secs(30)),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn retry_after_delay_is_capped_by_options() {
        let value = HeaderValue::from_static("3600");
        assert_eq!(
            retry_after_delay(Some(&value), Duration::from_secs(30)),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn retryable_errors_include_rate_limit_signals() {
        assert!(is_retryable_api_error(StatusCode::TOO_MANY_REQUESTS, ""));
        assert!(is_retryable_api_error(
            StatusCode::BAD_REQUEST,
            "provider rate limit exceeded"
        ));
        assert!(!is_retryable_api_error(StatusCode::BAD_REQUEST, "bad json"));
    }

    #[tokio::test]
    async fn skips_entity_when_estimate_exceeds_budget() {
        let entity = make_review("pub fn handle() { println!(\"too large\"); }");
        let client = OpenAIClient::new_with_options(
            "test-model",
            Some("http://127.0.0.1:1/v1"),
            None,
            LlmReviewOptions {
                max_input_tokens: 1,
                max_retries: 0,
                initial_retry_delay: Duration::from_millis(1),
                max_retry_delay: Duration::from_millis(1),
            },
        )
        .unwrap();

        let review = client.review_entity(&entity).await.unwrap();

        assert_eq!(review.status, LlmReviewStatus::Skipped);
        assert_eq!(review.tokens_used, 0);
        assert!(review
            .failure_reason
            .as_deref()
            .unwrap()
            .contains("estimated prompt size"));
    }

    #[test]
    fn failed_records_preserve_entity_identity_and_reason() {
        let entity = make_review("pub fn handle() {}");
        let review = EntityLlmReview::failed(&entity, "API error 429", 42);

        assert_eq!(review.status, LlmReviewStatus::Failed);
        assert_eq!(review.entity_name, "handle");
        assert_eq!(review.file_path, "src/lib.rs");
        assert_eq!(review.estimated_input_tokens, 42);
        assert_eq!(review.failure_reason.as_deref(), Some("API error 429"));
    }

    #[test]
    fn malformed_llm_response_is_an_error() {
        let entity = make_review("pub fn handle() {}");
        let err = parse_llm_output("not json", &entity, 12).unwrap_err();

        assert!(err.contains("Failed to parse structured LLM response"));
    }

    #[tokio::test]
    async fn openai_client_retries_rate_limits_then_returns_review() {
        let (base_url, request_count) = spawn_openai_test_server(vec![
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 10\r\nContent-Length: 19\r\n\r\nrate limit exceeded"
                .to_string(),
            openai_success_response(),
        ])
        .await;
        let entity = make_review("pub fn handle() {}");
        let client = OpenAIClient::new_with_options(
            "test-model",
            Some(&base_url),
            None,
            LlmReviewOptions {
                max_input_tokens: 10_000,
                max_retries: 1,
                initial_retry_delay: Duration::from_millis(1),
                max_retry_delay: Duration::from_millis(1),
            },
        )
        .unwrap();

        let review = client.review_entity(&entity).await.unwrap();

        assert_eq!(request_count.load(Ordering::SeqCst), 2);
        assert_eq!(review.status, LlmReviewStatus::Reviewed);
        assert_eq!(review.verdict, LlmVerdict::Approve);
    }

    #[tokio::test]
    async fn openai_client_returns_error_after_retry_budget() {
        let (base_url, request_count) = spawn_openai_test_server(vec![
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 19\r\n\r\nrate limit exceeded"
                .to_string(),
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 19\r\n\r\nrate limit exceeded"
                .to_string(),
        ])
        .await;
        let entity = make_review("pub fn handle() {}");
        let client = OpenAIClient::new_with_options(
            "test-model",
            Some(&base_url),
            None,
            LlmReviewOptions {
                max_input_tokens: 10_000,
                max_retries: 1,
                initial_retry_delay: Duration::from_millis(1),
                max_retry_delay: Duration::from_millis(1),
            },
        )
        .unwrap();

        let err = client.review_entity(&entity).await.unwrap_err();

        assert_eq!(request_count.load(Ordering::SeqCst), 2);
        assert!(err.contains("after 2 attempts"));
    }

    async fn spawn_openai_test_server(responses: Vec<String>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let request_count = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&request_count);

        tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);

                let mut buf = vec![0_u8; 4096];
                let _ = stream.read(&mut buf).await.unwrap();
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });

        (format!("http://{}", addr), request_count)
    }

    fn openai_success_response() -> String {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"verdict\":\"approve\",\"issues\":[],\"summary\":\"ok\"}"
                }
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 2
            }
        })
        .to_string();

        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
    }

    #[test]
    fn build_prompt_includes_review_context_sections() {
        let entity = EntityReview {
            entity_id: "src/lib.rs::function::validate".to_string(),
            entity_name: "validate".to_string(),
            entity_type: "function".to_string(),
            file_path: "src/lib.rs".to_string(),
            change_type: ChangeType::Added,
            classification: ChangeClassification::Functional,
            risk_score: 0.7,
            risk_level: RiskLevel::High,
            blast_radius: 2,
            dependent_count: 1,
            dependency_count: 1,
            is_public_api: true,
            structural_change: Some(true),
            group_id: 1,
            start_line: 10,
            end_line: 14,
            before_content: None,
            after_content: Some("pub fn validate(ok: bool) { reject(); }".to_string()),
            dependent_names: vec![("caller".to_string(), "src/lib.rs".to_string())],
            dependency_names: vec![("reject".to_string(), "src/lib.rs".to_string())],
            dependent_entities: vec![related_entity("caller", "fn caller() { validate(true); }")],
            dependency_entities: vec![related_entity("reject", "fn reject() -> ! { panic!(); }")],
            before_file_context: None,
            after_file_context: Some(SourceContext {
                file_path: "src/lib.rs".to_string(),
                start_line: 8,
                end_line: 16,
                content: "   8: fn other() {}\n  10: pub fn validate(ok: bool) {".to_string(),
            }),
        };

        let prompt = build_prompt(&entity);

        assert!(prompt.contains("Context semantics:"));
        assert!(prompt.contains("Treat added entities/blocks as additive"));
        assert!(prompt.contains("Dependencies / helpers:"));
        assert!(prompt.contains("AFTER FILE CONTEXT"));
        assert!(prompt.contains("DEPENDENCY / HELPER SOURCE [after]: function reject"));
        assert!(prompt.contains("DEPENDENT / CALLER SOURCE [after]: function caller"));
    }
}
