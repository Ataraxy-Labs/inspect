use std::io::{self, Write};
use std::path::PathBuf;

use clap::Args;
use colored::Colorize;
use sem_core::git::types::DiffScope;

use crate::OutputFormat;
use inspect_core::analyze::{analyze_with_options, AnalyzeOptions};
use inspect_core::llm::{
    estimate_entity_input_tokens, AnthropicClient, EntityLlmReview, LlmProvider, LlmReviewOptions,
    LlmReviewStatus, LlmVerdict, OpenAIClient,
};
use inspect_core::types::RiskLevel;

#[derive(Args)]
pub struct ReviewArgs {
    /// Commit ref, range, or PR number (with --remote)
    pub target: String,

    /// Output format
    #[arg(long, value_enum, default_value = "terminal")]
    pub format: OutputFormat,

    /// Minimum risk level to review (default: high)
    #[arg(long, default_value = "high")]
    pub min_risk: String,

    /// Model to use (e.g. claude-sonnet-4-5-20250929, gpt-4o, llama3)
    #[arg(long, default_value = "claude-sonnet-4-5-20250929")]
    pub model: String,

    /// Max entities to send for LLM review
    #[arg(long, default_value = "10")]
    pub max_entities: usize,

    /// Repository path
    #[arg(short = 'C', long, default_value = ".")]
    pub repo: PathBuf,

    /// LLM provider: anthropic, openai, ollama. Inferred from --api-base if omitted.
    #[arg(long)]
    pub provider: Option<String>,

    /// Custom API base URL (e.g. http://localhost:8000/v1). Implies openai provider.
    #[arg(long)]
    pub api_base: Option<String>,

    /// API key (overrides env var)
    #[arg(long)]
    pub api_key: Option<String>,

    /// Remote repo (e.g. owner/repo). Target becomes PR number.
    #[arg(long)]
    pub remote: Option<String>,

    /// Review strategy (for remote reviews)
    #[arg(long)]
    pub strategy: Option<String>,

    /// Timeout in seconds for remote review polling (default: 120)
    #[arg(long, default_value = "120")]
    pub timeout: u64,

    /// Maximum estimated input tokens per entity prompt before skipping
    #[arg(long, default_value = "120000")]
    pub max_input_tokens: u64,

    /// Retries for 429/rate-limit API responses
    #[arg(long, default_value = "3")]
    pub max_retries: u32,
}

fn build_provider(args: &ReviewArgs) -> Result<Box<dyn LlmProvider>, String> {
    // Infer provider: explicit flag > api-base implies openai > default anthropic
    let provider = args.provider.as_deref().unwrap_or_else(|| {
        if args.api_base.is_some() {
            "openai"
        } else {
            "anthropic"
        }
    });

    let options = LlmReviewOptions {
        max_input_tokens: args.max_input_tokens,
        max_retries: args.max_retries,
        ..LlmReviewOptions::default()
    };

    match provider {
        "anthropic" => {
            let client =
                AnthropicClient::new_with_options(&args.model, args.api_key.as_deref(), options)?;
            Ok(Box::new(client))
        }
        "openai" => {
            let client = OpenAIClient::new_with_options(
                &args.model,
                args.api_base.as_deref(),
                args.api_key.as_deref(),
                options,
            )?;
            Ok(Box::new(client))
        }
        "ollama" => {
            let base = args
                .api_base
                .as_deref()
                .unwrap_or("http://localhost:11434/v1");
            let client = OpenAIClient::new_with_options(&args.model, Some(base), None, options)?;
            Ok(Box::new(client))
        }
        other => Err(format!(
            "Unknown provider '{}'. Use: anthropic, openai, ollama",
            other
        )),
    }
}

pub async fn run(args: ReviewArgs) {
    if args.remote.is_some() {
        return run_remote(args).await;
    }

    let scope = parse_scope(&args.target);
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

    let options = AnalyzeOptions {
        include_dependent_code: true,
        include_dependency_code: true,
        include_file_context: true,
        ..AnalyzeOptions::default()
    };

    let mut result = match analyze_with_options(&repo, scope, &options) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let total_entities = result.entity_reviews.len();

    let min_level = parse_risk_level(&args.min_risk);
    result.entity_reviews.retain(|r| r.risk_level >= min_level);
    result.entity_reviews.truncate(args.max_entities);

    let review_count = result.entity_reviews.len();

    if review_count == 0 {
        eprintln!("No entities at {} risk or above.", args.min_risk);
        std::process::exit(0);
    }

    let reduction = if total_entities > 0 {
        ((total_entities - review_count) as f64 / total_entities as f64 * 100.0) as u32
    } else {
        0
    };

    eprintln!(
        "Triaged {} entities -> {} for LLM review ({}% reduction)",
        total_entities, review_count, reduction
    );

    let client = match build_provider(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let mut reviews: Vec<EntityLlmReview> = Vec::new();
    let mut degraded_entities: Vec<String> = Vec::new();

    for (i, entity) in result.entity_reviews.iter().enumerate() {
        eprint!(
            "  [{}/{}] Reviewing {} ... ",
            i + 1,
            review_count,
            entity.entity_name
        );

        match client.review_entity(entity).await {
            Ok(review) => {
                eprintln!("{}", format_review_inline(&review));
                if entity.risk_level >= RiskLevel::High && !review.is_reviewed() {
                    degraded_entities.push(entity.entity_name.clone());
                }
                reviews.push(review);
            }
            Err(e) => {
                eprintln!("{}", format!("failed: {}", e).red().bold());
                let review =
                    EntityLlmReview::failed(entity, e, estimate_entity_input_tokens(entity));
                if entity.risk_level >= RiskLevel::High {
                    degraded_entities.push(entity.entity_name.clone());
                }
                reviews.push(review);
            }
        }
    }

    match args.format {
        OutputFormat::Terminal => print_terminal(&reviews),
        OutputFormat::Json => print_json(&reviews),
        OutputFormat::Markdown => print_markdown(&reviews),
    }

    if !degraded_entities.is_empty() {
        flush_stdout_before_exit();
        eprintln!(
            "{}",
            format!(
                "error: review degraded: {} high-risk entit{} not reviewed ({})",
                degraded_entities.len(),
                if degraded_entities.len() == 1 {
                    "y was"
                } else {
                    "ies were"
                },
                degraded_entities.join(", ")
            )
            .red()
        );
        std::process::exit(2);
    }
}

fn flush_stdout_before_exit() {
    if let Err(e) = io::stdout().flush() {
        eprintln!("{}", format!("error: failed to flush stdout: {}", e).red());
        std::process::exit(1);
    }
}

fn format_review_inline(review: &EntityLlmReview) -> String {
    match review.status {
        LlmReviewStatus::Reviewed => format_verdict_inline(review.verdict),
        LlmReviewStatus::Skipped => "skipped".yellow().to_string(),
        LlmReviewStatus::Failed => "failed".red().bold().to_string(),
    }
}

fn format_verdict_inline(verdict: LlmVerdict) -> String {
    match verdict {
        LlmVerdict::Approve => "approved".green().to_string(),
        LlmVerdict::Comment => "comment".yellow().to_string(),
        LlmVerdict::RequestChanges => "changes requested".red().bold().to_string(),
    }
}

fn print_terminal(reviews: &[EntityLlmReview]) {
    if reviews.is_empty() {
        return;
    }

    let summary = summarize_reviews(reviews);
    let total_tokens: u64 = reviews
        .iter()
        .filter(|r| r.is_reviewed())
        .map(|r| r.tokens_used)
        .sum();
    let changes_requested = reviews
        .iter()
        .filter(|r| r.is_reviewed() && r.verdict == LlmVerdict::RequestChanges)
        .count();
    let comments = reviews
        .iter()
        .filter(|r| r.is_reviewed() && r.verdict == LlmVerdict::Comment)
        .count();
    let approved = reviews
        .iter()
        .filter(|r| r.is_reviewed() && r.verdict == LlmVerdict::Approve)
        .count();

    println!(
        "\n{} {}/{} entities reviewed ({} tokens)",
        "review".bold().cyan(),
        summary.reviewed,
        summary.total,
        total_tokens,
    );
    println!(
        "  {} approved, {} comments, {} changes requested",
        format!("{}", approved).green(),
        format!("{}", comments).yellow(),
        format!("{}", changes_requested).red(),
    );
    if summary.failed > 0 || summary.skipped > 0 {
        println!(
            "  {} failed, {} skipped",
            format!("{}", summary.failed).red(),
            format!("{}", summary.skipped).yellow(),
        );
    }

    for review in reviews {
        let badge = format_review_badge(review);

        println!(
            "\n  {} {} {}",
            badge,
            review.entity_name.bold(),
            format!("({})", review.file_path).dimmed(),
        );

        if !review.summary.is_empty() {
            println!("    {}", review.summary);
        }

        if let Some(reason) = &review.failure_reason {
            println!("    {}", reason);
        }

        if review.is_reviewed() {
            for issue in &review.issues {
                let sev = match issue.severity.as_str() {
                    "error" => "error".red().bold().to_string(),
                    "warning" => "warning".yellow().to_string(),
                    _ => "info".dimmed().to_string(),
                };
                println!("    [{}] {}", sev, issue.description);
            }
        }
    }

    println!();
}

fn format_review_badge(review: &EntityLlmReview) -> String {
    match review.status {
        LlmReviewStatus::Reviewed => match review.verdict {
            LlmVerdict::Approve => " APPROVE ".on_green().white().bold().to_string(),
            LlmVerdict::Comment => " COMMENT ".on_yellow().black().bold().to_string(),
            LlmVerdict::RequestChanges => " CHANGES ".on_red().white().bold().to_string(),
        },
        LlmReviewStatus::Skipped => " SKIPPED ".on_yellow().black().bold().to_string(),
        LlmReviewStatus::Failed => " FAILED ".on_red().white().bold().to_string(),
    }
}

fn print_json(reviews: &[EntityLlmReview]) {
    println!("{}", serde_json::to_string_pretty(reviews).unwrap());
}

fn print_markdown(reviews: &[EntityLlmReview]) {
    println!("# Code Review\n");

    let summary = summarize_reviews(reviews);
    let changes_requested = reviews
        .iter()
        .filter(|r| r.is_reviewed() && r.verdict == LlmVerdict::RequestChanges)
        .count();
    let comments = reviews
        .iter()
        .filter(|r| r.is_reviewed() && r.verdict == LlmVerdict::Comment)
        .count();
    let approved = reviews
        .iter()
        .filter(|r| r.is_reviewed() && r.verdict == LlmVerdict::Approve)
        .count();

    println!(
        "{}/{} entities reviewed: {} approved, {} comments, {} changes requested\n",
        summary.reviewed, summary.total, approved, comments, changes_requested,
    );

    if summary.failed > 0 || summary.skipped > 0 {
        println!(
            "**Coverage gaps:** {} failed, {} skipped\n",
            summary.failed, summary.skipped
        );
    }

    for review in reviews {
        let verdict_str = format_markdown_status(review);

        println!(
            "## {} `{}` ({})\n",
            verdict_str, review.entity_name, review.file_path
        );

        if !review.summary.is_empty() {
            println!("{}\n", review.summary);
        }

        if let Some(reason) = &review.failure_reason {
            println!("**Reason:** {}\n", reason);
        }

        if review.is_reviewed() {
            for issue in &review.issues {
                println!("- **{}**: {}", issue.severity, issue.description);
            }
        }

        println!();
    }
}

fn format_markdown_status(review: &EntityLlmReview) -> &'static str {
    match review.status {
        LlmReviewStatus::Reviewed => match review.verdict {
            LlmVerdict::Approve => "Approve",
            LlmVerdict::Comment => "Comment",
            LlmVerdict::RequestChanges => "Changes Requested",
        },
        LlmReviewStatus::Skipped => "Skipped",
        LlmReviewStatus::Failed => "Failed",
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ReviewSummary {
    total: usize,
    reviewed: usize,
    failed: usize,
    skipped: usize,
}

fn summarize_reviews(reviews: &[EntityLlmReview]) -> ReviewSummary {
    let mut summary = ReviewSummary {
        total: reviews.len(),
        ..ReviewSummary::default()
    };

    for review in reviews {
        match review.status {
            LlmReviewStatus::Reviewed => summary.reviewed += 1,
            LlmReviewStatus::Failed => summary.failed += 1,
            LlmReviewStatus::Skipped => summary.skipped += 1,
        }
    }

    summary
}

async fn run_remote(args: ReviewArgs) {
    let remote = args.remote.as_ref().unwrap();
    let pr_number: u64 = match args.target.parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!(
                "{}",
                "error: target must be a PR number when using --remote".red()
            );
            std::process::exit(1);
        }
    };

    let creds = match crate::config::require_credentials() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e.red());
            std::process::exit(1);
        }
    };

    let client = reqwest::Client::new();

    eprintln!(
        "Submitting review for {} PR #{}...",
        remote.bold(),
        pr_number
    );

    // POST /v1/review
    let body = serde_json::json!({
        "repo": remote,
        "pr_number": pr_number,
        "strategy": args.strategy,
    });

    let resp = client
        .post(format!("{}/v1/review", creds.api_url))
        .header("Authorization", format!("Bearer {}", creds.api_key))
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", format!("error: could not reach API: {e}").red());
            std::process::exit(1);
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        eprintln!("{}", format!("error: API returned {status}: {text}").red());
        std::process::exit(1);
    }

    let create_resp: serde_json::Value = resp.json().await.unwrap();
    let job_id = create_resp["id"].as_str().unwrap().to_string();

    // Poll GET /v1/review/{id}
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.timeout);
    let poll_interval = std::time::Duration::from_secs(2);

    loop {
        if std::time::Instant::now() > deadline {
            eprintln!(
                "{}",
                format!("error: timed out after {}s", args.timeout).red()
            );
            std::process::exit(1);
        }

        tokio::time::sleep(poll_interval).await;

        let poll = client
            .get(format!("{}/v1/review/{}", creds.api_url, job_id))
            .header("Authorization", format!("Bearer {}", creds.api_key))
            .send()
            .await;

        let poll_resp = match poll {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  poll error: {e}");
                continue;
            }
        };

        let job: serde_json::Value = poll_resp.json().await.unwrap();
        let status = job["status"].as_str().unwrap_or("unknown");

        eprint!("\r  Status: {}    ", status);

        match status {
            "complete" => {
                eprintln!();
                print_remote_result(&args, &job);
                return;
            }
            "failed" => {
                eprintln!();
                let err = job["error"].as_str().unwrap_or("unknown error");
                eprintln!("{}", format!("error: review failed: {err}").red());
                std::process::exit(1);
            }
            _ => continue,
        }
    }
}

fn print_remote_result(args: &ReviewArgs, job: &serde_json::Value) {
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&job["result"]).unwrap());
        }
        OutputFormat::Markdown | OutputFormat::Terminal => {
            let result = &job["result"];
            let triage = &result["triage"];
            let timing = &result["timing"];

            let verdict = triage["verdict"].as_str().unwrap_or("unknown");
            let total_entities = triage["total_entities"].as_u64().unwrap_or(0);

            eprintln!(
                "Triage: {} entities, verdict: {}",
                total_entities,
                match verdict {
                    "approve" => verdict.green().to_string(),
                    "review" => verdict.yellow().to_string(),
                    _ => verdict.red().to_string(),
                }
            );

            let findings = result["findings"].as_array();
            let finding_count = findings.map(|f| f.len()).unwrap_or(0);

            if finding_count > 0 {
                println!("{} findings:", finding_count);
                for f in findings.unwrap() {
                    let severity = f["severity"].as_str().unwrap_or("info");
                    let description = f["description"].as_str().unwrap_or("");
                    let file = f["file"].as_str().unwrap_or("");
                    let sev_colored = match severity {
                        "error" | "critical" | "high" => {
                            format!("[{}]", severity).red().to_string()
                        }
                        "warning" | "medium" => format!("[{}]", severity).yellow().to_string(),
                        _ => format!("[{}]", severity).dimmed().to_string(),
                    };
                    println!("  {} {} in {}", sev_colored, description, file.dimmed());
                }
            } else {
                println!("No findings.");
            }

            let triage_ms = timing["triage_ms"].as_u64().unwrap_or(0);
            let review_ms = timing["review_ms"].as_u64().unwrap_or(0);
            let total_ms = timing["total_ms"].as_u64().unwrap_or(0);
            eprintln!(
                "Timing: triage {}ms, review {}ms, total {}ms",
                triage_ms, review_ms, total_ms
            );
        }
    }
}

fn parse_scope(target: &str) -> DiffScope {
    if target.contains("..") {
        let parts: Vec<&str> = target.split("..").collect();
        DiffScope::Range {
            from: parts[0].to_string(),
            to: parts[1].to_string(),
        }
    } else {
        DiffScope::Commit {
            sha: target.to_string(),
        }
    }
}

fn parse_risk_level(s: &str) -> RiskLevel {
    match s.to_lowercase().as_str() {
        "critical" => RiskLevel::Critical,
        "high" => RiskLevel::High,
        "medium" => RiskLevel::Medium,
        _ => RiskLevel::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inspect_core::llm::LlmIssue;

    fn llm_review(status: LlmReviewStatus, verdict: LlmVerdict) -> EntityLlmReview {
        EntityLlmReview {
            entity_name: "handle".to_string(),
            file_path: "src/lib.rs".to_string(),
            status,
            verdict,
            issues: vec![LlmIssue {
                severity: "info".to_string(),
                description: "detail".to_string(),
            }],
            summary: "summary".to_string(),
            tokens_used: 10,
            estimated_input_tokens: 20,
            failure_reason: None,
        }
    }

    #[test]
    fn summary_counts_reviewed_failed_and_skipped_separately() {
        let reviews = vec![
            llm_review(LlmReviewStatus::Reviewed, LlmVerdict::Approve),
            llm_review(LlmReviewStatus::Failed, LlmVerdict::Comment),
            llm_review(LlmReviewStatus::Skipped, LlmVerdict::Comment),
        ];

        assert_eq!(
            summarize_reviews(&reviews),
            ReviewSummary {
                total: 3,
                reviewed: 1,
                failed: 1,
                skipped: 1,
            }
        );
    }

    #[test]
    fn markdown_status_uses_review_status_before_verdict() {
        let failed = llm_review(LlmReviewStatus::Failed, LlmVerdict::Approve);
        let skipped = llm_review(LlmReviewStatus::Skipped, LlmVerdict::RequestChanges);
        let reviewed = llm_review(LlmReviewStatus::Reviewed, LlmVerdict::RequestChanges);

        assert_eq!(format_markdown_status(&failed), "Failed");
        assert_eq!(format_markdown_status(&skipped), "Skipped");
        assert_eq!(format_markdown_status(&reviewed), "Changes Requested");
    }
}
