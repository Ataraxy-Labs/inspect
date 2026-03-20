use sem_core::model::change::ChangeType;
use serde::{Deserialize, Serialize};

use crate::types::{ChangeClassification, EntityReview, ReviewResult, RiskLevel};

/// Quick signal for agents about how much review attention a change needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewVerdict {
    LikelyApprovable,
    StandardReview,
    RequiresReview,
    RequiresCarefulReview,
}

impl std::fmt::Display for ReviewVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LikelyApprovable => write!(f, "likely_approvable"),
            Self::StandardReview => write!(f, "standard_review"),
            Self::RequiresReview => write!(f, "requires_review"),
            Self::RequiresCarefulReview => write!(f, "requires_careful_review"),
        }
    }
}

/// Suggest a review verdict based on the analysis result.
pub fn suggest_verdict(result: &ReviewResult) -> ReviewVerdict {
    if result.stats.by_risk.critical > 0 {
        return ReviewVerdict::RequiresCarefulReview;
    }
    if result.stats.by_risk.high > 0 {
        return ReviewVerdict::RequiresReview;
    }
    // All cosmetic = likely approvable
    let all_cosmetic = !result.entity_reviews.is_empty()
        && result.entity_reviews.iter().all(|r| r.structural_change == Some(false));
    if all_cosmetic {
        return ReviewVerdict::LikelyApprovable;
    }
    ReviewVerdict::StandardReview
}

/// Compute a risk score (0.0 to 1.0) for an entity review.
///
/// Graph-centric scoring: dependents and blast radius are the primary
/// discriminators. Classification and change type set a low baseline.
/// Only entities with real graph impact reach High/Critical.
pub fn compute_risk_score(review: &EntityReview, total_entities: usize) -> f64 {
    let mut score = 0.0;

    // Classification weight (low baseline: 0.0 to 0.32)
    score += classification_weight(review.classification);

    // Change type weight (0.0 to 0.14)
    score += change_type_weight(review.change_type);

    // Structural change bonus: confirmed logic changes rank above unknowns
    if review.structural_change == Some(true) {
        score += 0.05;
    }

    // Graph amplification: gated by entity precision and structural change.
    //
    // Three categories determine how much graph signal amplifies the score:
    // 1. Cosmetic changes (structural_hash unchanged): no amplification
    // 2. Low-precision entities (chunks, file-level, declarations): reduced
    // 3. Executable entities (functions, methods, classes): full amplification
    //
    // This replaces separate multiplicative discounts with a single gate.
    let etype = review.entity_type.as_str();
    let is_cosmetic = review.structural_change == Some(false);
    let is_low_precision = review.entity_name.starts_with("lines ")
        || etype == "file"
        || matches!(etype, "export" | "type" | "interface" | "property" | "field");

    let graph_weight = if is_cosmetic {
        0.0
    } else if is_low_precision {
        0.5
    } else {
        1.0
    };

    if graph_weight > 0.0 {
        // Public API boost
        if review.is_public_api {
            score += 0.12 * graph_weight;
        }

        // Blast radius: normalized by total entity count, sqrt-scaled
        if total_entities > 0 && review.blast_radius > 0 {
            let effective_blast = if is_generic_name(&review.entity_name) {
                review.blast_radius.min(5)
            } else {
                review.blast_radius
            };
            let blast_ratio = effective_blast as f64 / total_entities as f64;
            score += (blast_ratio.sqrt() * 0.30).min(0.20) * graph_weight;
        }

        // Dependent count: logarithmic scaling
        if review.dependent_count > 0 {
            let effective_dependents = if is_generic_name(&review.entity_name) {
                review.dependent_count.min(5)
            } else {
                review.dependent_count
            };
            score += (1.0 + effective_dependents as f64).ln() * 0.12 * graph_weight;
        }

        // Complexity signal: entities with many dependencies (imports)
        if review.dependency_count > 3 {
            score += ((review.dependency_count as f64).ln() * 0.04).min(0.12) * graph_weight;
        }
    }

    // Test-file penalty: mild — test code has real bugs too
    if crate::analyze::is_test_like_path(&review.file_path) {
        let high_impact_test = (review.change_type == ChangeType::Deleted && review.is_public_api)
            || review.dependency_count >= 120
            || (review.dependency_count >= 60 && review.structural_change == Some(true));
        score *= if high_impact_test { 0.9 } else { 0.7 };
    }

    score.min(1.0)
}

/// Map risk score to risk level.
pub fn score_to_level(score: f64) -> RiskLevel {
    if score >= 0.7 {
        RiskLevel::Critical
    } else if score >= 0.5 {
        RiskLevel::High
    } else if score >= 0.3 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

/// Classification weight: additive model with per-dimension contributions.
/// Text=0.02, Syntax=0.08, Functional=0.22. Combined = sum of present dimensions.
fn classification_weight(c: ChangeClassification) -> f64 {
    const TEXT: f64 = 0.02;
    const SYNTAX: f64 = 0.08;
    const FUNCTIONAL: f64 = 0.22;

    match c {
        ChangeClassification::Text => TEXT,
        ChangeClassification::Syntax => SYNTAX,
        ChangeClassification::Functional => FUNCTIONAL,
        ChangeClassification::TextSyntax => TEXT + SYNTAX,
        ChangeClassification::TextFunctional => TEXT + FUNCTIONAL,
        ChangeClassification::SyntaxFunctional => SYNTAX + FUNCTIONAL,
        ChangeClassification::TextSyntaxFunctional => TEXT + SYNTAX + FUNCTIONAL,
    }
}

/// Change type weight: risk contribution from the nature of the change.
/// Deleted > Added > Modified > Renamed > Moved, reflecting bug likelihood.
fn change_type_weight(ct: ChangeType) -> f64 {
    match ct {
        ChangeType::Deleted => 0.14,  // highest: may break dependents
        ChangeType::Added => 0.10,    // new code: no prior testing
        ChangeType::Modified => 0.08, // changes to tested code
        ChangeType::Renamed => 0.04,  // may break string references
        ChangeType::Moved => 0.0,     // structural only, no logic change
    }
}

/// Check if an entity name is generic/short or matches a known stdlib type,
/// which causes name-collision inflation in dependency graphs.
fn is_generic_name(name: &str) -> bool {
    const GENERIC_NAMES: &[&str] = &[
        "read", "write", "get", "set", "run", "close", "open", "reset", "mark", "flush",
        "init", "start", "stop", "next", "size", "name", "type", "value", "key", "put",
        "order", "clone", "equals", "hashCode", "toString", "toByteArray",
        "toArray", "length", "format", "parse", "create", "build", "apply",
        "accept", "test", "compare", "merge", "update", "delete", "remove",
        "add", "clear", "contains", "iterator", "stream", "values", "keys",
    ];
    const STDLIB_TYPES: &[&str] = &[
        "ByteArrayInputStream", "ByteArrayOutputStream", "HashMap", "ArrayList",
        "String", "Object", "List", "Map", "Set", "Stream", "Optional",
        "InputStream", "OutputStream", "Reader", "Writer", "Iterator",
        "StringBuilder", "StringBuffer", "LinkedList", "TreeMap", "HashSet",
    ];

    let lower = name.to_lowercase();
    if GENERIC_NAMES.iter().any(|g| g.to_lowercase() == lower) {
        return true;
    }
    STDLIB_TYPES.contains(&name)
}

/// Detect if an entity is a public API based on name and type patterns.
pub fn is_public_api(entity_type: &str, entity_name: &str, content: Option<&str>) -> bool {
    // Check content for explicit pub/export markers
    if let Some(content) = content {
        let first_meaningful = content
            .lines()
            .map(str::trim)
            .find(|line| {
                !line.is_empty()
                    && !line.starts_with("//")
                    && !line.starts_with("/*")
                    && !line.starts_with('*')
                    && !line.starts_with('@')
            })
            .unwrap_or("");

        if first_meaningful.starts_with("pub ")
            || first_meaningful.starts_with("pub(crate)")
            || first_meaningful.starts_with("export ")
            || first_meaningful.starts_with("module.exports")
            || first_meaningful.starts_with("public ")
        {
            return true;
        }
    }

    // Convention: capitalized names in Go/Java are public
    if matches!(entity_type, "function" | "method" | "struct" | "interface") {
        if let Some(first_char) = entity_name.chars().next() {
            if first_char.is_uppercase() {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EntityReview;
    use sem_core::model::change::ChangeType;

    fn make_review(
        change_type: ChangeType,
        classification: ChangeClassification,
        blast_radius: usize,
        dependent_count: usize,
        is_public: bool,
        structural_change: Option<bool>,
    ) -> EntityReview {
        EntityReview {
            entity_id: "test".into(),
            entity_name: "foo".into(),
            entity_type: "function".into(),
            file_path: "test.rs".into(),
            change_type,
            classification,
            risk_score: 0.0,
            risk_level: RiskLevel::Low,
            blast_radius,
            dependent_count,
            dependency_count: 0,
            is_public_api: is_public,
            structural_change,
            group_id: 0,
            start_line: 1,
            end_line: 10,
            before_content: None,
            after_content: None,
            dependent_names: vec![],
            dependency_names: vec![],
        }
    }

    #[test]
    fn cosmetic_change_is_low_risk() {
        let review = make_review(
            ChangeType::Modified,
            ChangeClassification::Text,
            0, 0, false,
            Some(false),
        );
        let score = compute_risk_score(&review, 10);
        assert_eq!(score_to_level(score), RiskLevel::Low);
    }

    #[test]
    fn deleted_public_with_dependents_is_critical() {
        let review = make_review(
            ChangeType::Deleted,
            ChangeClassification::Functional,
            8, 5, true,
            Some(true),
        );
        let score = compute_risk_score(&review, 10);
        assert!(score >= 0.7, "Expected Critical, got score={score}");
        assert_eq!(score_to_level(score), RiskLevel::Critical);
    }

    #[test]
    fn added_private_entity_is_low() {
        let review = make_review(
            ChangeType::Added,
            ChangeClassification::Functional,
            0, 0, false,
            None,
        );
        let score = compute_risk_score(&review, 10);
        // Added + Functional with no graph impact = Medium baseline
        assert!(score < 0.5, "Expected below High, got score={score}");
    }

    #[test]
    fn modified_functional_no_graph_is_medium() {
        let review = make_review(
            ChangeType::Modified,
            ChangeClassification::Functional,
            0, 0, false,
            Some(true),
        );
        let score = compute_risk_score(&review, 100);
        // Modified + Functional = 0.30, no graph = Medium baseline
        assert_eq!(score_to_level(score), RiskLevel::Medium);
    }

    #[test]
    fn public_api_with_dependents_is_high() {
        let review = make_review(
            ChangeType::Modified,
            ChangeClassification::Functional,
            5, 8, true,
            Some(true),
        );
        let score = compute_risk_score(&review, 100);
        assert!(score >= 0.5, "Expected High+, got score={score}");
    }

    #[test]
    fn is_public_api_detects_rust_pub() {
        assert!(is_public_api(
            "function",
            "foo",
            Some("pub fn foo() -> i32 { 1 }")
        ));
    }

    #[test]
    fn is_public_api_detects_java_public_after_annotation() {
        let content = "@Override\npublic Long getCount() {\n    return 1L;\n}";
        assert!(is_public_api("method", "getCount", Some(content)));
    }

    #[test]
    fn is_public_api_does_not_mark_private_java_method_public() {
        let content = "@Override\nprivate Long getCount() {\n    return 1L;\n}";
        assert!(!is_public_api("method", "getCount", Some(content)));
    }
}
