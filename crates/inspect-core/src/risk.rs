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

    // Classification weight (low baseline: 0.0 to 0.15)
    score += classification_weight(review.classification);

    // Change type weight (0.0 to 0.1)
    score += change_type_weight(review.change_type);

    // Public API boost
    if review.is_public_api {
        score += 0.12;
    }

    // Blast radius: normalized by total entity count, sqrt-scaled
    if total_entities > 0 && review.blast_radius > 0 {
        let blast_ratio = review.blast_radius as f64 / total_entities as f64;
        score += blast_ratio.sqrt() * 0.30;
    }

    // Dependent count: logarithmic scaling
    if review.dependent_count > 0 {
        score += (1.0 + review.dependent_count as f64).ln() * 0.15;
    }

    // Cosmetic-only discount (structural_hash unchanged)
    if review.structural_change == Some(false) {
        score *= 0.2;
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

fn classification_weight(c: ChangeClassification) -> f64 {
    match c {
        ChangeClassification::Text => 0.0,
        ChangeClassification::Syntax => 0.08,
        ChangeClassification::Functional => 0.22,
        ChangeClassification::TextSyntax => 0.1,
        ChangeClassification::TextFunctional => 0.22,
        ChangeClassification::SyntaxFunctional => 0.25,
        ChangeClassification::TextSyntaxFunctional => 0.28,
    }
}

fn change_type_weight(ct: ChangeType) -> f64 {
    match ct {
        ChangeType::Deleted => 0.12,
        ChangeType::Modified => 0.08,
        ChangeType::Renamed => 0.04,
        ChangeType::Moved => 0.0,
        ChangeType::Added => 0.02,
    }
}

/// Detect if an entity is a public API based on name and type patterns.
pub fn is_public_api(entity_type: &str, entity_name: &str, content: Option<&str>) -> bool {
    // Check content for explicit pub/export markers
    if let Some(content) = content {
        let first_line = content.lines().next().unwrap_or("");
        if first_line.starts_with("pub ")
            || first_line.starts_with("pub(crate)")
            || first_line.starts_with("export ")
            || first_line.starts_with("module.exports")
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
        // Added + Functional with no graph impact = low baseline
        assert_eq!(score_to_level(score), RiskLevel::Low);
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
}
