mod contracts;
mod diff_heuristics;
mod patterns;
pub mod types;

pub use contracts::run_contract_checks;
pub use diff_heuristics::run_diff_heuristics;
pub use patterns::run_pattern_rules;
pub use types::{DetectorFinding, DetectorKind, Severity};

use std::collections::HashSet;

use sem_core::model::change::SemanticChange;
use sem_core::parser::graph::EntityGraph;

use crate::types::EntityReview;

/// Run all deterministic detectors and return merged, deduped findings.
///
/// Each detector runs independently on the input data:
/// - **Pattern rules**: AST pattern matching on `after_content`
/// - **Contract checks**: Signature/dependency analysis using the entity graph
/// - **Diff heuristics**: Before/after content comparison for regression patterns
///
/// Results are deduped by `(entity_id, rule_id)` — if the same rule fires on
/// the same entity from multiple detectors, only the highest-confidence one is kept.
pub fn run_all_detectors(
    reviews: &[EntityReview],
    changes: &[SemanticChange],
    graph: Option<&EntityGraph>,
) -> Vec<DetectorFinding> {
    let mut all_findings = Vec::new();

    all_findings.extend(run_pattern_rules(changes));
    all_findings.extend(run_contract_checks(reviews, changes, graph));
    all_findings.extend(run_diff_heuristics(changes));

    dedup_findings(all_findings)
}

/// Dedup findings by (entity_id, rule_id), keeping the highest confidence.
fn dedup_findings(mut findings: Vec<DetectorFinding>) -> Vec<DetectorFinding> {
    // Sort by confidence descending so the first occurrence wins
    findings.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

    let mut seen = HashSet::new();
    findings.retain(|f| seen.insert((f.entity_id.clone(), f.rule_id.clone())));

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_keeps_highest_confidence() {
        let findings = vec![
            DetectorFinding {
                rule_id: "test-rule".to_string(),
                message: "low".to_string(),
                detector: DetectorKind::Pattern,
                confidence: 0.5,
                severity: Severity::Low,
                entity_id: "e1".to_string(),
                entity_name: "foo".to_string(),
                file_path: "test.ts".to_string(),
                evidence: "x".to_string(),
                start_line: 1,
                end_line: 1,
            },
            DetectorFinding {
                rule_id: "test-rule".to_string(),
                message: "high".to_string(),
                detector: DetectorKind::DiffHeuristic,
                confidence: 0.9,
                severity: Severity::High,
                entity_id: "e1".to_string(),
                entity_name: "foo".to_string(),
                file_path: "test.ts".to_string(),
                evidence: "x".to_string(),
                start_line: 1,
                end_line: 1,
            },
        ];

        let deduped = dedup_findings(findings);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].confidence, 0.9);
        assert_eq!(deduped[0].message, "high");
    }

    #[test]
    fn test_different_rules_not_deduped() {
        let findings = vec![
            DetectorFinding {
                rule_id: "rule-a".to_string(),
                message: "a".to_string(),
                detector: DetectorKind::Pattern,
                confidence: 0.7,
                severity: Severity::Medium,
                entity_id: "e1".to_string(),
                entity_name: "foo".to_string(),
                file_path: "test.ts".to_string(),
                evidence: "x".to_string(),
                start_line: 1,
                end_line: 1,
            },
            DetectorFinding {
                rule_id: "rule-b".to_string(),
                message: "b".to_string(),
                detector: DetectorKind::Pattern,
                confidence: 0.8,
                severity: Severity::High,
                entity_id: "e1".to_string(),
                entity_name: "foo".to_string(),
                file_path: "test.ts".to_string(),
                evidence: "y".to_string(),
                start_line: 2,
                end_line: 2,
            },
        ];

        let deduped = dedup_findings(findings);
        assert_eq!(deduped.len(), 2);
    }
}
