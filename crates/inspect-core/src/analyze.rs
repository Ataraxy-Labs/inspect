use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sem_core::git::bridge::GitBridge;
use sem_core::git::types::{DiffScope, FileChange, FileStatus};
use sem_core::model::change::ChangeType;
use sem_core::parser::differ::compute_semantic_diff;
use sem_core::parser::graph::{EntityGraph, EntityInfo};
use sem_core::parser::plugins::create_default_registry;
use sem_core::parser::registry::ParserRegistry;

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
    /// Include full source code of dependency entities (callees/helpers).
    pub include_dependency_code: bool,
    /// Include a small source window around each changed entity.
    pub include_file_context: bool,
    /// Maximum number of dependents to include per changed entity.
    pub max_dependents_per_entity: usize,
    /// Maximum number of dependencies to include per changed entity.
    pub max_dependencies_per_entity: usize,
    /// Skip dependent entities larger than this many lines.
    pub max_dependent_lines: usize,
    /// Skip dependency entities larger than this many lines.
    pub max_dependency_lines: usize,
    /// Number of lines to include before and after the changed entity.
    pub file_context_lines: usize,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            include_dependent_code: false,
            include_dependency_code: false,
            include_file_context: false,
            max_dependents_per_entity: 5,
            max_dependencies_per_entity: 5,
            max_dependent_lines: 100,
            max_dependency_lines: 100,
            file_context_lines: 8,
        }
    }
}

/// Shared context from Phases 1-3: diff, file listing, graph build.
/// Used by both analyze and predict.
pub(crate) struct AnalysisContext {
    pub graph: EntityGraph,
    pub before_graph: Option<EntityGraph>,
    pub changes: Vec<sem_core::model::change::SemanticChange>,
    pub file_changes: HashMap<String, FileChange>,
    pub changed_entity_ids: HashSet<String>,
    pub after_source: ContentSource,
    pub before_source: Option<ContentSource>,
    pub total_graph_entities: usize,
    pub diff_ms: u64,
    pub list_files_ms: u64,
    pub file_count: usize,
    pub graph_build_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) enum ContentSource {
    Worktree,
    Index,
    GitRef(String),
}

/// Run Phases 1-3: entity diff, file listing, graph build.
/// Returns None if there are no changes.
pub(crate) fn build_context(
    repo_path: &Path,
    scope: DiffScope,
    include_before_graph: bool,
) -> Result<Option<AnalysisContext>, AnalyzeError> {
    use std::time::Instant;

    let git = GitBridge::open(repo_path).map_err(|e| AnalyzeError::Git(e.to_string()))?;
    let registry = create_default_registry();
    let after_source = after_source_for_scope(&scope);
    let before_source = before_source_for_scope(&scope);

    let file_changes: Vec<FileChange> = git
        .get_changed_files(&scope, &[])
        .map_err(|e| AnalyzeError::Git(e.to_string()))?
        .into_iter()
        .filter(|change| !is_noise_file(&change.file_path))
        .collect();

    if file_changes.is_empty() {
        return Ok(None);
    }

    let file_change_map: HashMap<String, FileChange> = file_changes
        .iter()
        .map(|f| (f.file_path.clone(), f.clone()))
        .collect();

    // Phase 1: Compute entity-level diff
    let diff_start = Instant::now();
    let diff = compute_semantic_diff(&file_changes, &registry, None, None);
    let diff_ms = diff_start.elapsed().as_millis() as u64;

    if diff.changes.is_empty() {
        return Ok(None);
    }

    // Phase 2: List all source files in the reviewed after-side tree
    let list_start = Instant::now();
    let all_files = list_source_files_from_source(repo_path, &after_source)?;
    let file_count = all_files.len();
    let list_files_ms = list_start.elapsed().as_millis() as u64;

    let changed_entity_ids: HashSet<String> =
        diff.changes.iter().map(|c| c.entity_id.clone()).collect();

    // Phase 3: Build entity graph from ALL source files (parallel via rayon)
    let graph_start = Instant::now();
    let (graph, _all_entities) = build_graph_for_source(
        repo_path,
        git.repo_root(),
        &after_source,
        &all_files,
        &registry,
    )?;
    let graph_build_ms = graph_start.elapsed().as_millis() as u64;
    let total_graph_entities = graph.entities.len();

    let before_graph = if include_before_graph {
        before_source.as_ref().and_then(|source| {
            let files = list_source_files_from_source(repo_path, source).ok()?;
            build_graph_for_source(repo_path, git.repo_root(), source, &files, &registry)
                .ok()
                .map(|(graph, _)| graph)
        })
    } else {
        None
    };

    Ok(Some(AnalysisContext {
        graph,
        before_graph,
        changes: diff.changes,
        file_changes: file_change_map,
        changed_entity_ids,
        after_source,
        before_source,
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

    let ctx = match build_context(repo_path, scope, options.include_dependency_code)? {
        Some(ctx) => ctx,
        None => return Ok(empty_result()),
    };

    let AnalysisContext {
        graph,
        before_graph,
        changes,
        file_changes,
        changed_entity_ids,
        after_source,
        before_source,
        total_graph_entities,
        diff_ms,
        list_files_ms,
        file_count,
        graph_build_ms,
    } = ctx;

    // Phase 4: Score, classify, untangle
    let scoring_start = Instant::now();

    let mut reviews: Vec<EntityReview> = Vec::new();
    let mut dependency_edges: Vec<(String, String)> = Vec::new();
    let mut before_entity_ids: HashMap<String, String> = HashMap::new();

    for change in &changes {
        let before_entity_id = before_graph
            .as_ref()
            .and_then(|g| resolve_before_entity_id(change, g));
        if let Some(ref before_entity_id) = before_entity_id {
            before_entity_ids.insert(change.entity_id.clone(), before_entity_id.clone());
        }

        let dependents = graph.get_dependents(&change.entity_id);
        let dependencies = graph.get_dependencies(&change.entity_id);
        let before_dependents = before_graph
            .as_ref()
            .zip(before_entity_id.as_deref())
            .map(|(g, id)| g.get_dependents(id))
            .unwrap_or_default();
        let before_dependencies = before_graph
            .as_ref()
            .zip(before_entity_id.as_deref())
            .map(|(g, id)| g.get_dependencies(id))
            .unwrap_or_default();
        // Use capped impact count to avoid full BFS on hub entities
        let after_blast_radius = graph.impact_count(&change.entity_id, 10_000);
        let before_blast_radius = before_graph
            .as_ref()
            .zip(before_entity_id.as_deref())
            .map(|(g, id)| g.impact_count(id, 10_000))
            .unwrap_or(0);
        let blast_radius = after_blast_radius.max(before_blast_radius);

        let classification = classify_change(change);
        let after_content_ref = change.after_content.as_deref();
        let pub_api = is_public_api(&change.entity_type, &change.entity_name, after_content_ref);

        let after_span = graph
            .entities
            .get(&change.entity_id)
            .map(|e| (e.start_line, e.end_line));
        let before_span = before_graph
            .as_ref()
            .and_then(|g| {
                before_entity_id
                    .as_deref()
                    .and_then(|id| g.entities.get(id))
            })
            .map(|e| (e.start_line, e.end_line));
        let (start_line, end_line) = after_span.or(before_span).unwrap_or((0, 0));

        let dependent_names = related_names(&dependents, &before_dependents);
        let dependency_names = related_names(&dependencies, &before_dependencies);
        let file_change = file_changes.get(&change.file_path);

        let (before_start, before_end) = context_span(
            before_span.map(|span| span.0).unwrap_or(0),
            before_span.map(|span| span.1).unwrap_or(0),
            change.entity_line,
            change.before_content.as_deref(),
        );
        let (after_start, after_end) = context_span(
            after_span.map(|span| span.0).unwrap_or(0),
            after_span.map(|span| span.1).unwrap_or(0),
            change.entity_line,
            change.after_content.as_deref(),
        );

        let before_file_context = if options.include_file_context {
            file_change.and_then(|f| {
                let path = f.old_file_path.as_deref().unwrap_or(&change.file_path);
                extract_source_context(
                    f.before_content.as_deref(),
                    path,
                    before_start,
                    before_end,
                    options.file_context_lines,
                )
            })
        } else {
            None
        };

        let after_file_context = if options.include_file_context {
            file_change.and_then(|f| {
                extract_source_context(
                    f.after_content.as_deref(),
                    &change.file_path,
                    after_start,
                    after_end,
                    options.file_context_lines,
                )
            })
        } else {
            None
        };

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
            dependent_count: dependent_names.len(),
            dependency_count: dependency_names.len(),
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
            dependency_entities: vec![],
            before_file_context,
            after_file_context,
        };

        review.risk_score = compute_risk_score(&review, total_graph_entities);
        review.risk_level = score_to_level(review.risk_score);

        for dep in dependencies.iter().chain(before_dependencies.iter()) {
            if changed_entity_ids.contains(&dep.id) {
                dependency_edges.push((change.entity_id.clone(), dep.id.clone()));
            }
        }
        for dep in dependents.iter().chain(before_dependents.iter()) {
            if changed_entity_ids.contains(&dep.id) {
                dependency_edges.push((change.entity_id.clone(), dep.id.clone()));
            }
        }

        reviews.push(review);
    }

    // Phase 4b: Collect dependent entity code if requested
    if options.include_dependent_code {
        for review in &mut reviews {
            review.dependent_entities = collect_dependent_code(
                &graph,
                before_graph.as_ref(),
                before_entity_ids.get(&review.entity_id).map(String::as_str),
                &review.entity_id,
                repo_path,
                &after_source,
                before_source.as_ref(),
                options,
            );
        }
    }

    if options.include_dependency_code {
        for review in &mut reviews {
            review.dependency_entities = collect_dependency_code(
                &graph,
                before_graph.as_ref(),
                before_entity_ids.get(&review.entity_id).map(String::as_str),
                &review.entity_id,
                repo_path,
                &after_source,
                before_source.as_ref(),
                options,
            );
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
        dependency_edges,
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
            dependency_entities: vec![],
            before_file_context: None,
            after_file_context: None,
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
        dependency_edges: vec![],
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

/// Retain entity reviews and keep derived result summaries in sync.
pub fn retain_entity_reviews<F>(result: &mut ReviewResult, mut keep: F)
where
    F: FnMut(&EntityReview) -> bool,
{
    result.entity_reviews.retain(|review| keep(review));
    refresh_result_summaries(result);
}

fn refresh_result_summaries(result: &mut ReviewResult) {
    let remaining_ids: HashSet<String> = result
        .entity_reviews
        .iter()
        .map(|review| review.entity_id.clone())
        .collect();

    let dependency_edges: Vec<(String, String)> = result
        .dependency_edges
        .iter()
        .filter(|(from, to)| remaining_ids.contains(from) && remaining_ids.contains(to))
        .cloned()
        .collect();

    let groups = untangle(&result.entity_reviews, &dependency_edges);
    let entity_to_group: HashMap<&str, usize> = groups
        .iter()
        .flat_map(|g| g.entity_ids.iter().map(move |id| (id.as_str(), g.id)))
        .collect();

    for review in &mut result.entity_reviews {
        if let Some(&gid) = entity_to_group.get(review.entity_id.as_str()) {
            review.group_id = gid;
        }
    }

    result.groups = groups;
    result.stats = compute_stats(&result.entity_reviews);
    result.dependency_edges = dependency_edges;
}

/// Collect full source code of the top dependent entities for a changed entity.
/// Uses the entity graph to get precise function boundaries via tree-sitter.
fn collect_dependent_code(
    graph: &EntityGraph,
    before_graph: Option<&EntityGraph>,
    before_entity_id: Option<&str>,
    entity_id: &str,
    repo_path: &Path,
    after_source: &ContentSource,
    before_source: Option<&ContentSource>,
    options: &AnalyzeOptions,
) -> Vec<DependentEntity> {
    let mut related = collect_related_entity_code(
        graph,
        entity_id,
        graph.get_dependents(entity_id),
        repo_path,
        after_source,
        "after",
        options.max_dependents_per_entity,
        options.max_dependent_lines,
    );

    if let (Some(before_graph), Some(before_source)) = (before_graph, before_source) {
        let before_entity_id = before_entity_id.unwrap_or(entity_id);
        related.extend(collect_related_entity_code(
            before_graph,
            before_entity_id,
            before_graph.get_dependents(before_entity_id),
            repo_path,
            before_source,
            "before",
            options.max_dependents_per_entity,
            options.max_dependent_lines,
        ));
    }

    merge_related_entities(related)
}

/// Collect full source code of the top dependency entities for a changed entity.
/// Dependencies are direct callees/helpers referenced by the changed entity.
fn collect_dependency_code(
    graph: &EntityGraph,
    before_graph: Option<&EntityGraph>,
    before_entity_id: Option<&str>,
    entity_id: &str,
    repo_path: &Path,
    after_source: &ContentSource,
    before_source: Option<&ContentSource>,
    options: &AnalyzeOptions,
) -> Vec<DependentEntity> {
    let mut related = collect_related_entity_code(
        graph,
        entity_id,
        graph.get_dependencies(entity_id),
        repo_path,
        after_source,
        "after",
        options.max_dependencies_per_entity,
        options.max_dependency_lines,
    );

    if let (Some(before_graph), Some(before_source)) = (before_graph, before_source) {
        let before_entity_id = before_entity_id.unwrap_or(entity_id);
        related.extend(collect_related_entity_code(
            before_graph,
            before_entity_id,
            before_graph.get_dependencies(before_entity_id),
            repo_path,
            before_source,
            "before",
            options.max_dependencies_per_entity,
            options.max_dependency_lines,
        ));
    }

    merge_related_entities(related)
}

fn collect_related_entity_code(
    graph: &EntityGraph,
    entity_id: &str,
    related_entities: Vec<&EntityInfo>,
    repo_path: &Path,
    source: &ContentSource,
    relation: &str,
    max_entities: usize,
    max_lines: usize,
) -> Vec<DependentEntity> {
    if related_entities.is_empty() {
        return vec![];
    }

    let source_file = graph
        .entities
        .get(entity_id)
        .map(|e| e.file_path.as_str())
        .unwrap_or("");

    let mut scored: Vec<(&EntityInfo, f64)> = related_entities
        .into_iter()
        .map(|dep| {
            let own_dep_count = graph.get_dependents(&dep.id).len();
            let content_hint =
                read_file_from_source(repo_path, source, &dep.file_path).and_then(|c| {
                    let lines: Vec<&str> = c.lines().collect();
                    lines
                        .get(dep.start_line.saturating_sub(1))
                        .map(|l| l.to_string())
                });
            let is_pub = is_public_api(&dep.entity_type, &dep.name, content_hint.as_deref());
            let is_cross_file = dep.file_path != source_file;
            let score = rank_dependent(own_dep_count, is_pub, is_cross_file);
            (dep, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored.truncate(max_entities);

    scored
        .into_iter()
        .filter_map(|(dep, _score)| {
            let line_count = dep.end_line.saturating_sub(dep.start_line) + 1;
            if line_count > max_lines {
                return None;
            }

            let file_content = read_file_from_source(repo_path, source, &dep.file_path)?;
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
                relation: Some(relation.to_string()),
            })
        })
        .collect()
}

fn merge_related_entities(entities: Vec<DependentEntity>) -> Vec<DependentEntity> {
    let mut merged: Vec<DependentEntity> = Vec::new();

    'outer: for entity in entities {
        for existing in &mut merged {
            if existing.entity_name == entity.entity_name
                && existing.entity_type == entity.entity_type
                && existing.file_path == entity.file_path
                && existing.content == entity.content
            {
                if existing.relation != entity.relation {
                    existing.relation = Some("before_after".to_string());
                }
                continue 'outer;
            }
        }
        merged.push(entity);
    }

    merged
}

fn related_names(
    after_entities: &[&EntityInfo],
    before_entities: &[&EntityInfo],
) -> Vec<(String, String)> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();

    for entity in after_entities.iter().chain(before_entities.iter()) {
        let key = (entity.name.clone(), entity.file_path.clone());
        if seen.insert(key.clone()) {
            names.push(key);
        }
    }

    names
}

fn resolve_before_entity_id(
    change: &sem_core::model::change::SemanticChange,
    before_graph: &EntityGraph,
) -> Option<String> {
    if before_graph.entities.contains_key(&change.entity_id) {
        return Some(change.entity_id.clone());
    }

    let before_name = change
        .old_entity_name
        .as_deref()
        .unwrap_or(&change.entity_name);
    let before_file = change.old_file_path.as_deref().unwrap_or(&change.file_path);

    before_graph
        .entities
        .values()
        .find(|entity| {
            entity.name == before_name
                && entity.file_path == before_file
                && entity.entity_type == change.entity_type
        })
        .map(|entity| entity.id.clone())
}

fn context_span(
    start_line: usize,
    end_line: usize,
    fallback_line: usize,
    entity_content: Option<&str>,
) -> (usize, usize) {
    let start = if start_line > 0 {
        start_line
    } else if fallback_line > 0 {
        fallback_line
    } else {
        1
    };

    let content_lines = entity_content
        .map(|c| c.lines().count().max(1))
        .unwrap_or(1);
    let end = if end_line >= start {
        end_line
    } else {
        start + content_lines - 1
    };

    (start, end)
}

fn extract_source_context(
    file_content: Option<&str>,
    file_path: &str,
    entity_start_line: usize,
    entity_end_line: usize,
    context_lines: usize,
) -> Option<SourceContext> {
    let lines: Vec<&str> = file_content?.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let anchor_start = entity_start_line.max(1).min(lines.len());
    let anchor_end = entity_end_line.max(anchor_start).min(lines.len());
    let window_start = anchor_start.saturating_sub(context_lines).max(1);
    let window_end = (anchor_end + context_lines).min(lines.len());

    let content = (window_start..=window_end)
        .map(|line_no| format!("{:>4}: {}", line_no, lines[line_no - 1]))
        .collect::<Vec<_>>()
        .join("\n");

    Some(SourceContext {
        file_path: file_path.to_string(),
        start_line: window_start,
        end_line: window_end,
        content,
    })
}

fn after_source_for_scope(scope: &DiffScope) -> ContentSource {
    match scope {
        DiffScope::Commit { sha } => ContentSource::GitRef(sha.clone()),
        DiffScope::Range { to, .. } => ContentSource::GitRef(to.clone()),
        DiffScope::Staged => ContentSource::Index,
        DiffScope::Working | DiffScope::RefToWorking { .. } => ContentSource::Worktree,
    }
}

fn before_source_for_scope(scope: &DiffScope) -> Option<ContentSource> {
    match scope {
        DiffScope::Commit { sha } => Some(ContentSource::GitRef(format!("{sha}~1"))),
        DiffScope::Range { from, .. } => Some(ContentSource::GitRef(from.clone())),
        DiffScope::RefToWorking { refspec } => Some(ContentSource::GitRef(refspec.clone())),
        DiffScope::Working | DiffScope::Staged => Some(ContentSource::GitRef("HEAD".to_string())),
    }
}

fn build_graph_for_source(
    repo_path: &Path,
    worktree_root: &Path,
    source: &ContentSource,
    files: &[String],
    registry: &ParserRegistry,
) -> Result<(EntityGraph, usize), AnalyzeError> {
    match source {
        ContentSource::Worktree => {
            let (graph, entities) = EntityGraph::build(worktree_root, files, registry);
            Ok((graph, entities.len()))
        }
        ContentSource::Index | ContentSource::GitRef(_) => {
            let snapshot = materialize_source_tree(repo_path, source, files)?;
            let (graph, entities) = EntityGraph::build(&snapshot, files, registry);
            let _ = std::fs::remove_dir_all(snapshot);
            Ok((graph, entities.len()))
        }
    }
}

fn materialize_source_tree(
    repo_path: &Path,
    source: &ContentSource,
    files: &[String],
) -> Result<PathBuf, AnalyzeError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let snapshot =
        std::env::temp_dir().join(format!("inspect-graph-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&snapshot)
        .map_err(|e| AnalyzeError::Git(format!("failed to create graph snapshot: {e}")))?;

    for file in files {
        let content = match read_file_from_source(repo_path, source, file) {
            Some(content) => content,
            None => continue,
        };
        let path = snapshot.join(file);
        if !path.starts_with(&snapshot) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AnalyzeError::Git(format!("failed to create graph snapshot directory: {e}"))
            })?;
        }
        std::fs::write(path, content)
            .map_err(|e| AnalyzeError::Git(format!("failed to write graph snapshot: {e}")))?;
    }

    Ok(snapshot)
}

pub(crate) fn read_file_from_source(
    repo_path: &Path,
    source: &ContentSource,
    file_path: &str,
) -> Option<String> {
    match source {
        ContentSource::Worktree => std::fs::read_to_string(repo_path.join(file_path))
            .ok()
            .map(normalize_line_endings),
        ContentSource::Index => git_show_index(repo_path, file_path),
        ContentSource::GitRef(refspec) => git_show(repo_path, refspec, file_path),
    }
}

fn git_show_index(repo_path: &Path, file_path: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["show", &format!(":{file_path}")])
        .current_dir(repo_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(normalize_line_endings)
}

fn git_show(repo_path: &Path, refspec: &str, file_path: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{refspec}:{file_path}")])
        .current_dir(repo_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(normalize_line_endings)
}

fn list_source_files_from_source(
    repo_path: &Path,
    source: &ContentSource,
) -> Result<Vec<String>, AnalyzeError> {
    match source {
        ContentSource::Worktree => list_source_files(repo_path),
        ContentSource::Index => list_source_files(repo_path),
        ContentSource::GitRef(refspec) => list_source_files_at_ref(repo_path, refspec),
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
        .filter(|f| !is_noise_file(f))
        .filter(|f| is_source_file(f))
        .map(|s| s.to_string())
        .collect();

    Ok(files)
}

fn list_source_files_at_ref(repo_path: &Path, refspec: &str) -> Result<Vec<String>, AnalyzeError> {
    let output = std::process::Command::new("git")
        .args(["ls-tree", "-r", "--name-only", refspec])
        .current_dir(repo_path)
        .output()
        .map_err(|e| AnalyzeError::Git(format!("failed to run git ls-tree: {}", e)))?;

    if !output.status.success() {
        return Err(AnalyzeError::Git(format!(
            "git ls-tree failed for {refspec}"
        )));
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

fn is_source_file(file_path: &str) -> bool {
    let file_path = file_path.to_lowercase();
    file_path.ends_with(".rs")
        || file_path.ends_with(".ts")
        || file_path.ends_with(".tsx")
        || file_path.ends_with(".js")
        || file_path.ends_with(".jsx")
        || file_path.ends_with(".py")
        || file_path.ends_with(".go")
        || file_path.ends_with(".java")
        || file_path.ends_with(".c")
        || file_path.ends_with(".cpp")
        || file_path.ends_with(".rb")
        || file_path.ends_with(".cs")
        || file_path.ends_with(".php")
}

fn normalize_line_endings(s: String) -> String {
    if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        s
    }
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
        dependency_edges: vec![],
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

    fn make_review(id: &str, name: &str, risk_level: RiskLevel) -> EntityReview {
        EntityReview {
            entity_id: id.into(),
            entity_name: name.into(),
            entity_type: "function".into(),
            file_path: "main.rs".into(),
            change_type: ChangeType::Modified,
            classification: ChangeClassification::Functional,
            risk_score: 0.5,
            risk_level,
            blast_radius: 0,
            dependent_count: 0,
            dependency_count: 0,
            is_public_api: false,
            structural_change: None,
            group_id: 0,
            start_line: 1,
            end_line: 1,
            before_content: None,
            after_content: None,
            dependent_names: vec![],
            dependency_names: vec![],
            dependent_entities: vec![],
            dependency_entities: vec![],
            before_file_context: None,
            after_file_context: None,
        }
    }

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

    #[test]
    fn retain_entity_reviews_recomputes_stats_and_groups() {
        let mut result = ReviewResult {
            entity_reviews: vec![
                make_review("main.rs::one", "one", RiskLevel::High),
                make_review("main.rs::two", "two", RiskLevel::Medium),
            ],
            groups: vec![ChangeGroup {
                id: 0,
                label: "main.rs".into(),
                entity_ids: vec!["main.rs::one".into(), "main.rs::two".into()],
            }],
            stats: compute_stats(&[]),
            timing: Timing::default(),
            dependency_edges: vec![("main.rs::one".into(), "main.rs::two".into())],
            changes: vec![],
        };

        retain_entity_reviews(&mut result, |review| {
            review.risk_level >= RiskLevel::Critical
        });

        assert!(result.entity_reviews.is_empty());
        assert!(result.groups.is_empty());
        assert_eq!(result.stats.total_entities, 0);
        assert_eq!(result.stats.by_risk.high, 0);
    }

    #[test]
    fn retain_entity_reviews_updates_remaining_group_ids() {
        let mut result = ReviewResult {
            entity_reviews: vec![
                make_review("main.rs::one", "one", RiskLevel::High),
                make_review("main.rs::two", "two", RiskLevel::Medium),
            ],
            groups: vec![ChangeGroup {
                id: 0,
                label: "main.rs".into(),
                entity_ids: vec!["main.rs::one".into(), "main.rs::two".into()],
            }],
            stats: compute_stats(&[]),
            timing: Timing::default(),
            dependency_edges: vec![("main.rs::one".into(), "main.rs::two".into())],
            changes: vec![],
        };

        retain_entity_reviews(&mut result, |review| review.entity_id == "main.rs::two");

        assert_eq!(result.entity_reviews.len(), 1);
        assert_eq!(result.entity_reviews[0].group_id, 0);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].entity_ids, vec!["main.rs::two"]);
        assert_eq!(result.groups[0].label, "two");
        assert_eq!(result.stats.total_entities, 1);
        assert_eq!(result.stats.by_risk.medium, 1);
    }

    #[test]
    fn retain_entity_reviews_drops_removed_bridge_edges() {
        let mut result = ReviewResult {
            entity_reviews: vec![
                make_review("main.rs::one", "one", RiskLevel::High),
                make_review("main.rs::two", "two", RiskLevel::Medium),
                make_review("main.rs::three", "three", RiskLevel::High),
            ],
            groups: vec![ChangeGroup {
                id: 0,
                label: "main.rs".into(),
                entity_ids: vec![
                    "main.rs::one".into(),
                    "main.rs::two".into(),
                    "main.rs::three".into(),
                ],
            }],
            stats: compute_stats(&[]),
            timing: Timing::default(),
            dependency_edges: vec![
                ("main.rs::one".into(), "main.rs::two".into()),
                ("main.rs::two".into(), "main.rs::three".into()),
            ],
            changes: vec![],
        };

        retain_entity_reviews(&mut result, |review| review.risk_level >= RiskLevel::High);

        assert_eq!(result.entity_reviews.len(), 2);
        assert_eq!(result.groups.len(), 2);
        assert!(result
            .groups
            .iter()
            .all(|group| group.entity_ids.len() == 1));
        assert_ne!(
            result.entity_reviews[0].group_id,
            result.entity_reviews[1].group_id
        );
        assert!(result.dependency_edges.is_empty());
        assert_eq!(result.stats.total_entities, 2);
        assert_eq!(result.stats.by_risk.high, 2);
    }

    #[test]
    fn retain_entity_reviews_supports_sequential_filters() {
        let mut result = ReviewResult {
            entity_reviews: vec![
                make_review("main.rs::one", "one", RiskLevel::High),
                make_review("main.rs::two", "two", RiskLevel::Medium),
                make_review("main.rs::three", "three", RiskLevel::High),
                make_review("main.rs::four", "four", RiskLevel::Low),
            ],
            groups: vec![
                ChangeGroup {
                    id: 0,
                    label: "main.rs".into(),
                    entity_ids: vec![
                        "main.rs::one".into(),
                        "main.rs::two".into(),
                        "main.rs::three".into(),
                    ],
                },
                ChangeGroup {
                    id: 1,
                    label: "four".into(),
                    entity_ids: vec!["main.rs::four".into()],
                },
            ],
            stats: compute_stats(&[]),
            timing: Timing::default(),
            dependency_edges: vec![
                ("main.rs::one".into(), "main.rs::two".into()),
                ("main.rs::two".into(), "main.rs::three".into()),
            ],
            changes: vec![],
        };

        retain_entity_reviews(&mut result, |review| review.entity_id != "main.rs::four");
        assert_eq!(result.dependency_edges.len(), 2);

        retain_entity_reviews(&mut result, |review| review.risk_level >= RiskLevel::High);

        assert_eq!(result.entity_reviews.len(), 2);
        assert_eq!(result.groups.len(), 2);
        assert!(result.dependency_edges.is_empty());
    }

    #[test]
    fn analyze_with_options_collects_review_prompt_context() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(
            dir.join("main.rs"),
            r#"fn reject() -> ! {
    panic!("invalid");
}

pub fn validate(ok: bool) -> &'static str {
    if !ok {
        return "bad";
    }
    "ok"
}

fn consumer() {
    let _ = validate(true);
}
"#,
        )
        .unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("main.rs"),
            r#"fn reject() -> ! {
    panic!("invalid");
}

pub fn validate(ok: bool) -> &'static str {
    if !ok {
        reject();
    }
    "ok"
}

fn consumer() {
    let _ = validate(true);
}
"#,
        )
        .unwrap();
        commit(dir, "call reject");

        std::fs::write(
            dir.join("main.rs"),
            r#"fn reject() -> ! {
    panic!("worktree drift");
}

pub fn validate(ok: bool) -> &'static str {
    if !ok {
        reject();
    }
    "ok"
}

fn consumer() {
    let _ = validate(true);
}
"#,
        )
        .unwrap();

        let result = analyze_with_options(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
            &AnalyzeOptions {
                include_dependent_code: true,
                include_dependency_code: true,
                include_file_context: true,
                file_context_lines: 1,
                ..AnalyzeOptions::default()
            },
        )
        .unwrap();

        let review = result
            .entity_reviews
            .iter()
            .find(|review| review.entity_name == "validate")
            .expect("validate should be reviewed");

        assert!(review
            .dependency_entities
            .iter()
            .any(|entity| entity.entity_name == "reject" && entity.content.contains("panic!")));
        assert!(review
            .dependency_entities
            .iter()
            .all(|entity| !entity.content.contains("worktree drift")));
        assert!(review
            .dependent_entities
            .iter()
            .any(|entity| entity.entity_name == "consumer"));
        assert!(review
            .after_file_context
            .as_ref()
            .is_some_and(|context| context.content.contains("validate")));
    }

    #[test]
    fn analyze_with_options_collects_before_side_dependency_context() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        init_repo(dir);

        std::fs::write(
            dir.join("main.rs"),
            r#"fn reject() -> ! {
    panic!("invalid");
}

pub fn validate(ok: bool) -> &'static str {
    if !ok {
        reject();
    }
    "ok"
}
"#,
        )
        .unwrap();
        commit(dir, "init");

        std::fs::write(
            dir.join("main.rs"),
            r#"fn reject() -> ! {
    panic!("invalid");
}

pub fn validate(ok: bool) -> &'static str {
    if !ok {
        return "bad";
    }
    "ok"
}
"#,
        )
        .unwrap();
        commit(dir, "remove reject call");

        let result = analyze_with_options(
            dir,
            DiffScope::Commit {
                sha: "HEAD".to_string(),
            },
            &AnalyzeOptions {
                include_dependency_code: true,
                ..AnalyzeOptions::default()
            },
        )
        .unwrap();

        let review = result
            .entity_reviews
            .iter()
            .find(|review| review.entity_name == "validate")
            .expect("validate should be reviewed");

        assert!(review.dependency_entities.iter().any(|entity| {
            entity.entity_name == "reject"
                && entity.relation.as_deref() == Some("before")
                && entity.content.contains("panic!")
        }));
    }

    #[test]
    fn resolve_before_entity_id_uses_old_name_and_file_for_renames() {
        let mut entities = HashMap::new();
        entities.insert(
            "old-id".to_string(),
            EntityInfo {
                id: "old-id".to_string(),
                name: "validate".to_string(),
                entity_type: "function".to_string(),
                file_path: "old.rs".to_string(),
                parent_id: None,
                start_line: 1,
                end_line: 3,
            },
        );
        let graph = EntityGraph::from_parts(entities, vec![]);

        let change: sem_core::model::change::SemanticChange =
            serde_json::from_value(serde_json::json!({
                "id": "change-id",
                "entityId": "new-id",
                "changeType": "renamed",
                "entityType": "function",
                "entityName": "validate_new",
                "entityLine": 1,
                "filePath": "new.rs",
                "oldEntityName": "validate",
                "oldFilePath": "old.rs"
            }))
            .unwrap();

        assert_eq!(
            resolve_before_entity_id(&change, &graph).as_deref(),
            Some("old-id")
        );
    }
}
