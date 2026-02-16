use std::path::PathBuf;
use std::process::Command;

use clap::Args;
use sem_core::git::types::DiffScope;

use crate::formatters;
use crate::OutputFormat;
use inspect_core::analyze::{analyze, analyze_remote};
use inspect_core::github::GitHubClient;
use inspect_core::noise::is_noise_file;
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

    /// Remote repository (owner/repo). If set, fetches from GitHub API instead of local git.
    #[arg(long)]
    pub remote: Option<String>,

    /// Repository path (for local mode)
    #[arg(short = 'C', long, default_value = ".")]
    pub repo: PathBuf,
}

pub async fn run(args: PrArgs) {
    if let Some(ref remote_repo) = args.remote {
        run_remote(&args, remote_repo).await;
    } else {
        run_local(&args);
    }
}

fn run_local(args: &PrArgs) {
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());

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
            apply_filters_and_print(&mut result, args);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn run_remote(args: &PrArgs, remote_repo: &str) {
    let client = match GitHubClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!("Fetching PR #{} from {}...", args.number, remote_repo);

    let pr = match client.get_pr(remote_repo, args.number).await {
        Ok(pr) => pr,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let visible_files: Vec<_> = pr
        .files
        .iter()
        .filter(|f| !is_noise_file(&f.filename))
        .cloned()
        .collect();

    let noise_count = pr.files.len() - visible_files.len();
    if noise_count > 0 {
        eprintln!("({} noise files hidden)", noise_count);
    }

    eprintln!("Fetching {} file contents...", visible_files.len());

    let file_pairs = client
        .get_file_pairs(remote_repo, &visible_files, &pr.base_ref, &pr.head_ref)
        .await;

    match analyze_remote(&file_pairs) {
        Ok(mut result) => {
            apply_filters_and_print(&mut result, args);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn apply_filters_and_print(
    result: &mut inspect_core::types::ReviewResult,
    args: &PrArgs,
) {
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
        OutputFormat::Terminal => formatters::terminal::print(result, args.context),
        OutputFormat::Json => formatters::json::print(result),
        OutputFormat::Markdown => formatters::markdown::print(result, args.context),
    }
}
