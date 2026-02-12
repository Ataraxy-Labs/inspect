use std::collections::{HashMap, HashSet};
use std::path::Path;

use sem_core::git::bridge::GitBridge;
use sem_core::git::types::DiffScope;
use sem_core::model::change::ChangeType;
use sem_core::parser::differ::compute_semantic_diff;
use sem_core::parser::graph::EntityGraph;
use sem_core::parser::plugins::create_default_registry;

use crate::classify::classify_change;
use crate::risk::{compute_risk_score, is_public_api, score_to_level};
use crate::types::*;
use crate::untangle::untangle;

/// Analyze a diff scope and produce a ReviewResult.
pub fn analyze(repo_path: &Path, scope: DiffScope) -> Result<ReviewResult, AnalyzeError> {
    use std::time::Instant;

    let total_start = Instant::now();
    let git = GitBridge::open(repo_path).map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let registry = create_default_registry();

    // Get file changes
    let file_changes = git
        .get_changed_files(&scope)
        .map_err(|e| AnalyzeError::Git(e.to_string()))?;

    if file_changes.is_empty() {
        return Ok(empty_result());
    }

    // Phase 1: Compute entity-level diff
    let diff_start = Instant::now();
    let diff = compute_semantic_diff(&file_changes, &registry, None, None);
    let diff_ms = diff_start.elapsed().as_millis() as u64;

    if diff.changes.is_empty() {
        return Ok(empty_result());
    }

    // Phase 2: List all source files in the repo
    let list_start = Instant::now();
    let all_files = list_source_files(repo_path)?;
    let file_count = all_files.len();
    let list_files_ms = list_start.elapsed().as_millis() as u64;

    let changed_entity_ids: HashSet<&str> = diff.changes.iter().map(|c| c.entity_id.as_str()).collect();

    // Phase 3: Build entity graph from ALL source files (parallel via rayon)
    let graph_start = Instant::now();
    let graph = EntityGraph::build(git.repo_root(), &all_files, &registry);
    let graph_build_ms = graph_start.elapsed().as_millis() as u64;
    let total_graph_entities = graph.entities.len();

    // Phase 4: Score, classify, untangle
    let scoring_start = Instant::now();

    let mut reviews: Vec<EntityReview> = Vec::new();
    let mut dependency_edges: Vec<(String, String)> = Vec::new();

    for change in &diff.changes {
        let dependents = graph.get_dependents(&change.entity_id);
        let dependencies = graph.get_dependencies(&change.entity_id);
        // Use capped impact count to avoid full BFS on hub entities
        let blast_radius = graph.impact_count(&change.entity_id, 10_000);

        let classification = classify_change(change);
        let after_content = change.after_content.as_deref();
        let pub_api = is_public_api(&change.entity_type, &change.entity_name, after_content);

        let (start_line, end_line) = graph
            .entities
            .get(&change.entity_id)
            .map(|e| (e.start_line, e.end_line))
            .unwrap_or((0, 0));

        let mut review = EntityReview {
            entity_id: change.entity_id.clone(),
            entity_name: change.entity_name.clone(),
            entity_type: change.entity_type.clone(),
            file_path: change.file_path.clone(),
            change_type: change.change_type,
            classification,
            risk_score: 0.0,
            risk_level: RiskLevel::Low,
            blast_radius,
            dependent_count: dependents.len(),
            dependency_count: dependencies.len(),
            is_public_api: pub_api,
            structural_change: change.structural_change,
            group_id: 0,
            start_line,
            end_line,
        };

        review.risk_score = compute_risk_score(&review, total_graph_entities);
        review.risk_level = score_to_level(review.risk_score);

        for dep in &dependencies {
            if changed_entity_ids.contains(dep.id.as_str()) {
                dependency_edges.push((change.entity_id.clone(), dep.id.clone()));
            }
        }
        for dep in &dependents {
            if changed_entity_ids.contains(dep.id.as_str()) {
                dependency_edges.push((change.entity_id.clone(), dep.id.clone()));
            }
        }

        reviews.push(review);
    }

    reviews.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());

    let groups = untangle(&reviews, &dependency_edges);

    let entity_to_group: HashMap<&str, usize> = groups
        .iter()
        .flat_map(|g| g.entity_ids.iter().map(move |id| (id.as_str(), g.id)))
        .collect();

    for review in &mut reviews {
        if let Some(&gid) = entity_to_group.get(review.entity_id.as_str()) {
            review.group_id = gid;
        }
    }

    let scoring_ms = scoring_start.elapsed().as_millis() as u64;
    let total_ms = total_start.elapsed().as_millis() as u64;

    let stats = compute_stats(&reviews);

    let timing = Timing {
        diff_ms,
        list_files_ms,
        file_count,
        graph_build_ms,
        graph_entity_count: total_graph_entities,
        scoring_ms,
        total_ms,
    };

    Ok(ReviewResult {
        entity_reviews: reviews,
        groups,
        stats,
        timing,
        changes: diff.changes,
    })
}

fn compute_stats(reviews: &[EntityReview]) -> ReviewStats {
    let mut by_risk = RiskBreakdown {
        critical: 0,
        high: 0,
        medium: 0,
        low: 0,
    };
    let mut by_classification = ClassificationBreakdown {
        text: 0,
        syntax: 0,
        functional: 0,
        mixed: 0,
    };
    let mut by_change = ChangeTypeBreakdown {
        added: 0,
        modified: 0,
        deleted: 0,
        moved: 0,
        renamed: 0,
    };

    for r in reviews {
        match r.risk_level {
            RiskLevel::Critical => by_risk.critical += 1,
            RiskLevel::High => by_risk.high += 1,
            RiskLevel::Medium => by_risk.medium += 1,
            RiskLevel::Low => by_risk.low += 1,
        }
        match r.classification {
            ChangeClassification::Text => by_classification.text += 1,
            ChangeClassification::Syntax => by_classification.syntax += 1,
            ChangeClassification::Functional => by_classification.functional += 1,
            _ => by_classification.mixed += 1,
        }
        match r.change_type {
            ChangeType::Added => by_change.added += 1,
            ChangeType::Modified => by_change.modified += 1,
            ChangeType::Deleted => by_change.deleted += 1,
            ChangeType::Moved => by_change.moved += 1,
            ChangeType::Renamed => by_change.renamed += 1,
        }
    }

    ReviewStats {
        total_entities: reviews.len(),
        by_risk,
        by_classification: by_classification,
        by_change_type: by_change,
    }
}

/// List all tracked source files in the repo via `git ls-files`.
fn list_source_files(repo_path: &Path) -> Result<Vec<String>, AnalyzeError> {
    let output = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| AnalyzeError::Git(format!("failed to run git ls-files: {}", e)))?;

    if !output.status.success() {
        return Err(AnalyzeError::Git("git ls-files failed".into()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|f| {
            let f = f.to_lowercase();
            f.ends_with(".rs")
                || f.ends_with(".ts")
                || f.ends_with(".tsx")
                || f.ends_with(".js")
                || f.ends_with(".jsx")
                || f.ends_with(".py")
                || f.ends_with(".go")
                || f.ends_with(".java")
                || f.ends_with(".c")
                || f.ends_with(".cpp")
                || f.ends_with(".rb")
                || f.ends_with(".cs")
                || f.ends_with(".php")
        })
        .map(|s| s.to_string())
        .collect();

    Ok(files)
}

fn empty_result() -> ReviewResult {
    ReviewResult {
        entity_reviews: vec![],
        groups: vec![],
        stats: ReviewStats {
            total_entities: 0,
            by_risk: RiskBreakdown {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            by_classification: ClassificationBreakdown {
                text: 0,
                syntax: 0,
                functional: 0,
                mixed: 0,
            },
            by_change_type: ChangeTypeBreakdown {
                added: 0,
                modified: 0,
                deleted: 0,
                moved: 0,
                renamed: 0,
            },
        },
        timing: Timing::default(),
        changes: vec![],
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnalyzeError {
    #[error("git error: {0}")]
    Git(String),
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

    fn commit(dir: &Path, msg: &str) {
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", msg, "--allow-empty"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn analyze_added_function() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        // Initial commit with empty file
        std::fs::write(dir.join("main.rs"), "").unwrap();
        commit(dir, "init");

        // Add a function
        std::fs::write(dir.join("main.rs"), "fn hello() {\n    println!(\"hello\");\n}\n").unwrap();
        commit(dir, "add hello");

        let result = analyze(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
        )
        .unwrap();

        assert!(!result.entity_reviews.is_empty());
        let review = &result.entity_reviews[0];
        assert_eq!(review.change_type, ChangeType::Added);
        assert_eq!(review.classification, ChangeClassification::Functional);
    }

    #[test]
    fn analyze_empty_diff() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(dir.join("main.rs"), "fn hello() {}\n").unwrap();
        commit(dir, "init");

        // No changes
        let result = analyze(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
        );
        // This should either succeed with entities or succeed with empty
        // depending on whether the initial commit has a parent
        assert!(result.is_ok());
    }
}
