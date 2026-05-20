use std::path::Path;

use sem_core::git::types::DiffScope;

use crate::analyze::{build_context, AnalyzeError};
use crate::classify::classify_change;
use crate::risk::{is_public_api, predict_risk_score, score_to_level};
use crate::types::*;

/// Options for controlling predict behavior.
pub struct PredictOptions {
    /// Maximum at-risk entities to show per changed entity.
    pub max_at_risk_per_change: usize,
    /// Skip dependent entities larger than this many lines.
    pub max_entity_lines: usize,
    /// Minimum risk level to include in output.
    pub min_risk: RiskLevel,
}

impl Default for PredictOptions {
    fn default() -> Self {
        Self {
            max_at_risk_per_change: 10,
            max_entity_lines: 100,
            min_risk: RiskLevel::Low,
        }
    }
}

/// Predict which unchanged entities are at risk from a set of changes.
pub fn predict(repo_path: &Path, scope: DiffScope) -> Result<PredictResult, AnalyzeError> {
    predict_with_options(repo_path, scope, &PredictOptions::default())
}

/// Predict with configurable options.
pub fn predict_with_options(
    repo_path: &Path,
    scope: DiffScope,
    options: &PredictOptions,
) -> Result<PredictResult, AnalyzeError> {
    use std::time::Instant;

    let total_start = Instant::now();

    let ctx = match build_context(repo_path, scope)? {
        Some(ctx) => ctx,
        None => {
            return Ok(PredictResult {
                threats: vec![],
                total_changes: 0,
                total_at_risk: 0,
                at_risk_by_level: RiskBreakdown {
                    critical: 0,
                    high: 0,
                    medium: 0,
                    low: 0,
                },
                timing: Timing::default(),
            });
        }
    };

    let scoring_start = Instant::now();
    let mut threats: Vec<ThreatSource> = Vec::new();

    for change in &ctx.changes {
        // Added entities can't break anything
        if change.change_type == sem_core::model::change::ChangeType::Added {
            continue;
        }

        let dependents = ctx.graph.get_dependents(&change.entity_id);
        if dependents.is_empty() {
            continue;
        }

        let classification = classify_change(change);
        let source_file = ctx
            .graph
            .entities
            .get(&change.entity_id)
            .map(|e| e.file_path.as_str())
            .unwrap_or("");

        let mut at_risk: Vec<AtRiskEntity> = Vec::new();

        for dep in &dependents {
            // Skip dependents that are themselves changed (already in the diff)
            if ctx.changed_entity_ids.contains(&dep.id) {
                continue;
            }

            let line_count = dep.end_line.saturating_sub(dep.start_line) + 1;
            if line_count > options.max_entity_lines {
                continue;
            }

            // Read source from the same tree used to build the graph.
            let file_content = match std::fs::read_to_string(ctx.source_root.join(&dep.file_path)) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let lines: Vec<&str> = file_content.lines().collect();
            let start = dep.start_line.saturating_sub(1);
            let end = dep.end_line.min(lines.len());
            if start >= lines.len() || start >= end {
                continue;
            }
            let content = lines[start..end].join("\n");

            let first_line = lines.get(start).copied().unwrap_or("");
            let is_pub = is_public_api(&dep.entity_type, &dep.name, Some(first_line));
            let is_cross_file = dep.file_path != source_file;
            let own_dep_count = ctx.graph.get_dependents(&dep.id).len();

            let risk_score = predict_risk_score(
                own_dep_count,
                is_pub,
                is_cross_file,
                classification,
                change.change_type,
            );
            let risk_level = score_to_level(risk_score);

            if risk_level < options.min_risk {
                continue;
            }

            at_risk.push(AtRiskEntity {
                entity_name: dep.name.clone(),
                entity_type: dep.entity_type.clone(),
                file_path: dep.file_path.clone(),
                start_line: dep.start_line,
                end_line: dep.end_line,
                content,
                risk_level,
                risk_score,
                own_dependent_count: own_dep_count,
                is_public_api: is_pub,
                is_cross_file,
            });
        }

        if at_risk.is_empty() {
            continue;
        }

        // Sort by risk descending, cap
        at_risk.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
        at_risk.truncate(options.max_at_risk_per_change);

        threats.push(ThreatSource {
            entity_name: change.entity_name.clone(),
            entity_type: change.entity_type.clone(),
            file_path: change.file_path.clone(),
            change_type: change.change_type,
            classification,
            at_risk,
        });
    }

    // Sort threats by total at-risk count descending
    threats.sort_by(|a, b| b.at_risk.len().cmp(&a.at_risk.len()));

    // Compute stats
    let total_at_risk: usize = threats.iter().map(|t| t.at_risk.len()).sum();
    let mut at_risk_by_level = RiskBreakdown {
        critical: 0,
        high: 0,
        medium: 0,
        low: 0,
    };
    for threat in &threats {
        for entity in &threat.at_risk {
            match entity.risk_level {
                RiskLevel::Critical => at_risk_by_level.critical += 1,
                RiskLevel::High => at_risk_by_level.high += 1,
                RiskLevel::Medium => at_risk_by_level.medium += 1,
                RiskLevel::Low => at_risk_by_level.low += 1,
            }
        }
    }

    let scoring_ms = scoring_start.elapsed().as_millis() as u64;
    let total_ms = total_start.elapsed().as_millis() as u64;

    let timing = Timing {
        diff_ms: ctx.diff_ms,
        list_files_ms: ctx.list_files_ms,
        file_count: ctx.file_count,
        graph_build_ms: ctx.graph_build_ms,
        graph_entity_count: ctx.total_graph_entities,
        scoring_ms,
        total_ms,
    };

    Ok(PredictResult {
        total_changes: ctx.changes.len(),
        threats,
        total_at_risk,
        at_risk_by_level,
        timing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    fn commit(dir: &Path, msg: &str) -> String {
        let add = Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(add.status.success(), "git add failed");

        let commit = Command::new("git")
            .args(["commit", "-m", msg, "--allow-empty"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            commit.status.success(),
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );

        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "git rev-parse failed");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn predict_finds_at_risk_callers() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        // Initial commit: helper + 3 callers
        std::fs::write(
            dir.join("lib.py"),
            "def calculate_tax(amount):\n    return amount * 0.1\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("main.py"),
            concat!(
                "from lib import calculate_tax\n\n",
                "def process_payment(amount):\n    tax = calculate_tax(amount)\n    return amount + tax\n\n",
                "def process_refund(amount):\n    tax = calculate_tax(amount)\n    return amount - tax\n\n",
                "def generate_invoice(amount):\n    tax = calculate_tax(amount)\n    return f'Total: {amount + tax}'\n",
            ),
        )
        .unwrap();
        commit(dir, "init");

        // Change calculate_tax
        std::fs::write(
            dir.join("lib.py"),
            "def calculate_tax(amount, rate=0.2):\n    return amount * rate\n",
        )
        .unwrap();
        commit(dir, "change tax");

        let result = predict(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
        )
        .unwrap();

        // Should find at least 1 threat (calculate_tax changed)
        assert!(!result.threats.is_empty(), "Expected threats, got none");

        let threat = &result.threats[0];
        assert_eq!(threat.entity_name, "calculate_tax");
    }

    #[test]
    fn predict_commit_reads_at_risk_code_from_commit_tree() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(
            dir.join("lib.py"),
            "def calculate_tax(amount):\n    return amount * 0.1\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("main.py"),
            concat!(
                "from lib import calculate_tax\n\n",
                "def process_payment(amount):\n    tax = calculate_tax(amount)\n    return amount + tax\n",
            ),
        )
        .unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("lib.py"),
            "def calculate_tax(amount, rate=0.2):\n    return amount * rate\n",
        )
        .unwrap();
        let change_sha = commit(dir, "change tax");

        let rm = Command::new("git")
            .args(["rm", "-q", "lib.py", "main.py"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(rm.status.success(), "git rm failed");
        commit(dir, "remove files");

        let result = predict(dir, DiffScope::Commit { sha: change_sha }).unwrap();
        let threat = result
            .threats
            .iter()
            .find(|t| t.entity_name == "calculate_tax")
            .expect("calculate_tax should threaten historical callers");

        assert!(threat.at_risk.iter().any(|entity| {
            entity.entity_name == "process_payment" && entity.content.contains("calculate_tax")
        }));
    }

    #[test]
    fn predict_empty_for_added_entities() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(dir.join("main.py"), "").unwrap();
        commit(dir, "init");

        // Add new function (nobody calls it)
        std::fs::write(dir.join("main.py"), "def new_func():\n    return 42\n").unwrap();
        commit(dir, "add new func");

        let result = predict(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
        )
        .unwrap();

        // Added entities have no dependents
        assert_eq!(result.total_at_risk, 0);
    }
}
