use std::collections::{HashMap, HashSet};
use std::path::Path;

use git2::{ObjectType, Repository, Tree};
use sem_core::git::bridge::GitBridge;
use sem_core::git::types::{DiffScope, FileChange, FileStatus};
use sem_core::model::change::ChangeType;
use sem_core::parser::differ::compute_semantic_diff;
use sem_core::parser::graph::EntityGraph;
use sem_core::parser::plugins::create_default_registry;
use tempfile::TempDir;

use crate::classify::classify_change;
use crate::github::FilePair;
use crate::noise::is_noise_file;
use crate::risk::{compute_risk_score, is_public_api, rank_dependent, score_to_level};
use crate::types::*;
use crate::untangle::untangle;

/// Options for controlling analysis behavior.
pub struct AnalyzeOptions {
    /// Include full source code of dependent entities (callers/consumers).
    pub include_dependent_code: bool,
    /// Maximum number of dependents to include per changed entity.
    pub max_dependents_per_entity: usize,
    /// Skip dependent entities larger than this many lines.
    pub max_dependent_lines: usize,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            include_dependent_code: false,
            max_dependents_per_entity: 5,
            max_dependent_lines: 100,
        }
    }
}

/// Shared context from Phases 1-3: diff, file listing, graph build.
/// Used by both analyze and predict.
pub(crate) struct AnalysisContext {
    pub graph: EntityGraph,
    pub source_root: std::path::PathBuf,
    _source_tree: Option<TempDir>,
    pub changes: Vec<sem_core::model::change::SemanticChange>,
    pub changed_entity_ids: HashSet<String>,
    pub total_graph_entities: usize,
    pub diff_ms: u64,
    pub list_files_ms: u64,
    pub file_count: usize,
    pub graph_build_ms: u64,
}

/// Run Phases 1-3: entity diff, file listing, graph build.
/// Returns None if there are no changes.
pub(crate) fn build_context(
    repo_path: &Path,
    scope: DiffScope,
) -> Result<Option<AnalysisContext>, AnalyzeError> {
    use std::time::Instant;

    let git = GitBridge::open(repo_path).map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let registry = create_default_registry();

    let file_changes: Vec<FileChange> = git
        .get_changed_files(&scope, &[])
        .map_err(|e| AnalyzeError::Git(e.to_string()))?
        .into_iter()
        .filter(|change| !is_noise_file(&change.file_path))
        .collect();

    if file_changes.is_empty() {
        return Ok(None);
    }

    // Phase 1: Compute entity-level diff
    let diff_start = Instant::now();
    let diff = compute_semantic_diff(&file_changes, &registry, None, None);
    let diff_ms = diff_start.elapsed().as_millis() as u64;

    if diff.changes.is_empty() {
        return Ok(None);
    }

    // Phase 2: List all source files in the graph source tree
    let list_start = Instant::now();
    let graph_source = graph_source_for_scope(git.repo_root(), &scope)?;
    let all_files = graph_source.files;
    let file_count = all_files.len();
    let list_files_ms = list_start.elapsed().as_millis() as u64;

    let changed_entity_ids: HashSet<String> =
        diff.changes.iter().map(|c| c.entity_id.clone()).collect();

    // Phase 3: Build entity graph from ALL source files (parallel via rayon)
    let graph_start = Instant::now();
    let (graph, _all_entities) = EntityGraph::build(&graph_source.root, &all_files, &registry);
    let graph_build_ms = graph_start.elapsed().as_millis() as u64;
    let total_graph_entities = graph.entities.len();

    Ok(Some(AnalysisContext {
        graph,
        source_root: graph_source.root,
        _source_tree: graph_source.temp_dir,
        changes: diff.changes,
        changed_entity_ids,
        total_graph_entities,
        diff_ms,
        list_files_ms,
        file_count,
        graph_build_ms,
    }))
}

/// Analyze a diff scope and produce a ReviewResult.
pub fn analyze(repo_path: &Path, scope: DiffScope) -> Result<ReviewResult, AnalyzeError> {
    analyze_with_options(repo_path, scope, &AnalyzeOptions::default())
}

/// Analyze with configurable options (e.g. dependent entity code).
pub fn analyze_with_options(
    repo_path: &Path,
    scope: DiffScope,
    options: &AnalyzeOptions,
) -> Result<ReviewResult, AnalyzeError> {
    use std::time::Instant;

    let total_start = Instant::now();

    let ctx = match build_context(repo_path, scope)? {
        Some(ctx) => ctx,
        None => return Ok(empty_result()),
    };

    let AnalysisContext {
        graph,
        changes,
        changed_entity_ids,
        total_graph_entities,
        diff_ms,
        list_files_ms,
        file_count,
        graph_build_ms,
        source_root,
        _source_tree,
    } = ctx;

    // Phase 4: Score, classify, untangle
    let scoring_start = Instant::now();

    let mut reviews: Vec<EntityReview> = Vec::new();
    let mut dependency_edges: Vec<(String, String)> = Vec::new();

    for change in &changes {
        let dependents = graph.get_dependents(&change.entity_id);
        let dependencies = graph.get_dependencies(&change.entity_id);
        // Use capped impact count to avoid full BFS on hub entities
        let blast_radius = graph.impact_count(&change.entity_id, 10_000);

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
            dependent_entities: vec![],
        };

        review.risk_score = compute_risk_score(&review, total_graph_entities);
        review.risk_level = score_to_level(review.risk_score);

        for dep in &dependencies {
            if changed_entity_ids.contains(&dep.id) {
                dependency_edges.push((change.entity_id.clone(), dep.id.clone()));
            }
        }
        for dep in &dependents {
            if changed_entity_ids.contains(&dep.id) {
                dependency_edges.push((change.entity_id.clone(), dep.id.clone()));
            }
        }

        reviews.push(review);
    }

    // Phase 4b: Collect dependent entity code if requested
    if options.include_dependent_code {
        for review in &mut reviews {
            review.dependent_entities =
                collect_dependent_code(&graph, &review.entity_id, &source_root, options);
        }
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
        changes,
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
        .filter(|fp| !is_noise_file(&fp.filename))
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
            dependent_entities: vec![],
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
    let total_ms = total_start.elapsed().as_millis() as u64;

    let stats = compute_stats(&reviews);

    let timing = Timing {
        diff_ms,
        list_files_ms: 0,
        file_count: file_changes.len(),
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
            ChangeType::Reordered => by_change.modified += 1,
        }
    }

    ReviewStats {
        total_entities: reviews.len(),
        by_risk,
        by_classification: by_classification,
        by_change_type: by_change,
    }
}

/// Collect full source code of the top dependent entities for a changed entity.
/// Uses the entity graph to get precise function boundaries via tree-sitter.
fn collect_dependent_code(
    graph: &EntityGraph,
    entity_id: &str,
    source_root: &Path,
    options: &AnalyzeOptions,
) -> Vec<DependentEntity> {
    let dependents = graph.get_dependents(entity_id);
    if dependents.is_empty() {
        return vec![];
    }

    let source_file = graph
        .entities
        .get(entity_id)
        .map(|e| e.file_path.as_str())
        .unwrap_or("");

    // Score and rank dependents
    let mut scored: Vec<(&sem_core::parser::graph::EntityInfo, f64)> = dependents
        .iter()
        .map(|dep| {
            let own_dep_count = graph.get_dependents(&dep.id).len();
            let content_hint = std::fs::read_to_string(source_root.join(&dep.file_path))
                .ok()
                .and_then(|c| {
                    let lines: Vec<&str> = c.lines().collect();
                    lines
                        .get(dep.start_line.saturating_sub(1))
                        .map(|l| l.to_string())
                });
            let is_pub = is_public_api(&dep.entity_type, &dep.name, content_hint.as_deref());
            let is_cross_file = dep.file_path != source_file;
            let score = rank_dependent(own_dep_count, is_pub, is_cross_file);
            (*dep, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored.truncate(options.max_dependents_per_entity);

    scored
        .into_iter()
        .filter_map(|(dep, _score)| {
            let line_count = dep.end_line.saturating_sub(dep.start_line) + 1;
            if line_count > options.max_dependent_lines {
                return None;
            }

            let file_content = std::fs::read_to_string(source_root.join(&dep.file_path)).ok()?;
            let lines: Vec<&str> = file_content.lines().collect();
            let start = dep.start_line.saturating_sub(1);
            let end = dep.end_line.min(lines.len());
            if start >= lines.len() || start >= end {
                return None;
            }
            let content = lines[start..end].join("\n");

            let own_dep_count = graph.get_dependents(&dep.id).len();
            let first_line = lines.get(start).copied().unwrap_or("");
            let is_pub = is_public_api(&dep.entity_type, &dep.name, Some(first_line));

            Some(DependentEntity {
                entity_name: dep.name.clone(),
                entity_type: dep.entity_type.clone(),
                file_path: dep.file_path.clone(),
                start_line: dep.start_line,
                end_line: dep.end_line,
                content,
                own_dependent_count: own_dep_count,
                is_public_api: is_pub,
            })
        })
        .collect()
}

struct GraphSource {
    root: std::path::PathBuf,
    files: Vec<String>,
    temp_dir: Option<TempDir>,
}

fn graph_source_for_scope(
    repo_root: &Path,
    scope: &DiffScope,
) -> Result<GraphSource, AnalyzeError> {
    match scope {
        DiffScope::Commit { sha } => materialize_tree_source(repo_root, sha),
        DiffScope::Range { to, .. } => materialize_tree_source(repo_root, to),
        DiffScope::Staged => materialize_index_source(repo_root),
        DiffScope::Working | DiffScope::RefToWorking { .. } => {
            let files = list_source_files(repo_root)?;
            Ok(GraphSource {
                root: repo_root.to_path_buf(),
                files,
                temp_dir: None,
            })
        }
    }
}

fn materialize_index_source(repo_root: &Path) -> Result<GraphSource, AnalyzeError> {
    let repo = Repository::discover(repo_root).map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let index = repo.index().map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let temp_dir = TempDir::new().map_err(|e| AnalyzeError::Io(e.to_string()))?;
    let mut files = Vec::new();

    for entry in index.iter() {
        let Ok(path) = std::str::from_utf8(&entry.path) else {
            continue;
        };
        if !is_source_file(path) {
            continue;
        }

        let blob = repo
            .find_blob(entry.id)
            .map_err(|e| AnalyzeError::Git(e.to_string()))?;
        write_source_blob(temp_dir.path(), path, blob.content(), &mut files)?;
    }

    Ok(GraphSource {
        root: temp_dir.path().to_path_buf(),
        files,
        temp_dir: Some(temp_dir),
    })
}

fn materialize_tree_source(repo_root: &Path, refspec: &str) -> Result<GraphSource, AnalyzeError> {
    let repo = Repository::discover(repo_root).map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let object = repo
        .revparse_single(refspec)
        .map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let tree = object
        .peel_to_tree()
        .map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let temp_dir = TempDir::new().map_err(|e| AnalyzeError::Io(e.to_string()))?;
    let mut files = Vec::new();

    materialize_source_files(&repo, &tree, "", temp_dir.path(), &mut files)?;

    Ok(GraphSource {
        root: temp_dir.path().to_path_buf(),
        files,
        temp_dir: Some(temp_dir),
    })
}

fn materialize_source_files(
    repo: &Repository,
    tree: &Tree<'_>,
    prefix: &str,
    target_root: &Path,
    files: &mut Vec<String>,
) -> Result<(), AnalyzeError> {
    for entry in tree.iter() {
        let name = entry
            .name()
            .ok_or_else(|| AnalyzeError::Git("tree entry is not valid UTF-8".into()))?;
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };

        match entry.kind() {
            Some(ObjectType::Tree) => {
                let subtree = repo
                    .find_tree(entry.id())
                    .map_err(|e| AnalyzeError::Git(e.to_string()))?;
                materialize_source_files(repo, &subtree, &path, target_root, files)?;
            }
            Some(ObjectType::Blob) if is_source_file(&path) => {
                let blob = repo
                    .find_blob(entry.id())
                    .map_err(|e| AnalyzeError::Git(e.to_string()))?;
                write_source_blob(target_root, &path, blob.content(), files)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn write_source_blob(
    target_root: &Path,
    path: &str,
    content: &[u8],
    files: &mut Vec<String>,
) -> Result<(), AnalyzeError> {
    let target_path = target_root.join(path);
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AnalyzeError::Io(e.to_string()))?;
    }
    std::fs::write(&target_path, content).map_err(|e| AnalyzeError::Io(e.to_string()))?;
    files.push(path.to_string());
    Ok(())
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
        .filter(|f| !is_noise_file(f))
        .filter(|f| is_source_file(f))
        .map(|s| s.to_string())
        .collect();

    Ok(files)
}

fn is_source_file(path: &str) -> bool {
    let path = path.to_lowercase();
    path.ends_with(".rs")
        || path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".py")
        || path.ends_with(".go")
        || path.ends_with(".java")
        || path.ends_with(".c")
        || path.ends_with(".cpp")
        || path.ends_with(".rb")
        || path.ends_with(".cs")
        || path.ends_with(".php")
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
    #[error("io error: {0}")]
    Io(String),
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
    fn analyze_added_function() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        // Initial commit with empty file
        std::fs::write(dir.join("main.rs"), "").unwrap();
        commit(dir, "init");

        // Add a function
        std::fs::write(
            dir.join("main.rs"),
            "fn hello() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
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
    fn analyze_commit_uses_commit_tree_for_graph_metadata() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(dir.join("README.md"), "init\n").unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("service.py"),
            concat!(
                "def helper():\n",
                "    return 'ok'\n\n",
                "def caller():\n",
                "    return helper()\n",
            ),
        )
        .unwrap();
        let add_sha = commit(dir, "add service");

        let rm = Command::new("git")
            .args(["rm", "-q", "service.py"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(rm.status.success(), "git rm failed");
        commit(dir, "remove service");

        let result = analyze(dir, DiffScope::Commit { sha: add_sha }).unwrap();

        assert!(result.timing.file_count > 0);
        assert!(result.timing.graph_entity_count >= 2);

        let helper = result
            .entity_reviews
            .iter()
            .find(|r| r.entity_name == "helper")
            .expect("helper should be reviewed");
        assert!(helper.start_line > 0);
        assert!(helper.end_line >= helper.start_line);
        assert!(helper.dependent_count > 0);

        let caller = result
            .entity_reviews
            .iter()
            .find(|r| r.entity_name == "caller")
            .expect("caller should be reviewed");
        assert!(caller.start_line > 0);
        assert!(caller.dependency_count > 0);
    }

    #[test]
    fn analyze_range_uses_to_tree_for_graph_metadata() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(dir.join("README.md"), "init\n").unwrap();
        let init_sha = commit(dir, "init");

        std::fs::write(
            dir.join("service.py"),
            concat!(
                "def helper():\n",
                "    return 'ok'\n\n",
                "def caller():\n",
                "    return helper()\n",
            ),
        )
        .unwrap();
        let add_sha = commit(dir, "add service");

        let rm = Command::new("git")
            .args(["rm", "-q", "service.py"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(rm.status.success(), "git rm failed");
        commit(dir, "remove service");

        let result = analyze(
            dir,
            DiffScope::Range {
                from: init_sha,
                to: add_sha,
            },
        )
        .unwrap();

        let helper = result
            .entity_reviews
            .iter()
            .find(|r| r.entity_name == "helper")
            .expect("helper should be reviewed");
        assert!(helper.start_line > 0);
        assert!(helper.dependent_count > 0);
    }

    #[test]
    fn analyze_staged_uses_index_tree_for_graph_metadata() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(
            dir.join("service.py"),
            concat!(
                "def helper():\n",
                "    return 'old'\n\n",
                "def caller():\n",
                "    return helper()\n",
            ),
        )
        .unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("service.py"),
            concat!(
                "def helper(prefix='ok'):\n",
                "    return prefix\n\n",
                "def caller():\n",
                "    return helper()\n",
            ),
        )
        .unwrap();
        let add = Command::new("git")
            .args(["add", "service.py"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(add.status.success(), "git add failed");

        std::fs::write(
            dir.join("service.py"),
            "def helper(prefix='ok'):\n    return prefix\n",
        )
        .unwrap();

        let result = analyze(dir, DiffScope::Staged).unwrap();
        let helper = result
            .entity_reviews
            .iter()
            .find(|r| r.entity_name == "helper")
            .expect("helper should be reviewed");

        assert!(helper.start_line > 0);
        assert_eq!(helper.dependent_count, 1);
    }

    #[test]
    fn analyze_with_dependents_reads_dependent_code_from_commit_tree() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(
            dir.join("service.py"),
            concat!(
                "def helper():\n",
                "    return 'old'\n\n",
                "def caller():\n",
                "    return helper()\n",
            ),
        )
        .unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("service.py"),
            concat!(
                "def helper(prefix='ok'):\n",
                "    return prefix\n\n",
                "def caller():\n",
                "    return helper()\n",
            ),
        )
        .unwrap();
        let change_sha = commit(dir, "change helper");

        let rm = Command::new("git")
            .args(["rm", "-q", "service.py"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(rm.status.success(), "git rm failed");
        commit(dir, "remove service");

        let result = analyze_with_options(
            dir,
            DiffScope::Commit { sha: change_sha },
            &AnalyzeOptions {
                include_dependent_code: true,
                ..AnalyzeOptions::default()
            },
        )
        .unwrap();

        let helper = result
            .entity_reviews
            .iter()
            .find(|r| r.entity_name == "helper")
            .expect("helper should be reviewed");

        assert!(helper.dependent_entities.iter().any(|entity| {
            entity.entity_name == "caller" && entity.content.contains("return helper()")
        }));
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

    #[test]
    fn analyze_filters_noise_files_before_diffing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(
            dir.join("main.rs"),
            "fn hello() {\n    println!(\"one\");\n}\n",
        )
        .unwrap();
        std::fs::write(dir.join("go.sum"), "module.example/pkg v0.0.1 h1:aaaa=\n").unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("main.rs"),
            "fn hello() {\n    println!(\"two\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("go.sum"),
            "module.example/pkg v0.0.1 h1:bbbb=\nmodule.example/pkg v0.0.2 h1:bbbb=\n",
        )
        .unwrap();
        commit(dir, "change");

        let result = analyze(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
        )
        .unwrap();

        assert!(!result.entity_reviews.is_empty());
        assert!(result
            .entity_reviews
            .iter()
            .all(|review| review.file_path == "main.rs"));
        assert_eq!(result.stats.total_entities, result.entity_reviews.len());
    }

    #[test]
    fn analyze_returns_empty_when_only_noise_files_change() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(dir.join("go.sum"), "module.example/pkg v0.0.1 h1:aaaa=\n").unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("go.sum"),
            "module.example/pkg v0.0.1 h1:bbbb=\nmodule.example/pkg v0.0.2 h1:bbbb=\n",
        )
        .unwrap();
        commit(dir, "change");

        let result = analyze(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
        )
        .unwrap();

        assert!(result.entity_reviews.is_empty());
        assert_eq!(result.stats.total_entities, 0);
    }

    #[test]
    fn analyze_remote_counts_filtered_files_in_timing() {
        let result = analyze_remote(&[
            FilePair {
                filename: "go.sum".to_string(),
                status: "modified".to_string(),
                before_content: Some("module.example/pkg v0.0.1 h1:aaaa=\n".to_string()),
                after_content: Some("module.example/pkg v0.0.1 h1:bbbb=\n".to_string()),
            },
            FilePair {
                filename: "main.rs".to_string(),
                status: "modified".to_string(),
                before_content: Some("fn hello() {\n    println!(\"one\");\n}\n".to_string()),
                after_content: Some("fn hello() {\n    println!(\"two\");\n}\n".to_string()),
            },
        ])
        .unwrap();

        assert_eq!(result.timing.file_count, 1);
        assert!(!result.entity_reviews.is_empty());
        assert!(result
            .entity_reviews
            .iter()
            .all(|review| review.file_path == "main.rs"));
    }
}
