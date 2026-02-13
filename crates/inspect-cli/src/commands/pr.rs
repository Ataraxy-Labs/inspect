use std::path::PathBuf;
use std::process::Command;

use clap::Args;
use sem_core::git::types::DiffScope;

use crate::formatters;
use crate::OutputFormat;
use inspect_core::analyze::analyze;
use inspect_core::types::RiskLevel;

#[derive(Args)]
pub struct PrArgs {
    /// PR number
    pub number: u64,

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

pub fn run(args: PrArgs) {
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

    // Get PR base and head refs via gh CLI
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &args.number.to_string(),
            "--json",
            "baseRefName,headRefName",
        ])
        .current_dir(&repo)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!(
                "error: gh pr view failed: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: could not run gh CLI: {}", e);
            std::process::exit(1);
        }
    };

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("invalid gh output");
    let base = json["baseRefName"].as_str().unwrap_or("main");
    let head = json["headRefName"].as_str().unwrap_or("HEAD");

    let scope = DiffScope::Range {
        from: base.to_string(),
        to: head.to_string(),
    };

    match analyze(&repo, scope) {
        Ok(mut result) => {
            if let Some(ref min) = args.min_risk {
                let min_level = match min.to_lowercase().as_str() {
                    "critical" => RiskLevel::Critical,
                    "high" => RiskLevel::High,
                    "medium" => RiskLevel::Medium,
                    _ => RiskLevel::Low,
                };
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
