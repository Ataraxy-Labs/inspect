use std::path::PathBuf;

use clap::Args;
use sem_core::git::types::DiffScope;

use crate::formatters;
use crate::OutputFormat;
use inspect_core::analyze::analyze;
use inspect_core::types::RiskLevel;

#[derive(Args)]
pub struct FileArgs {
    /// File path to inspect
    pub path: String,

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

pub fn run(args: FileArgs) {
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

    // Use working tree diff (uncommitted changes)
    let scope = DiffScope::Working;

    match analyze(&repo, scope) {
        Ok(mut result) => {
            // Filter to only the specified file
            result
                .entity_reviews
                .retain(|r| r.file_path.ends_with(&args.path));

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
