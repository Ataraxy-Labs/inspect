use std::collections::HashSet;

use sem_core::model::change::{ChangeType, SemanticChange};
use sem_core::parser::graph::EntityGraph;

use crate::types::{ChangeClassification, EntityReview};

use super::types::{DetectorFinding, DetectorKind, Severity};

/// Run contract-based checks using the entity graph.
///
/// These rules detect when a change breaks implicit contracts:
/// a public API was removed, a signature changed with active callers,
/// or a type changed but its dependents were not updated.
pub fn run_contract_checks(
    reviews: &[EntityReview],
    changes: &[SemanticChange],
    graph: Option<&EntityGraph>,
) -> Vec<DetectorFinding> {
    let graph = match graph {
        Some(g) => g,
        None => return Vec::new(), // remote analysis — no graph available
    };

    let mut findings = Vec::new();

    // Build a set of all entity_ids that were changed in this diff
    let changed_ids: HashSet<&str> = changes.iter().map(|c| c.entity_id.as_str()).collect();

    for review in reviews {
        match review.change_type {
            ChangeType::Deleted => {
                check_removed_public_api(review, graph, &mut findings);
            }
            ChangeType::Modified => {
                check_signature_change(review, graph, &mut findings);
                check_async_contract_regression(review, changes, graph, &mut findings);
                check_type_change_propagation(review, graph, &changed_ids, &mut findings);
            }
            _ => {}
        }
    }

    findings
}

/// A public entity was deleted that has dependents in the graph.
fn check_removed_public_api(
    review: &EntityReview,
    graph: &EntityGraph,
    findings: &mut Vec<DetectorFinding>,
) {
    if !review.is_public_api {
        return;
    }

    let dependents = graph.get_dependents(&review.entity_id);
    if dependents.is_empty() {
        return;
    }

    let dep_names: Vec<String> = dependents
        .iter()
        .take(5)
        .map(|d| format!("{} ({})", d.name, d.file_path))
        .collect();
    let suffix = if dependents.len() > 5 {
        format!(" and {} more", dependents.len() - 5)
    } else {
        String::new()
    };

    findings.push(DetectorFinding {
        rule_id: "removed-public-api".to_string(),
        message: format!(
            "Public entity `{}` was deleted but has {} dependent(s): {}{}",
            review.entity_name,
            dependents.len(),
            dep_names.join(", "),
            suffix,
        ),
        detector: DetectorKind::Contract,
        confidence: 0.8,
        severity: Severity::Critical,
        entity_id: review.entity_id.clone(),
        entity_name: review.entity_name.clone(),
        file_path: review.file_path.clone(),
        evidence: format!("Dependents: {}{}", dep_names.join(", "), suffix),
        start_line: review.start_line,
        end_line: review.end_line,
    });
}

/// A public function/method changed its signature and has dependents.
fn check_signature_change(
    review: &EntityReview,
    graph: &EntityGraph,
    findings: &mut Vec<DetectorFinding>,
) {
    if !review.is_public_api {
        return;
    }

    // Only fire for signature changes
    let is_sig_change = matches!(
        review.classification,
        ChangeClassification::Syntax
            | ChangeClassification::SyntaxFunctional
            | ChangeClassification::TextSyntax
            | ChangeClassification::TextSyntaxFunctional
    );
    if !is_sig_change {
        return;
    }

    // Only for functions/methods
    let etype = review.entity_type.as_str();
    if etype != "function" && etype != "method" && etype != "fn" {
        return;
    }

    let dependents = graph.get_dependents(&review.entity_id);
    if dependents.is_empty() {
        return;
    }

    let dep_names: Vec<String> = dependents
        .iter()
        .take(5)
        .map(|d| format!("{} ({})", d.name, d.file_path))
        .collect();

    findings.push(DetectorFinding {
        rule_id: "signature-change-with-callers".to_string(),
        message: format!(
            "Public function `{}` changed signature but has {} caller(s) that may need updating",
            review.entity_name,
            dependents.len(),
        ),
        detector: DetectorKind::Contract,
        confidence: 0.7,
        severity: Severity::High,
        entity_id: review.entity_id.clone(),
        entity_name: review.entity_name.clone(),
        file_path: review.file_path.clone(),
        evidence: format!("Callers: {}", dep_names.join(", ")),
        start_line: review.start_line,
        end_line: review.end_line,
    });
}

/// Detect when a public callable appears to remove async/future-style contract cues
/// while still having active callers. This can silently break call-site behavior.
fn check_async_contract_regression(
    review: &EntityReview,
    changes: &[SemanticChange],
    graph: &EntityGraph,
    findings: &mut Vec<DetectorFinding>,
) {
    if !review.is_public_api {
        return;
    }
    if !matches!(review.entity_type.as_str(), "function" | "method" | "fn") {
        return;
    }

    let Some(change) = changes.iter().find(|c| c.entity_id == review.entity_id) else {
        return;
    };
    let (Some(before), Some(after)) = (change.before_content.as_deref(), change.after_content.as_deref()) else {
        return;
    };

    let before_async = has_async_cue(before);
    let after_async = has_async_cue(after);
    if !before_async || after_async {
        return;
    }

    let dependents = graph.get_dependents(&review.entity_id);
    if dependents.is_empty() {
        return;
    }

    findings.push(DetectorFinding {
        rule_id: "async-contract-regression".to_string(),
        message: format!(
            "Public callable `{}` dropped async/future cues but still has {} caller(s)",
            review.entity_name,
            dependents.len()
        ),
        detector: DetectorKind::Contract,
        confidence: 0.68,
        severity: Severity::High,
        entity_id: review.entity_id.clone(),
        entity_name: review.entity_name.clone(),
        file_path: review.file_path.clone(),
        evidence: "before had async/future cue, after does not".to_string(),
        start_line: review.start_line,
        end_line: review.end_line,
    });
}

fn has_async_cue(content: &str) -> bool {
    let header = content.lines().take(4).collect::<Vec<_>>().join("\n").to_lowercase();
    header.contains("async ")
        || header.contains("future")
        || header.contains("promise<")
        || header.contains("completablefuture")
        || header.contains("task<")
}

#[cfg(test)]
mod tests {
    use super::has_async_cue;

    #[test]
    fn has_async_cue_detects_async_keyword() {
        assert!(has_async_cue("pub async fn run() -> Result<()> {"));
    }

    #[test]
    fn has_async_cue_detects_future_return() {
        assert!(has_async_cue("public CompletableFuture<User> load() {"));
    }

    #[test]
    fn has_async_cue_ignores_non_async_header() {
        assert!(!has_async_cue("fn run() {\n  work();\n}"));
    }
}

/// A type/struct/interface was modified but its dependents were NOT also changed.
fn check_type_change_propagation(
    review: &EntityReview,
    graph: &EntityGraph,
    changed_ids: &HashSet<&str>,
    findings: &mut Vec<DetectorFinding>,
) {
    let etype = review.entity_type.as_str();
    if etype != "type" && etype != "struct" && etype != "interface"
        && etype != "class" && etype != "enum" && etype != "type_alias"
    {
        return;
    }

    let dependents = graph.get_dependents(&review.entity_id);
    if dependents.is_empty() {
        return;
    }

    // Find dependents that were NOT changed in this diff
    let unchanged_deps: Vec<&str> = dependents
        .iter()
        .filter(|d| !changed_ids.contains(d.id.as_str()))
        .map(|d| d.name.as_str())
        .collect();

    if unchanged_deps.is_empty() {
        return; // all dependents were also updated — good
    }

    let display_deps: Vec<&str> = unchanged_deps.iter().take(5).copied().collect();
    let suffix = if unchanged_deps.len() > 5 {
        format!(" and {} more", unchanged_deps.len() - 5)
    } else {
        String::new()
    };

    findings.push(DetectorFinding {
        rule_id: "type-change-propagation".to_string(),
        message: format!(
            "Type `{}` was modified but {} dependent(s) were not updated in this diff: {}{}",
            review.entity_name,
            unchanged_deps.len(),
            display_deps.join(", "),
            suffix,
        ),
        detector: DetectorKind::Contract,
        confidence: 0.6,
        severity: Severity::Medium,
        entity_id: review.entity_id.clone(),
        entity_name: review.entity_name.clone(),
        file_path: review.file_path.clone(),
        evidence: format!("Unchanged dependents: {}{}", display_deps.join(", "), suffix),
        start_line: review.start_line,
        end_line: review.end_line,
    });
}
