use std::path::PathBuf;

use clap::Args;

use crate::commands::pr_source::{
    analyze_explicit_range, analyze_github_pr, analyze_local_pr, FetchSource,
};
use crate::formatters;
use crate::OutputFormat;
use inspect_core::analyze::retain_entity_reviews;
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

    /// PR content source
    #[arg(long, value_enum, default_value = "local")]
    pub fetch: FetchSource,

    /// Remote repository (owner/repo), required with --fetch github.
    #[arg(long)]
    pub remote: Option<String>,

    /// Explicit base commit/ref to compare instead of looking up PR refs.
    #[arg(long, requires = "head")]
    pub base: Option<String>,

    /// Explicit head commit/ref to compare instead of looking up PR refs.
    #[arg(long, requires = "base")]
    pub head: Option<String>,

    /// Repository path (for local mode)
    #[arg(short = 'C', long, default_value = ".")]
    pub repo: PathBuf,
}

pub async fn run(args: PrArgs) {
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

    let result = if args.base.is_some() || args.head.is_some() {
        if args.fetch == FetchSource::Github {
            eprintln!("error: --base/--head cannot be combined with --fetch github");
            std::process::exit(1);
        }
        if args.remote.is_some() {
            eprintln!("error: --remote is only used with --fetch github");
            std::process::exit(1);
        }
        analyze_explicit_range(&repo, args.base.as_deref(), args.head.as_deref())
            .and_then(|result| result.ok_or_else(|| "missing --base/--head".to_string()))
    } else {
        match args.fetch {
            FetchSource::Local => {
                if args.remote.is_some() {
                    eprintln!(
                        "error: --remote names a GitHub repository; use --fetch github --remote owner/repo"
                    );
                    std::process::exit(1);
                }
                analyze_local_pr(&repo, args.number)
            }
            FetchSource::Github => {
                let Some(remote_repo) = args.remote.as_deref() else {
                    eprintln!("error: --fetch github requires --remote owner/repo");
                    std::process::exit(1);
                };
                analyze_github_pr(remote_repo, args.number).await
            }
        }
    };

    match result {
        Ok(mut result) => {
            apply_filters_and_print(&mut result, &args);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn apply_filters_and_print(result: &mut inspect_core::types::ReviewResult, args: &PrArgs) {
    if let Some(ref min) = args.min_risk {
        let min_level = match min.to_lowercase().as_str() {
            "critical" => RiskLevel::Critical,
            "high" => RiskLevel::High,
            "medium" => RiskLevel::Medium,
            _ => RiskLevel::Low,
        };
        retain_entity_reviews(result, |r| r.risk_level >= min_level);
    }

    match args.format {
        OutputFormat::Terminal => formatters::terminal::print(result, args.context),
        OutputFormat::Json => formatters::json::print(result),
        OutputFormat::Markdown => formatters::markdown::print(result, args.context),
    }
}
