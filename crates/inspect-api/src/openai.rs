use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use inspect_core::detect::DetectorFinding;
use inspect_core::types::{EntityReview, RiskLevel};

use crate::prompts;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub issue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

#[derive(Deserialize)]
struct IssuesResponse {
    #[serde(default)]
    issues: Vec<serde_json::Value>,
}

/// Call OpenAI API. Uses the Responses API for GPT-5+ models (with reasoning_effort),
/// falls back to Chat Completions for older models.
///
/// `reasoning_effort`: "low", "medium", or "high". Only used for GPT-5+ (Responses API).
/// For Chat Completions models, `temperature` is used instead.
async fn call_openai(
    state: &AppState,
    system: &str,
    prompt: &str,
    temperature: f64,
    reasoning_effort: &str,
) -> Result<String, String> {
    let model = &state.openai_model;
    let is_gpt5 = model.starts_with("gpt-5");

    if is_gpt5 {
        call_openai_responses(state, system, prompt, reasoning_effort).await
    } else {
        call_openai_chat(state, system, prompt, temperature).await
    }
}

/// Call the Responses API for GPT-5+ models with reasoning_effort support.
async fn call_openai_responses(
    state: &AppState,
    system: &str,
    prompt: &str,
    reasoning_effort: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "model": state.openai_model,
        "instructions": system,
        "input": prompt,
        "reasoning": {
            "effort": reasoning_effort
        },
    });

    let resp = state
        .http
        .post("https://api.openai.com/v1/responses")
        .header("Authorization", format!("Bearer {}", state.openai_api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error {status}: {text}"));
    }

    // Responses API returns { output: [ { type: "message", content: [ { type: "output_text", text: "..." } ] } ] }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse failed: {e}"))?;
    let text = body["output"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find_map(|item| {
                if item["type"].as_str() == Some("message") {
                    item["content"]
                        .as_array()
                        .and_then(|c| {
                            c.iter().find_map(|part| {
                                if part["type"].as_str() == Some("output_text") {
                                    part["text"].as_str().map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                        })
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();

    Ok(text)
}

/// Call the Chat Completions API for non-GPT-5 models.
async fn call_openai_chat(
    state: &AppState,
    system: &str,
    prompt: &str,
    temperature: f64,
) -> Result<String, String> {
    let body = serde_json::json!({
        "model": state.openai_model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": prompt},
        ],
        "temperature": temperature,
    });

    let resp = state
        .http
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", state.openai_api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error {status}: {text}"));
    }

    let chat: ChatResponse = resp.json().await.map_err(|e| format!("parse failed: {e}"))?;
    let content = chat
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();

    Ok(content)
}

/// Strip markdown code fences and parse JSON issues.
fn parse_issues(text: &str) -> Vec<Finding> {
    let cleaned = strip_code_fences(text);

    let parsed: Result<IssuesResponse, _> = serde_json::from_str(&cleaned);
    match parsed {
        Ok(resp) => resp
            .issues
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(Finding {
                    issue: s,
                    evidence: None,
                    severity: None,
                    file: None,
                }),
                serde_json::Value::Object(map) => {
                    let issue = map
                        .get("issue")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if issue.is_empty() {
                        return None;
                    }
                    Some(Finding {
                        issue,
                        evidence: map.get("evidence").and_then(|v| v.as_str()).map(String::from),
                        severity: map.get("severity").and_then(|v| v.as_str()).map(String::from),
                        file: map.get("file").and_then(|v| v.as_str()).map(String::from),
                    })
                }
                _ => None,
            })
            .collect(),
        Err(e) => {
            warn!("Failed to parse LLM response as JSON: {e}");
            Vec::new()
        }
    }
}

fn strip_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        let after_fence = &trimmed[3..];
        // Skip optional language tag
        let content = if after_fence.starts_with("json") {
            &after_fence[4..]
        } else {
            after_fence
        };
        // Find closing fence
        if let Some(end) = content.rfind("```") {
            return content[..end].trim().to_string();
        }
        return content.trim().to_string();
    }
    trimmed.to_string()
}

/// Two-temperature merge + validation (deep_v2 strategy).
pub async fn review_deep_v2(
    state: &AppState,
    pr_title: &str,
    diff: &str,
    triage_section: &str,
    max_findings: usize,
) -> Vec<Finding> {
    let truncated = prompts::truncate_diff(diff, 80_000);
    let prompt = prompts::format_deep_prompt(pr_title, triage_section, &truncated);

    // Two passes in parallel: T=0 (deterministic) + T=0.3 (diverse)
    // deep_v2 is a discovery task → medium reasoning effort
    let (pass_0, pass_1) = tokio::join!(
        call_openai(state, prompts::SYSTEM_REVIEW, &prompt, 0.0, "medium"),
        call_openai(state, prompts::SYSTEM_REVIEW, &prompt, 0.3, "medium"),
    );

    let mut all_findings: Vec<Finding> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Merge T=0 results
    if let Ok(text) = pass_0 {
        for f in parse_issues(&text) {
            let key = f.issue.to_lowercase().chars().take(80).collect::<String>();
            if seen.insert(key) {
                all_findings.push(f);
            }
        }
    } else {
        warn!("T=0 pass failed: {:?}", pass_0.err());
    }

    // Add unique from T=0.3
    if let Ok(text) = pass_1 {
        for f in parse_issues(&text) {
            let key = f.issue.to_lowercase().chars().take(80).collect::<String>();
            if seen.insert(key) {
                all_findings.push(f);
            }
        }
    } else {
        warn!("T=0.3 pass failed: {:?}", pass_1.err());
    }

    if all_findings.is_empty() {
        return Vec::new();
    }

    // Skip validation if few findings
    if all_findings.len() <= 2 {
        return all_findings;
    }

    // Validation pass
    match validate_findings(state, pr_title, &truncated, &all_findings).await {
        Ok(validated) => validated.into_iter().take(max_findings).collect(),
        Err(e) => {
            warn!("Validation failed: {e}");
            all_findings.into_iter().take(max_findings).collect()
        }
    }
}

/// Diff-aware self-refine validation.
async fn validate_findings(
    state: &AppState,
    pr_title: &str,
    diff: &str,
    candidates: &[Finding],
) -> Result<Vec<Finding>, String> {
    let candidates_text: String = candidates
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let mut line = format!("{}. {}", i + 1, f.issue);
            if let Some(ref ev) = f.evidence {
                line.push_str(&format!("\n   Evidence: {ev}"));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = prompts::format_validate_prompt(pr_title, diff, &candidates_text);
    let text = call_openai(state, prompts::SYSTEM_VALIDATE, &prompt, 0.0, "low").await?;
    Ok(parse_issues(&text))
}

#[derive(Deserialize)]
struct VerdictsResponse {
    #[serde(default)]
    verdicts: Vec<Verdict>,
}

#[derive(Deserialize)]
struct Verdict {
    rule_id: String,
    entity_name: String,
    verdict: String,
    #[serde(default)]
    explanation: String,
}

/// Parse the `{"verdicts": [...]}` JSON schema from LLM validation response.
/// Converts confirmed `true_positive` verdicts back into `Finding` structs.
fn parse_verdicts(text: &str, findings: &[DetectorFinding]) -> Vec<Finding> {
    let cleaned = strip_code_fences(text);

    let parsed: Result<VerdictsResponse, _> = serde_json::from_str(&cleaned);
    match parsed {
        Ok(resp) => resp
            .verdicts
            .into_iter()
            .filter(|v| v.verdict == "true_positive")
            .map(|v| {
                let original = findings
                    .iter()
                    .find(|f| f.rule_id == v.rule_id && f.entity_name == v.entity_name);
                match original {
                    Some(f) => Finding {
                        issue: format!("[{}] {}", f.rule_id, f.message),
                        evidence: Some(f.evidence.clone()),
                        severity: Some(format!("{}", f.severity)),
                        file: Some(f.file_path.clone()),
                    },
                    None => Finding {
                        issue: format!("[{}] {} ({})", v.rule_id, v.explanation, v.entity_name),
                        evidence: None,
                        severity: None,
                        file: None,
                    },
                }
            })
            .collect(),
        Err(e) => {
            warn!("Failed to parse verdicts JSON: {e}");
            Vec::new()
        }
    }
}

/// Deterministic-first review pipeline.
///
/// Logic:
/// 1. If detector findings exist → LLM validates those specific findings only
/// 2. If no findings but high-risk entities exist → constrained LLM fallback (max 3 issues)
/// 3. If no findings and no high-risk entities → skip LLM entirely
pub async fn review_deterministic(
    state: &AppState,
    pr_title: &str,
    diff: &str,
    triage_section: &str,
    detector_findings: &[DetectorFinding],
    entity_reviews: &[EntityReview],
    max_findings: usize,
) -> Vec<Finding> {
    let truncated = prompts::truncate_diff(diff, 80_000);

    if !detector_findings.is_empty() {
        // Path 1: Validate deterministic findings with LLM (medium — needs to reason about code semantics)
        let prompt = prompts::format_validate_findings_prompt(
            pr_title,
            triage_section,
            &truncated,
            detector_findings,
            entity_reviews,
        );
        match call_openai(state, prompts::SYSTEM_VALIDATE_FINDINGS, &prompt, 0.0, "medium").await {
            Ok(text) => {
                let validated = parse_verdicts(&text, detector_findings);
                if validated.is_empty() {
                    // LLM rejected all — return high-confidence findings directly
                    detector_findings
                        .iter()
                        .filter(|f| f.confidence >= 0.7)
                        .take(max_findings)
                        .map(|f| Finding {
                            issue: format!("[{}] {}", f.rule_id, f.message),
                            evidence: Some(f.evidence.clone()),
                            severity: Some(format!("{}", f.severity)),
                            file: Some(f.file_path.clone()),
                        })
                        .collect()
                } else {
                    validated.into_iter().take(max_findings).collect()
                }
            }
            Err(e) => {
                warn!("LLM validation failed: {e}");
                detector_findings
                    .iter()
                    .filter(|f| f.confidence >= 0.7)
                    .take(max_findings)
                    .map(|f| Finding {
                        issue: format!("[{}] {}", f.rule_id, f.message),
                        evidence: Some(f.evidence.clone()),
                        severity: Some(format!("{}", f.severity)),
                        file: Some(f.file_path.clone()),
                    })
                    .collect()
            }
        }
    } else {
        let has_high_risk = entity_reviews
            .iter()
            .any(|r| matches!(r.risk_level, RiskLevel::High | RiskLevel::Critical));

        if has_high_risk {
            // Path 2: Constrained LLM fallback for high-risk changes (medium — discovering bugs)
            let prompt =
                prompts::format_deep_fallback_prompt(pr_title, triage_section, &truncated);
            match call_openai(state, prompts::SYSTEM_REVIEW, &prompt, 0.0, "medium").await {
                Ok(text) => parse_issues(&text).into_iter().take(max_findings).collect(),
                Err(e) => {
                    warn!("LLM fallback failed: {e}");
                    Vec::new()
                }
            }
        } else {
            // Path 3: No findings, no high-risk — skip LLM entirely
            Vec::new()
        }
    }
}

// ── Agentic review via pi-core ──────────────────────────────────────────────

#[derive(Serialize)]
struct AgentInput<'a> {
    pr_title: &'a str,
    diff: &'a str,
    triage_section: &'a str,
    findings: &'a [DetectorFinding],
    entity_reviews: &'a [EntityReview],
    repo_dir: &'a str,
    provider: Option<&'a str>,
    model: Option<&'a str>,
}

#[derive(Deserialize)]
struct AgentOutput {
    #[serde(default)]
    verdicts: Vec<Verdict>,
}

/// Spawn the pi-core agentic validator as a child process.
/// Falls back to the single-shot `review_deterministic` if the agent is unavailable.
pub async fn review_agentic(
    state: &AppState,
    pr_title: &str,
    diff: &str,
    triage_section: &str,
    detector_findings: &[DetectorFinding],
    entity_reviews: &[EntityReview],
    repo_dir: &str,
    max_findings: usize,
) -> Vec<Finding> {
    let truncated = prompts::truncate_diff(diff, 80_000);

    let input = AgentInput {
        pr_title,
        diff: &truncated,
        triage_section,
        findings: detector_findings,
        entity_reviews,
        repo_dir,
        provider: None,  // Uses default (anthropic)
        model: None,     // Uses default (claude-sonnet-4-20250514)
    };

    let input_json = match serde_json::to_string(&input) {
        Ok(j) => j,
        Err(e) => {
            warn!("Failed to serialize agent input: {e}");
            return review_deterministic(
                state, pr_title, diff, triage_section, detector_findings, entity_reviews, max_findings,
            ).await;
        }
    };

    // Resolve agent script path relative to workspace root
    let agent_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("agent"))
        .unwrap_or_else(|| std::path::PathBuf::from("agent"));

    let result = tokio::process::Command::new("node")
        .arg("--import")
        .arg("tsx/esm")
        .arg("src/validate.ts")
        .current_dir(&agent_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match result {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to spawn agent process: {e}, falling back to single-shot");
            return review_deterministic(
                state, pr_title, diff, triage_section, detector_findings, entity_reviews, max_findings,
            ).await;
        }
    };

    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(e) = stdin.write_all(input_json.as_bytes()).await {
            warn!("Failed to write to agent stdin: {e}");
        }
        drop(stdin); // Close stdin so the agent reads EOF
    }

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => {
            warn!("Agent process failed: {e}, falling back to single-shot");
            return review_deterministic(
                state, pr_title, diff, triage_section, detector_findings, entity_reviews, max_findings,
            ).await;
        }
    };

    // Log stderr (tool calls, errors)
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    if !stderr_str.is_empty() {
        info!("Agent stderr:\n{}", stderr_str);
    }

    if !output.status.success() {
        warn!("Agent exited with status {}, falling back", output.status);
        return review_deterministic(
            state, pr_title, diff, triage_section, detector_findings, entity_reviews, max_findings,
        ).await;
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    match serde_json::from_str::<AgentOutput>(&stdout_str) {
        Ok(agent_out) => {
            agent_out
                .verdicts
                .into_iter()
                .filter(|v| v.verdict == "true_positive")
                .take(max_findings)
                .map(|v| {
                    let original = detector_findings
                        .iter()
                        .find(|f| f.rule_id == v.rule_id && f.entity_name == v.entity_name);
                    match original {
                        Some(f) => Finding {
                            issue: format!("[{}] {}", f.rule_id, f.message),
                            evidence: Some(f.evidence.clone()),
                            severity: Some(format!("{}", f.severity)),
                            file: Some(f.file_path.clone()),
                        },
                        None => Finding {
                            issue: format!("[{}] {}", v.rule_id, v.explanation),
                            evidence: None,
                            severity: None,
                            file: None,
                        },
                    }
                })
                .collect()
        }
        Err(e) => {
            warn!("Failed to parse agent output: {e}, raw: {}", stdout_str.chars().take(500).collect::<String>());
            review_deterministic(
                state, pr_title, diff, triage_section, detector_findings, entity_reviews, max_findings,
            ).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_issues_string_array() {
        let input = r#"{"issues": ["bug 1", "bug 2"]}"#;
        let findings = parse_issues(input);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].issue, "bug 1");
    }

    #[test]
    fn test_parse_issues_object_array() {
        let input = r#"{"issues": [{"issue": "null check missing", "evidence": "if (x)"}]}"#;
        let findings = parse_issues(input);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].issue, "null check missing");
        assert_eq!(findings[0].evidence.as_deref(), Some("if (x)"));
    }

    #[test]
    fn test_parse_issues_with_code_fence() {
        let input = "```json\n{\"issues\": [\"bug\"]}\n```";
        let findings = parse_issues(input);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_strip_code_fences() {
        assert_eq!(strip_code_fences("```json\n{}\n```"), "{}");
        assert_eq!(strip_code_fences("```\n{}\n```"), "{}");
        assert_eq!(strip_code_fences("{}"), "{}");
    }

    #[test]
    fn test_parse_verdicts_true_positive() {
        use inspect_core::detect::{DetectorKind, Severity};

        let findings = vec![DetectorFinding {
            rule_id: "foreach-async".to_string(),
            message: "forEach doesn't await async callbacks".to_string(),
            detector: DetectorKind::Pattern,
            confidence: 0.9,
            severity: Severity::High,
            entity_id: "e1".to_string(),
            entity_name: "processItems".to_string(),
            file_path: "src/app.ts".to_string(),
            evidence: "items.forEach(async".to_string(),
            start_line: 1,
            end_line: 1,
        }];

        let input = r#"{"verdicts": [{"rule_id": "foreach-async", "entity_name": "processItems", "verdict": "true_positive", "explanation": "confirmed"}]}"#;
        let result = parse_verdicts(input, &findings);
        assert_eq!(result.len(), 1);
        assert!(result[0].issue.contains("foreach-async"));
        assert_eq!(result[0].file.as_deref(), Some("src/app.ts"));
    }

    #[test]
    fn test_parse_verdicts_false_positive_filtered() {
        let input = r#"{"verdicts": [{"rule_id": "test", "entity_name": "foo", "verdict": "false_positive", "explanation": "not a bug"}]}"#;
        let result = parse_verdicts(input, &[]);
        assert_eq!(result.len(), 0);
    }
}
