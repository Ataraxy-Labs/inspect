use colored::Colorize;
use inspect_core::types::{ReviewResult, RiskLevel};

pub fn print(result: &ReviewResult, show_context: bool) {
    if result.entity_reviews.is_empty() {
        println!("{}", "No entity-level changes found.".dimmed());
        return;
    }

    let stats = &result.stats;
    println!(
        "\n{} {} entities changed",
        "inspect".bold().cyan(),
        stats.total_entities
    );
    println!(
        "  {} critical, {} high, {} medium, {} low",
        format!("{}", stats.by_risk.critical).red().bold(),
        format!("{}", stats.by_risk.high).yellow().bold(),
        format!("{}", stats.by_risk.medium).blue(),
        format!("{}", stats.by_risk.low).dimmed(),
    );

    // Groups summary
    if result.groups.len() > 1 {
        println!(
            "\n{} {} logical groups:",
            "groups".bold(),
            result.groups.len()
        );
        for group in &result.groups {
            println!(
                "  [{}] {} ({} entities)",
                group.id,
                group.label.bold(),
                group.entity_ids.len()
            );
        }
    }

    println!("\n{}", "entities (by risk):".bold().underline());

    for review in &result.entity_reviews {
        let risk_badge = match review.risk_level {
            RiskLevel::Critical => format!(" CRITICAL ").on_red().white().bold().to_string(),
            RiskLevel::High => format!(" HIGH ").on_yellow().black().bold().to_string(),
            RiskLevel::Medium => format!(" MEDIUM ").on_blue().white().to_string(),
            RiskLevel::Low => format!(" LOW ").dimmed().to_string(),
        };

        let change_icon = match review.change_type {
            sem_core::model::change::ChangeType::Added => "+".green().bold(),
            sem_core::model::change::ChangeType::Deleted => "-".red().bold(),
            sem_core::model::change::ChangeType::Modified => "~".yellow(),
            sem_core::model::change::ChangeType::Moved => ">".blue(),
            sem_core::model::change::ChangeType::Renamed => "r".blue(),
        };

        println!(
            "\n  {} {} {} {}",
            change_icon,
            risk_badge,
            format!("{} {}", review.entity_type, review.entity_name).bold(),
            format!("({})", review.file_path).dimmed(),
        );

        println!(
            "    classification: {}  score: {:.2}  blast: {}  deps: {}/{}",
            review.classification,
            review.risk_score,
            review.blast_radius,
            review.dependency_count,
            review.dependent_count,
        );

        if review.is_public_api {
            println!("    {}", "public API".yellow());
        }

        if review.structural_change == Some(false) {
            println!("    {}", "cosmetic only (no structural change)".dimmed());
        }

        if show_context {
            // Find the corresponding change to show dependency info
            if review.dependent_count > 0 {
                println!("    {} {} dependents may be affected", ">>>".yellow(), review.dependent_count);
            }
            if review.dependency_count > 0 {
                println!("    {} depends on {} other entities", "<<<".cyan(), review.dependency_count);
            }
        }
    }

    // Timing
    let t = &result.timing;
    if t.total_ms > 0 {
        println!(
            "\n{}  {}ms total ({} files, {} entities)",
            "timing".dimmed(),
            t.total_ms,
            t.file_count,
            t.graph_entity_count,
        );
        println!(
            "  diff: {}ms  graph: {}ms  scoring: {}ms",
            t.diff_ms, t.graph_build_ms, t.scoring_ms,
        );
    }

    println!();
}
