use std::path::PathBuf;

use clap::Args;
use sem_core::git::types::DiffScope;

use crate::formatters;
use crate::OutputFormat;
use inspect_core::analyze::analyze;
use inspect_core::types::RiskLevel;

#[derive(Args)]
pub struct DiffArgs {
    /// Commit ref or range (e.g. HEAD~1, main..feature, abc123)
    pub target: String,

    /// Output format
    #[arg(long, value_enum, default_value = "terminal")]
    pub format: OutputFormat,

    /// Minimum risk level to show
    #[arg(long)]
    pub min_risk: Option<String>,

    /// Show dependency context
    #[arg(long)]
    pub context: bool,

    /// Repository path
    #[arg(short = 'C', long, default_value = ".")]
    pub repo: PathBuf,
}

pub fn run(args: DiffArgs) {
    let scope = parse_scope(&args.target);
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

    match analyze(&repo, scope) {
        Ok(mut result) => {
            // Filter by min risk if specified
            if let Some(ref min) = args.min_risk {
                let min_level = parse_risk_level(min);
                result.entity_reviews.retain(|r| r.risk_level >= min_level);
            }

            match args.format {
                OutputFormat::Terminal => formatters::terminal::print(&result, args.context),
                OutputFormat::Json => formatters::json::print(&result),
                OutputFormat::Markdown => formatters::markdown::print(&result, args.context),
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
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
