use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use clap::Args;
use sem_core::git::types::DiffScope;

use inspect_core::analyze::analyze;
use inspect_core::types::RiskLevel;

use serde::Serialize;

#[derive(Args)]
pub struct BenchArgs {
    /// Repository path
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,

    /// Maximum number of commits to analyze
    #[arg(long, default_value = "100")]
    pub limit: usize,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkResult {
    pub repo: String,
    pub total_commits: usize,
    pub analyzed_commits: usize,
    pub total_entities_reviewed: usize,
    // Noise reduction
    pub cosmetic_ratio: f64,
    pub noise_reduction: f64,
    pub avg_entities_per_file: f64,
    // Risk distribution
    pub risk_distribution: RiskDistribution,
    pub avg_blast_radius: f64,
    pub max_blast_radius: usize,
    // Grouping
    pub avg_groups_per_commit: f64,
    pub tangled_commit_ratio: f64,
    // Entity vs file comparison
    pub avg_files_per_commit: f64,
    pub avg_entities_per_commit: f64,
    pub high_critical_ratio: f64,
    // Cross-file impact
    pub cross_file_impact_ratio: f64,
    // Per-commit detail
    pub commits: Vec<CommitBenchmark>,
}

#[derive(Debug, Serialize)]
pub struct RiskDistribution {
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

#[derive(Debug, Serialize)]
pub struct CommitBenchmark {
    pub sha: String,
    pub message: String,
    pub entity_count: usize,
    pub file_count: usize,
    pub group_count: usize,
    pub cosmetic_count: usize,
    pub high_critical_count: usize,
    pub max_blast_radius: usize,
    pub cross_file_impacts: usize,
}

pub fn run(args: BenchArgs) {
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());
    let repo_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repo.display().to_string());

    eprintln!("inspect bench: analyzing {} (limit: {})", repo.display(), args.limit);

    // Get commit SHAs
    let output = Command::new("git")
        .args(["log", "--format=%H %s", &format!("-{}", args.limit)])
        .current_dir(&repo)
        .output()
        .expect("failed to run git log");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let commits_info: Vec<(&str, &str)> = stdout
        .lines()
        .filter_map(|line| {
            let (sha, msg) = line.split_once(' ')?;
            Some((sha, msg))
        })
        .collect();

    if commits_info.is_empty() {
        eprintln!("no commits found");
        return;
    }

    eprintln!("found {} commits", commits_info.len());

    let mut commit_benchmarks: Vec<CommitBenchmark> = Vec::new();
    let mut total_entities = 0usize;
    let mut total_cosmetic = 0usize;
    let mut total_files = 0usize;
    let mut total_groups = 0usize;
    let mut total_blast_radius = 0usize;
    let mut max_blast_radius = 0usize;
    let mut total_high_critical = 0usize;
    let mut total_cross_file = 0usize;
    let mut risk_dist = RiskDistribution {
        critical: 0,
        high: 0,
        medium: 0,
        low: 0,
    };
    let mut tangled_commits = 0usize;

    for (i, (sha, msg)) in commits_info.iter().enumerate() {
        eprint!("\r  [{}/{}] {:.50}", i + 1, commits_info.len(), msg);

        let scope = DiffScope::Commit {
            sha: sha.to_string(),
        };

        match analyze(&repo, scope) {
            Ok(result) => {
                if result.entity_reviews.is_empty() {
                    continue;
                }

                let entity_count = result.entity_reviews.len();
                let files: HashSet<&str> = result
                    .entity_reviews
                    .iter()
                    .map(|r| r.file_path.as_str())
                    .collect();
                let file_count = files.len();
                let group_count = result.groups.len();

                let cosmetic_count = result
                    .entity_reviews
                    .iter()
                    .filter(|r| r.structural_change == Some(false))
                    .count();

                let hc_count = result
                    .entity_reviews
                    .iter()
                    .filter(|r| matches!(r.risk_level, RiskLevel::High | RiskLevel::Critical))
                    .count();

                let commit_max_blast = result
                    .entity_reviews
                    .iter()
                    .map(|r| r.blast_radius)
                    .max()
                    .unwrap_or(0);

                let commit_blast_sum: usize =
                    result.entity_reviews.iter().map(|r| r.blast_radius).sum();

                // Cross-file: entities with dependents (potential cross-file impact)
                let cross_file_count = result
                    .entity_reviews
                    .iter()
                    .filter(|r| r.dependent_count > 0)
                    .count();

                for r in &result.entity_reviews {
                    match r.risk_level {
                        RiskLevel::Critical => risk_dist.critical += 1,
                        RiskLevel::High => risk_dist.high += 1,
                        RiskLevel::Medium => risk_dist.medium += 1,
                        RiskLevel::Low => risk_dist.low += 1,
                    }
                }

                if group_count > 1 {
                    tangled_commits += 1;
                }

                total_entities += entity_count;
                total_cosmetic += cosmetic_count;
                total_files += file_count;
                total_groups += group_count;
                total_blast_radius += commit_blast_sum;
                if commit_max_blast > max_blast_radius {
                    max_blast_radius = commit_max_blast;
                }
                total_high_critical += hc_count;
                total_cross_file += cross_file_count;

                commit_benchmarks.push(CommitBenchmark {
                    sha: sha.to_string(),
                    message: msg.to_string(),
                    entity_count,
                    file_count,
                    group_count,
                    cosmetic_count,
                    high_critical_count: hc_count,
                    max_blast_radius: commit_max_blast,
                    cross_file_impacts: cross_file_count,
                });
            }
            Err(_) => {
                // Skip commits that fail (e.g. initial commit with no parent)
            }
        }
    }

    eprintln!("\r  done.                                        ");

    let analyzed = commit_benchmarks.len();
    let cosmetic_ratio = if total_entities > 0 {
        total_cosmetic as f64 / total_entities as f64
    } else {
        0.0
    };

    // Noise reduction: how many fewer items to review vs file-level
    // If a commit touches 5 files but only 3 entities matter, that's 40% reduction
    let noise_reduction = if total_files > 0 && total_entities < total_files {
        1.0 - (total_entities as f64 / total_files as f64)
    } else {
        // Entity-level can be more granular (more entities than files)
        // In this case, noise reduction is the cosmetic ratio (items that can be skipped)
        cosmetic_ratio
    };

    let avg_entities_per_file = if total_files > 0 {
        total_entities as f64 / total_files as f64
    } else {
        0.0
    };

    let avg_blast_radius = if total_entities > 0 {
        total_blast_radius as f64 / total_entities as f64
    } else {
        0.0
    };

    let avg_groups_per_commit = if analyzed > 0 {
        total_groups as f64 / analyzed as f64
    } else {
        0.0
    };

    let tangled_commit_ratio = if analyzed > 0 {
        tangled_commits as f64 / analyzed as f64
    } else {
        0.0
    };

    let avg_files_per_commit = if analyzed > 0 {
        total_files as f64 / analyzed as f64
    } else {
        0.0
    };

    let avg_entities_per_commit = if analyzed > 0 {
        total_entities as f64 / analyzed as f64
    } else {
        0.0
    };

    let high_critical_ratio = if total_entities > 0 {
        total_high_critical as f64 / total_entities as f64
    } else {
        0.0
    };

    let cross_file_impact_ratio = if total_entities > 0 {
        total_cross_file as f64 / total_entities as f64
    } else {
        0.0
    };

    let result = BenchmarkResult {
        repo: repo_name,
        total_commits: commits_info.len(),
        analyzed_commits: analyzed,
        total_entities_reviewed: total_entities,
        cosmetic_ratio,
        noise_reduction,
        avg_entities_per_file,
        risk_distribution: risk_dist,
        avg_blast_radius,
        max_blast_radius,
        avg_groups_per_commit,
        tangled_commit_ratio,
        avg_files_per_commit,
        avg_entities_per_commit,
        high_critical_ratio,
        cross_file_impact_ratio,
        commits: commit_benchmarks,
    };

    let json = serde_json::to_string_pretty(&result).expect("failed to serialize");
    println!("{}", json);
}
