use inspect_core::risk::suggest_verdict;
use inspect_core::types::{ReviewResult, RiskLevel};

pub fn print(result: &ReviewResult, show_context: bool) {
    if result.entity_reviews.is_empty() {
        println!("No entity-level changes found.");
        return;
    }

    let stats = &result.stats;
    let verdict = suggest_verdict(result);

    println!("# inspect: {} entities changed", stats.total_entities);
    println!();
    println!(
        "**Verdict:** {} | **Critical:** {} | **High:** {} | **Medium:** {} | **Low:** {}",
        verdict, stats.by_risk.critical, stats.by_risk.high, stats.by_risk.medium, stats.by_risk.low,
    );

    // Groups
    if result.groups.len() > 1 {
        println!();
        println!("## Groups ({} logical change groups)", result.groups.len());
        println!();
        for group in &result.groups {
            println!(
                "- **[{}]** {} ({} entities)",
                group.id,
                group.label,
                group.entity_ids.len()
            );
        }
    }

    println!();
    println!("## Entities (by risk)");
    println!();
    println!("| Risk | Type | Entity | File | Score | Classification | Blast | Change |");
    println!("|------|------|--------|------|-------|----------------|-------|--------|");

    for review in &result.entity_reviews {
        let risk = match review.risk_level {
            RiskLevel::Critical => "CRITICAL",
            RiskLevel::High => "HIGH",
            RiskLevel::Medium => "MEDIUM",
            RiskLevel::Low => "LOW",
        };

        let change = format!("{:?}", review.change_type);

        println!(
            "| {} | {} | `{}` | `{}` | {:.2} | {} | {} | {} |",
            risk,
            review.entity_type,
            review.entity_name,
            review.file_path,
            review.risk_score,
            review.classification,
            review.blast_radius,
            change.to_lowercase(),
        );
    }

    // Detail section for high-risk entities
    let high_risk: Vec<_> = result
        .entity_reviews
        .iter()
        .filter(|r| r.risk_level >= RiskLevel::High)
        .collect();

    if !high_risk.is_empty() {
        println!();
        println!("## High-risk details");

        for review in high_risk {
            println!();
            println!(
                "### `{}` ({}) in `{}`",
                review.entity_name, review.entity_type, review.file_path
            );
            println!();
            println!(
                "- **Risk:** {:?} ({:.2}) | **Blast radius:** {} | **Public API:** {}",
                review.risk_level, review.risk_score, review.blast_radius, review.is_public_api,
            );

            if review.structural_change == Some(false) {
                println!("- Cosmetic only (no structural change)");
            }

            if show_context {
                if review.dependent_count > 0 {
                    println!("- {} dependents may be affected", review.dependent_count);
                }
                if review.dependency_count > 0 {
                    println!("- Depends on {} other entities", review.dependency_count);
                }
            }
        }
    }

    // Timing
    let t = &result.timing;
    if t.total_ms > 0 {
        println!();
        println!("---");
        println!(
            "*{}ms total ({} files, {} entities) | diff: {}ms, graph: {}ms, scoring: {}ms*",
            t.total_ms, t.file_count, t.graph_entity_count, t.diff_ms, t.graph_build_ms, t.scoring_ms,
        );
    }
}
