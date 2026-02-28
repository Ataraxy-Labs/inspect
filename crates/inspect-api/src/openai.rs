use serde::{Deserialize, Serialize};
use tracing::warn;

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

/// Call OpenAI chat completions API.
async fn call_openai(
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
    let (pass_0, pass_1) = tokio::join!(
        call_openai(state, prompts::SYSTEM_REVIEW, &prompt, 0.0),
        call_openai(state, prompts::SYSTEM_REVIEW, &prompt, 0.3),
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
    let text = call_openai(state, prompts::SYSTEM_VALIDATE, &prompt, 0.0).await?;
    Ok(parse_issues(&text))
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
}
