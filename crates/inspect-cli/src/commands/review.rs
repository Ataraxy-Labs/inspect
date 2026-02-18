use std::path::PathBuf;

use clap::Args;
use colored::Colorize;
use sem_core::git::types::DiffScope;

use crate::OutputFormat;
use inspect_core::analyze::analyze;
use inspect_core::llm::{AnthropicClient, EntityLlmReview, LlmVerdict};
use inspect_core::types::RiskLevel;

#[derive(Args)]
pub struct ReviewArgs {
    /// Commit ref or range (e.g. HEAD~1, main..feature, abc123)
    pub target: String,

    /// Output format
    #[arg(long, value_enum, default_value = "terminal")]
    pub format: OutputFormat,

    /// Minimum risk level to review (default: high)
    #[arg(long, default_value = "high")]
    pub min_risk: String,

    /// Claude model to use
    #[arg(long, default_value = "claude-sonnet-4-5-20250929")]
    pub model: String,

    /// Max entities to send for LLM review
    #[arg(long, default_value = "10")]
    pub max_entities: usize,

    /// Repository path
    #[arg(short = 'C', long, default_value = ".")]
    pub repo: PathBuf,
}

pub async fn run(args: ReviewArgs) {
    let scope = parse_scope(&args.target);
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

    let mut result = match analyze(&repo, scope) {
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

    let client = match AnthropicClient::new(&args.model) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let mut reviews: Vec<EntityLlmReview> = Vec::new();

    for (i, entity) in result.entity_reviews.iter().enumerate() {
        eprint!(
            "  [{}/{}] Reviewing {} ... ",
            i + 1,
            review_count,
            entity.entity_name
        );

        match client.review_entity(entity).await {
            Ok(review) => {
                eprintln!("{}", format_verdict_inline(review.verdict));
                reviews.push(review);
            }
            Err(e) => {
                eprintln!("{}", format!("error: {}", e).red());
            }
        }
    }

    match args.format {
        OutputFormat::Terminal => print_terminal(&reviews),
        OutputFormat::Json => print_json(&reviews),
        OutputFormat::Markdown => print_markdown(&reviews),
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

    let total_tokens: u64 = reviews.iter().map(|r| r.tokens_used).sum();
    let changes_requested = reviews
        .iter()
        .filter(|r| r.verdict == LlmVerdict::RequestChanges)
        .count();
    let comments = reviews
        .iter()
        .filter(|r| r.verdict == LlmVerdict::Comment)
        .count();
    let approved = reviews
        .iter()
        .filter(|r| r.verdict == LlmVerdict::Approve)
        .count();

    println!(
        "\n{} {} entities reviewed ({} tokens)",
        "review".bold().cyan(),
        reviews.len(),
        total_tokens,
    );
    println!(
        "  {} approved, {} comments, {} changes requested",
        format!("{}", approved).green(),
        format!("{}", comments).yellow(),
        format!("{}", changes_requested).red(),
    );

    for review in reviews {
        let badge = match review.verdict {
            LlmVerdict::Approve => " APPROVE ".on_green().white().bold().to_string(),
            LlmVerdict::Comment => " COMMENT ".on_yellow().black().bold().to_string(),
            LlmVerdict::RequestChanges => {
                " CHANGES ".on_red().white().bold().to_string()
            }
        };

        println!(
            "\n  {} {} {}",
            badge,
            review.entity_name.bold(),
            format!("({})", review.file_path).dimmed(),
        );

        if !review.summary.is_empty() {
            println!("    {}", review.summary);
        }

        for issue in &review.issues {
            let sev = match issue.severity.as_str() {
                "error" => "error".red().bold().to_string(),
                "warning" => "warning".yellow().to_string(),
                _ => "info".dimmed().to_string(),
            };
            println!("    [{}] {}", sev, issue.description);
        }
    }

    println!();
}

fn print_json(reviews: &[EntityLlmReview]) {
    println!("{}", serde_json::to_string_pretty(reviews).unwrap());
}

fn print_markdown(reviews: &[EntityLlmReview]) {
    println!("# Code Review\n");

    let changes_requested = reviews
        .iter()
        .filter(|r| r.verdict == LlmVerdict::RequestChanges)
        .count();
    let comments = reviews
        .iter()
        .filter(|r| r.verdict == LlmVerdict::Comment)
        .count();
    let approved = reviews
        .iter()
        .filter(|r| r.verdict == LlmVerdict::Approve)
        .count();

    println!(
        "{} entities reviewed: {} approved, {} comments, {} changes requested\n",
        reviews.len(),
        approved,
        comments,
        changes_requested,
    );

    for review in reviews {
        let verdict_str = match review.verdict {
            LlmVerdict::Approve => "Approve",
            LlmVerdict::Comment => "Comment",
            LlmVerdict::RequestChanges => "Changes Requested",
        };

        println!(
            "## {} `{}` ({})\n",
            verdict_str, review.entity_name, review.file_path
        );

        if !review.summary.is_empty() {
            println!("{}\n", review.summary);
        }

        for issue in &review.issues {
            println!("- **{}**: {}", issue.severity, issue.description);
        }

        println!();
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
