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
                check_arity_change_with_callers(review, changes, graph, &mut findings);
                check_async_contract_regression(review, changes, graph, &mut findings);
                check_type_change_propagation(review, graph, &changed_ids, &mut findings);
                check_interface_impl_mismatch(review, changes, graph, &changed_ids, &mut findings);
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

/// Detect parameter-count changes on public callables with active dependents.
fn check_arity_change_with_callers(
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

    let before_arity = estimate_param_count(before);
    let after_arity = estimate_param_count(after);
    let (Some(before_arity), Some(after_arity)) = (before_arity, after_arity) else {
        return;
    };
    if before_arity == after_arity {
        return;
    }

    let dependents = graph.get_dependents(&review.entity_id);
    if dependents.is_empty() {
        return;
    }

    findings.push(DetectorFinding {
        rule_id: "arity-change-with-callers".to_string(),
        message: format!(
            "Public callable `{}` changed arity ({} -> {}) with {} caller(s)",
            review.entity_name,
            before_arity,
            after_arity,
            dependents.len()
        ),
        detector: DetectorKind::Contract,
        confidence: 0.74,
        severity: Severity::High,
        entity_id: review.entity_id.clone(),
        entity_name: review.entity_name.clone(),
        file_path: review.file_path.clone(),
        evidence: format!("arity {} -> {}", before_arity, after_arity),
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

fn estimate_param_count(content: &str) -> Option<usize> {
    let head = content.lines().take(4).collect::<Vec<_>>().join(" ");
    let start = head.find('(')?;
    let end = head[start..].find(')')? + start;
    if end <= start {
        return None;
    }
    let params = head[start + 1..end].trim();
    if params.is_empty() {
        return Some(0);
    }
    Some(
        params
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty() && *p != "self" && *p != "&self" && *p != "this")
            .count(),
    )
}

#[cfg(test)]
mod tests {
    use super::{estimate_param_count, has_async_cue};

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

    #[test]
    fn estimate_param_count_works_for_simple_function() {
        assert_eq!(estimate_param_count("fn run(a: i32, b: i32) -> i32 {"), Some(2));
    }

    #[test]
    fn estimate_param_count_ignores_self_and_this() {
        assert_eq!(estimate_param_count("pub fn run(&self, value: i32) {"), Some(1));
        assert_eq!(estimate_param_count("public void run(this, a, b) {"), Some(2));
    }

    #[test]
    fn extract_method_signatures_detects_methods() {
        use super::extract_method_signatures;
        let content = "interface Repo {\n  findById(id: string): Promise<User>;\n  save(user: User): void;\n}";
        let sigs = extract_method_signatures(content);
        assert_eq!(sigs.len(), 2);
        assert_eq!(sigs[0].0, "findById");
        assert_eq!(sigs[1].0, "save");
    }

    #[test]
    fn detect_signature_changes_catches_arity_change() {
        use super::{extract_method_signatures, detect_signature_changes};
        let before = "interface Repo {\n  findById(id: string): Promise<User>;\n}";
        let after = "interface Repo {\n  findById(id: string, tenant: string): Promise<User>;\n}";
        let before_sigs = extract_method_signatures(before);
        let after_sigs = extract_method_signatures(after);
        assert!(detect_signature_changes(&before_sigs, &after_sigs));
    }

    #[test]
    fn detect_signature_changes_ignores_unchanged() {
        use super::{extract_method_signatures, detect_signature_changes};
        let before = "interface Repo {\n  findById(id: string): Promise<User>;\n}";
        let after = "interface Repo {\n  findById(id: string): Promise<User>;\n}";
        let before_sigs = extract_method_signatures(before);
        let after_sigs = extract_method_signatures(after);
        assert!(!detect_signature_changes(&before_sigs, &after_sigs));
    }
}

/// An interface/trait/protocol method signature changed but implementations were NOT updated.
///
/// When an interface adds, removes, or changes method signatures, implementations
/// need updating too. This fires when the graph shows dependents (implementations)
/// that were not part of this diff.
fn check_interface_impl_mismatch(
    review: &EntityReview,
    changes: &[SemanticChange],
    graph: &EntityGraph,
    changed_ids: &HashSet<&str>,
    findings: &mut Vec<DetectorFinding>,
) {
    // Only fire for interface-like entities
    let is_interface = review.entity_type.as_str() == "interface"
        || review.entity_type.as_str() == "trait"
        || review.entity_type.as_str() == "protocol"
        || review.entity_type.as_str() == "abstract_class";

    // Also check content for interface keyword if entity_type is generic
    let content_is_interface = review
        .after_content
        .as_deref()
        .map(|c| {
            let header = c.lines().take(3).collect::<Vec<_>>().join(" ");
            header.contains("interface ") || header.contains("trait ") || header.contains("protocol ")
        })
        .unwrap_or(false);

    if !is_interface && !content_is_interface {
        return;
    }

    // Find the matching change to get before/after content
    let Some(change) = changes.iter().find(|c| c.entity_id == review.entity_id) else {
        return;
    };
    let (Some(before), Some(after)) = (change.before_content.as_deref(), change.after_content.as_deref()) else {
        return;
    };

    // Extract method signatures from before and after
    let before_sigs = extract_method_signatures(before);
    let after_sigs = extract_method_signatures(after);

    // Check if any signature actually changed (arity or parameter types)
    let has_sig_change = detect_signature_changes(&before_sigs, &after_sigs);
    if !has_sig_change {
        return;
    }

    // Get implementations (dependents of this interface)
    let dependents = graph.get_dependents(&review.entity_id);
    if dependents.is_empty() {
        return;
    }

    // Find implementations that were NOT changed in this diff
    let unchanged_impls: Vec<_> = dependents
        .iter()
        .filter(|d| !changed_ids.contains(d.id.as_str()))
        .collect();

    if unchanged_impls.is_empty() {
        return; // all implementations were updated — good
    }

    let display_impls: Vec<String> = unchanged_impls
        .iter()
        .take(5)
        .map(|d| format!("{} ({})", d.name, d.file_path))
        .collect();
    let suffix = if unchanged_impls.len() > 5 {
        format!(" and {} more", unchanged_impls.len() - 5)
    } else {
        String::new()
    };

    findings.push(DetectorFinding {
        rule_id: "interface-impl-mismatch".to_string(),
        message: format!(
            "Interface `{}` method signature changed but {} implementation(s) were not updated: {}{}",
            review.entity_name,
            unchanged_impls.len(),
            display_impls.join(", "),
            suffix,
        ),
        detector: DetectorKind::Contract,
        confidence: 0.75,
        severity: Severity::High,
        entity_id: review.entity_id.clone(),
        entity_name: review.entity_name.clone(),
        file_path: review.file_path.clone(),
        evidence: format!("Unchanged implementations: {}{}", display_impls.join(", "), suffix),
        start_line: review.start_line,
        end_line: review.end_line,
    });
}

/// Extract method signatures from content as (name, param_count, param_types_hash).
fn extract_method_signatures(content: &str) -> Vec<(String, usize, String)> {
    let mut sigs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip comments, annotations, and empty lines
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with('@')
            || trimmed.starts_with('#')
        {
            continue;
        }

        // Look for function/method declarations: something followed by (params)
        let Some(paren_start) = trimmed.find('(') else {
            continue;
        };
        if paren_start == 0 {
            continue;
        }

        // Extract the name (last identifier before the paren)
        let before_paren = &trimmed[..paren_start];
        let name = before_paren
            .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");

        if name.is_empty() || name.len() < 2 {
            continue;
        }

        // Skip keywords that look like function names but aren't
        if matches!(name, "if" | "for" | "while" | "switch" | "catch" | "return" | "new" | "class" | "interface" | "trait") {
            continue;
        }

        // Extract parameters
        let after_paren = &trimmed[paren_start + 1..];
        let paren_end = after_paren.find(')').unwrap_or(after_paren.len());
        let params = after_paren[..paren_end].trim();

        let param_count = if params.is_empty() {
            0
        } else {
            params
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty() && *p != "self" && *p != "&self" && *p != "this")
                .count()
        };

        // Create a rough hash of parameter types for comparison
        let param_types: String = params
            .split(',')
            .map(|p| p.trim().to_string())
            .collect::<Vec<_>>()
            .join(",");

        sigs.push((name.to_string(), param_count, param_types));
    }
    sigs
}

/// Detect if any method signature changed between before and after.
fn detect_signature_changes(
    before_sigs: &[(String, usize, String)],
    after_sigs: &[(String, usize, String)],
) -> bool {
    for after_sig in after_sigs {
        if let Some(before_sig) = before_sigs.iter().find(|b| b.0 == after_sig.0) {
            // Same method name — check if arity or param types changed
            if before_sig.1 != after_sig.1 || before_sig.2 != after_sig.2 {
                return true;
            }
        }
    }
    // Also check for methods that were added or removed
    let before_names: HashSet<&str> = before_sigs.iter().map(|s| s.0.as_str()).collect();
    let after_names: HashSet<&str> = after_sigs.iter().map(|s| s.0.as_str()).collect();
    before_names != after_names
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
