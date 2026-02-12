use sem_core::model::change::ChangeType;

use crate::types::{ChangeClassification, EntityReview, RiskLevel};

/// Compute a risk score (0.0 to 1.0) for an entity review.
pub fn compute_risk_score(review: &EntityReview, total_entities: usize) -> f64 {
    let mut score = 0.0;

    // Classification weight (0.1 to 0.6)
    score += classification_weight(review.classification);

    // Blast radius: normalized by total entity count
    if total_entities > 0 {
        let blast_ratio = review.blast_radius as f64 / total_entities as f64;
        score += blast_ratio * 0.3;
    }

    // Dependent count: more dependents = riskier (logarithmic)
    if review.dependent_count > 0 {
        score += (1.0 + review.dependent_count as f64).ln() * 0.1;
    }

    // Public API boost
    if review.is_public_api {
        score += 0.15;
    }

    // Change type weight
    score += change_type_weight(review.change_type);

    // Cosmetic-only discount
    if review.structural_change == Some(false) {
        score *= 0.3;
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
        ChangeClassification::Text => 0.05,
        ChangeClassification::Syntax => 0.2,
        ChangeClassification::Functional => 0.4,
        ChangeClassification::TextSyntax => 0.25,
        ChangeClassification::TextFunctional => 0.45,
        ChangeClassification::SyntaxFunctional => 0.5,
        ChangeClassification::TextSyntaxFunctional => 0.55,
    }
}

fn change_type_weight(ct: ChangeType) -> f64 {
    match ct {
        ChangeType::Deleted => 0.2,
        ChangeType::Modified => 0.1,
        ChangeType::Renamed => 0.1,
        ChangeType::Moved => 0.05,
        ChangeType::Added => 0.05,
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
        assert_eq!(score_to_level(score), RiskLevel::Critical);
    }

    #[test]
    fn added_private_entity_is_medium_or_lower() {
        let review = make_review(
            ChangeType::Added,
            ChangeClassification::Functional,
            0, 0, false,
            None,
        );
        let score = compute_risk_score(&review, 10);
        // Added + Functional = 0.45, no blast radius or dependents
        assert!(score_to_level(score) <= RiskLevel::Medium);
        assert!(score < 0.5); // not High
    }
}
