use std::collections::{HashMap, HashSet};
use std::path::Path;

use sem_core::git::bridge::GitBridge;
use sem_core::git::types::{DiffScope, FileChange, FileStatus};
use sem_core::model::change::ChangeType;
use sem_core::parser::differ::compute_semantic_diff;
use sem_core::parser::graph::EntityGraph;
use sem_core::parser::plugins::create_default_registry;

use crate::classify::classify_change;
use crate::detect::run_all_detectors;
use crate::github::FilePair;
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

    // Phase 3: Build entity graph (with disk cache for stable worktrees)
    let graph_start = Instant::now();
    let graph = load_or_build_graph(repo_path, git.repo_root(), &all_files, &registry);
    let graph_build_ms = graph_start.elapsed().as_millis() as u64;
    let total_graph_entities = graph.entities.len();

    // Phase 4: Score, classify, untangle
    let scoring_start = Instant::now();

    let mut reviews: Vec<EntityReview> = Vec::new();
    let mut dependency_edges: Vec<(String, String)> = Vec::new();

    // Skip expensive BFS impact_count when there are many changed entities
    let skip_bfs = diff.changes.len() > 500;

    for change in &diff.changes {
        let dependents = graph.get_dependents(&change.entity_id);
        let dependencies = graph.get_dependencies(&change.entity_id);
        let blast_radius = if skip_bfs {
            dependents.len()
        } else {
            graph.impact_count(&change.entity_id, 10_000)
        };

        let classification = classify_change(change);
        let after_content_ref = change.after_content.as_deref();
        let pub_api = is_public_api(&change.entity_type, &change.entity_name, after_content_ref);

        let (start_line, end_line) = graph
            .entities
            .get(&change.entity_id)
            .map(|e| (e.start_line, e.end_line))
            .unwrap_or((0, 0));

        let dependent_names: Vec<(String, String)> = dependents
            .iter()
            .map(|e| (e.name.clone(), e.file_path.clone()))
            .collect();
        let dependency_names: Vec<(String, String)> = dependencies
            .iter()
            .map(|e| (e.name.clone(), e.file_path.clone()))
            .collect();

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
            before_content: change.before_content.clone(),
            after_content: change.after_content.clone(),
            dependent_names,
            dependency_names,
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

    // File diversity: mildly demote non-top entities within the same file
    // This pushes the top-20 towards covering more distinct files,
    // reducing cases where many entities from one file crowd out bugs in other files
    {
        let mut top_per_file: HashMap<String, f64> = HashMap::new();
        for review in reviews.iter() {
            let entry = top_per_file
                .entry(review.file_path.clone())
                .or_insert(0.0_f64);
            if review.risk_score > *entry {
                *entry = review.risk_score;
            }
        }
        for review in reviews.iter_mut() {
            if let Some(&top_score) = top_per_file.get(&review.file_path) {
                if review.risk_score < top_score {
                    review.risk_score *= 0.85;
                    review.risk_level = score_to_level(review.risk_score);
                }
            }
        }
        reviews.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
    }

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

    // Group-aware score elevation: entities in the same untangle group
    // as a high-scoring entity get a small boost. This captures "guilt by
    // association" — if entity A is related to high-risk entity B, A
    // is more likely to be involved in the same bug.
    {
        let mut group_max: HashMap<usize, f64> = HashMap::new();
        for review in reviews.iter() {
            if review.group_id > 0 {
                let entry = group_max.entry(review.group_id).or_insert(0.0_f64);
                if review.risk_score > *entry {
                    *entry = review.risk_score;
                }
            }
        }
        for review in reviews.iter_mut() {
            if review.group_id > 0 {
                if let Some(&max_score) = group_max.get(&review.group_id) {
                    if review.risk_score < max_score && max_score > 0.3 {
                        let gap = max_score - review.risk_score;
                        review.risk_score += gap * 0.15;
                        review.risk_level = score_to_level(review.risk_score);
                    }
                }
            }
        }
        reviews.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
    }

    let scoring_ms = scoring_start.elapsed().as_millis() as u64;

    // Phase 5: Run deterministic detectors
    let findings = run_all_detectors(&reviews, &diff.changes, Some(&graph));

    // Phase 6: Boost entity scores based on detector findings
    // Entities with concrete suspicious patterns (negation flips, removed guards,
    // etc.) get a significant score bump. This is the primary mechanism for
    // differentiating entities that look risky from those that ARE risky.
    if !findings.is_empty() {
        use std::collections::HashMap as FindingsMap;
        let mut finding_boost: FindingsMap<&str, f64> = FindingsMap::new();
        for f in &findings {
            let severity_bonus = match f.severity {
                crate::detect::Severity::Critical => 0.15,
                crate::detect::Severity::High => 0.12,
                crate::detect::Severity::Medium => 0.07,
                crate::detect::Severity::Low => 0.04,
            };
            let boost = severity_bonus * f.confidence;
            let entry = finding_boost.entry(f.entity_id.as_str()).or_insert(0.0);
            *entry = (*entry + boost).min(0.35); // aggressive cap — findings are our best signal
        }
        for review in &mut reviews {
            if let Some(&boost) = finding_boost.get(review.entity_id.as_str()) {
                review.risk_score = (review.risk_score + boost).min(1.0);
                review.risk_level = score_to_level(review.risk_score);
            }
        }
    }

    // Phase 7: Penalize duplicate-name entities in the same file.
    // When multiple entities share the same (file, name) (e.g., 4 `updatedValue`
    // variables in MultiEmail.tsx), discount the duplicates so they don't all
    // occupy top-20 slots. Keep the highest-scoring one at full score.
    {
        use std::collections::HashMap;
        // Find groups of (file, name) duplicates — collect indices
        let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (i, r) in reviews.iter().enumerate() {
            groups.entry((r.file_path.clone(), r.entity_name.clone()))
                .or_default()
                .push(i);
        }
        // Collect indices to discount (drop groups reference before mutating)
        let mut to_discount: Vec<usize> = Vec::new();
        for (_, indices) in &groups {
            if indices.len() <= 1 {
                continue;
            }
            let mut sorted_indices = indices.clone();
            sorted_indices.sort_by(|a, b| {
                reviews[*b].risk_score.partial_cmp(&reviews[*a].risk_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            // Skip the first (highest score), discount the rest
            to_discount.extend_from_slice(&sorted_indices[1..]);
        }
        drop(groups);
        for idx in to_discount {
            reviews[idx].risk_score *= 0.5;
            reviews[idx].risk_level = score_to_level(reviews[idx].risk_score);
        }
    }

    // Phase 8: Per-file diversity constraint.
    // If too many entities from the same file cluster at the top, discount
    // the excess so entities from other files get a chance. This prevents
    // a single high-blast-radius file from monopolizing all top-20 slots.
    {
        use std::collections::HashMap;
        reviews.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
        let max_per_file = 1;
        let mut file_counts: HashMap<&str, usize> = HashMap::new();
        let mut to_discount: Vec<usize> = Vec::new();
        for (i, r) in reviews.iter().enumerate() {
            let count = file_counts.entry(&r.file_path).or_insert(0);
            *count += 1;
            if *count > max_per_file {
                to_discount.push(i);
            }
        }
        for idx in to_discount {
            reviews[idx].risk_score *= 0.15;
            reviews[idx].risk_level = score_to_level(reviews[idx].risk_score);
        }
    }

    reviews.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());

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
        findings,
        changes: diff.changes,
    })
}

/// Analyze file pairs fetched from a remote source (e.g. GitHub API).
/// No local git repo or graph needed. Gets entity-level granularity,
/// ConGra classification, public API detection, and risk scoring
/// (blast_radius and dependent_count will be 0 since no graph is available).
pub fn analyze_remote(file_pairs: &[FilePair]) -> Result<ReviewResult, AnalyzeError> {
    use std::time::Instant;

    let total_start = Instant::now();
    let registry = create_default_registry();

    let file_changes: Vec<FileChange> = file_pairs
        .iter()
        .map(|fp| {
            let status = match fp.status.as_str() {
                "added" => FileStatus::Added,
                "removed" => FileStatus::Deleted,
                "renamed" => FileStatus::Renamed,
                _ => FileStatus::Modified,
            };
            FileChange {
                file_path: fp.filename.clone(),
                status,
                old_file_path: None,
                before_content: fp.before_content.clone(),
                after_content: fp.after_content.clone(),
            }
        })
        .collect();

    if file_changes.is_empty() {
        return Ok(empty_result());
    }

    let diff_start = Instant::now();
    let diff = compute_semantic_diff(&file_changes, &registry, None, None);
    let diff_ms = diff_start.elapsed().as_millis() as u64;

    if diff.changes.is_empty() {
        return Ok(empty_result());
    }

    let scoring_start = Instant::now();

    let mut reviews: Vec<EntityReview> = Vec::new();

    for change in &diff.changes {
        let classification = classify_change(change);
        let after_content_ref = change.after_content.as_deref();
        let pub_api = is_public_api(&change.entity_type, &change.entity_name, after_content_ref);

        let mut review = EntityReview {
            entity_id: change.entity_id.clone(),
            entity_name: change.entity_name.clone(),
            entity_type: change.entity_type.clone(),
            file_path: change.file_path.clone(),
            change_type: change.change_type,
            classification,
            risk_score: 0.0,
            risk_level: RiskLevel::Low,
            blast_radius: 0,
            dependent_count: 0,
            dependency_count: 0,
            is_public_api: pub_api,
            structural_change: change.structural_change,
            group_id: 0,
            start_line: 0,
            end_line: 0,
            before_content: change.before_content.clone(),
            after_content: change.after_content.clone(),
            dependent_names: vec![],
            dependency_names: vec![],
        };

        review.risk_score = compute_risk_score(&review, 0);
        review.risk_level = score_to_level(review.risk_score);

        reviews.push(review);
    }

    reviews.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());

    let groups = untangle(&reviews, &[]);

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

    // Run deterministic detectors (no graph available for remote analysis)
    let findings = run_all_detectors(&reviews, &diff.changes, None);

    // Boost entity scores based on detector findings (same as local analysis)
    if !findings.is_empty() {
        use std::collections::HashMap as FindingsMap;
        let mut finding_boost: FindingsMap<&str, f64> = FindingsMap::new();
        for f in &findings {
            let severity_bonus = match f.severity {
                crate::detect::Severity::Critical => 0.15,
                crate::detect::Severity::High => 0.12,
                crate::detect::Severity::Medium => 0.07,
                crate::detect::Severity::Low => 0.04,
            };
            let boost = severity_bonus * f.confidence;
            let entry = finding_boost.entry(f.entity_id.as_str()).or_insert(0.0);
            *entry = (*entry + boost).min(0.25);
        }
        for review in &mut reviews {
            if let Some(&boost) = finding_boost.get(review.entity_id.as_str()) {
                review.risk_score = (review.risk_score + boost).min(1.0);
                review.risk_level = score_to_level(review.risk_score);
            }
        }
        reviews.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
    }

    let total_ms = total_start.elapsed().as_millis() as u64;

    let stats = compute_stats(&reviews);

    let timing = Timing {
        diff_ms,
        list_files_ms: 0,
        file_count: file_pairs.len(),
        graph_build_ms: 0,
        graph_entity_count: 0,
        scoring_ms,
        total_ms,
    };

    Ok(ReviewResult {
        entity_reviews: reviews,
        groups,
        stats,
        timing,
        findings,
        changes: diff.changes,
    })
}

pub(crate) fn compute_stats(reviews: &[EntityReview]) -> ReviewStats {
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

/// Load a cached EntityGraph from disk, or build and cache it.
/// Cache key: SHA-256 of the canonical repo path. Stored in /tmp/inspect-graph-cache/.
fn load_or_build_graph(
    repo_path: &Path,
    root: &Path,
    files: &[String],
    registry: &sem_core::parser::registry::ParserRegistry,
) -> EntityGraph {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let cache_dir = Path::new("/tmp/inspect-graph-cache");
    let _ = std::fs::create_dir_all(cache_dir);

    // Hash the canonical repo path as cache key
    let canonical = repo_path.canonicalize().unwrap_or_else(|_| repo_path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    let cache_file = cache_dir.join(format!("{:016x}.bin", hasher.finish()));

    // Try loading from cache
    if let Ok(data) = std::fs::read(&cache_file) {
        if let Ok(graph) = bincode::deserialize::<EntityGraph>(&data) {
            return graph;
        }
    }

    // Build fresh and cache
    let graph = EntityGraph::build(root, files, registry);
    if let Ok(data) = bincode::serialize(&graph) {
        let _ = std::fs::write(&cache_file, data);
    }
    graph
}

/// Source file extensions to include in analysis.
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "cpp", "rb", "cs", "php",
];

/// List all tracked source files in the repo using fff-search's FilePicker.
/// Falls back to `git ls-files` if the picker fails to initialize.
fn list_source_files(repo_path: &Path) -> Result<Vec<String>, AnalyzeError> {
    use fff_search::file_picker::FilePicker;
    use fff_search::{FFFMode, SharedPicker, SharedFrecency};

    let shared_picker: SharedPicker = Default::default();
    let shared_frecency: SharedFrecency = Default::default();

    match FilePicker::new_with_shared_state(
        repo_path.to_string_lossy().into_owned(),
        false,
        FFFMode::Ai,
        shared_picker.clone(),
        shared_frecency,
    ) {
        Ok(_) => {
            FilePicker::wait_for_scan(&shared_picker);

            let picker_guard = shared_picker.read().unwrap();
            let picker = picker_guard.as_ref().ok_or_else(|| {
                AnalyzeError::Git("FilePicker scan produced no results".into())
            })?;

            let files: Vec<String> = picker
                .get_files()
                .iter()
                .filter(|f| {
                    let ext = f.relative_path.rsplit('.').next().unwrap_or("");
                    SOURCE_EXTENSIONS.contains(&ext)
                })
                .filter(|f| !f.is_binary)
                .map(|f| f.relative_path.clone())
                .collect();

            Ok(files)
        }
        Err(_) => list_source_files_fallback(repo_path),
    }
}

/// Fallback: list source files via `git ls-files` if fff-search is unavailable.
fn list_source_files_fallback(repo_path: &Path) -> Result<Vec<String>, AnalyzeError> {
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
            let ext = f.rsplit('.').next().unwrap_or("");
            SOURCE_EXTENSIONS.contains(&ext)
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
        findings: vec![],
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
