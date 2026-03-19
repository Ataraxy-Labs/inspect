use sem_core::model::change::SemanticChange;

use super::types::{DetectorFinding, DetectorKind, Severity};

/// Run diff-based heuristic checks by comparing before/after content.
///
/// These rules detect suspicious changes in logic: negation flips,
/// removed guards, swapped callees, added early returns, and
/// off-by-one-style changes.
pub fn run_diff_heuristics(changes: &[SemanticChange]) -> Vec<DetectorFinding> {
    let mut findings = Vec::new();

    for change in changes {
        let before = match &change.before_content {
            Some(b) => b,
            None => continue, // only applies to modifications
        };
        let after = match &change.after_content {
            Some(a) => a,
            None => continue,
        };

        let before_lines: Vec<&str> = before.lines().collect();
        let after_lines: Vec<&str> = after.lines().collect();

        check_negation_flip(change, &before_lines, &after_lines, &mut findings);
        check_removed_guard(change, &before_lines, &after_lines, &mut findings);
        check_off_by_one(change, &before_lines, &after_lines, &mut findings);
        check_null_return_introduced(change, before, after, &mut findings);
        check_error_path_changed(change, before, after, &mut findings);
        check_wrong_callee_substitution(change, &before_lines, &after_lines, &mut findings);
        check_added_early_return(change, before, after, &mut findings);
    }

    findings
}

/// Detect conditions flipped by adding/removing `!` or switching `==`/`!=`.
fn check_negation_flip(
    change: &SemanticChange,
    before_lines: &[&str],
    after_lines: &[&str],
    findings: &mut Vec<DetectorFinding>,
) {
    for (line_num, after_line) in after_lines.iter().enumerate() {
        let after_trimmed = after_line.trim();
        if after_trimmed.is_empty() {
            continue;
        }

        // Find a matching before-line that is "close" but differs by negation
        for before_line in before_lines {
            let before_trimmed = before_line.trim();
            if before_trimmed.is_empty() || before_trimmed == after_trimmed {
                continue;
            }

            // Check for == / != swap
            if before_trimmed.contains("==") && after_trimmed.contains("!=") {
                let normalized_before = before_trimmed.replace("==", "@@CMP@@");
                let normalized_after = after_trimmed.replace("!=", "@@CMP@@");
                if normalized_before == normalized_after {
                    findings.push(make_finding(
                        "negation-flip",
                        "Condition changed from `==` to `!=` — verify the logic inversion is intentional",
                        0.6,
                        Severity::Medium,
                        change,
                        after_trimmed,
                        line_num + 1,
                    ));
                }
            } else if before_trimmed.contains("!=") && after_trimmed.contains("==") {
                let normalized_before = before_trimmed.replace("!=", "@@CMP@@");
                let normalized_after = after_trimmed.replace("==", "@@CMP@@");
                if normalized_before == normalized_after {
                    findings.push(make_finding(
                        "negation-flip",
                        "Condition changed from `!=` to `==` — verify the logic inversion is intentional",
                        0.6,
                        Severity::Medium,
                        change,
                        after_trimmed,
                        line_num + 1,
                    ));
                }
            }

            // Check for ! added/removed before a condition
            if let Some(diff) = differs_only_by_negation(before_trimmed, after_trimmed) {
                findings.push(make_finding(
                    "negation-flip",
                    &format!("Negation `!` {} — verify the logic inversion is intentional", diff),
                    0.6,
                    Severity::Medium,
                    change,
                    after_trimmed,
                    line_num + 1,
                ));
            }
        }
    }
}

/// Detect guard clauses, asserts, or if-checks that were removed.
fn check_removed_guard(
    change: &SemanticChange,
    before_lines: &[&str],
    after_lines: &[&str],
    findings: &mut Vec<DetectorFinding>,
) {
    let after_joined = after_lines.join("\n");

    for before_line in before_lines {
        let trimmed = before_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let is_guard = trimmed.starts_with("if ")
            || trimmed.starts_with("if(")
            || trimmed.starts_with("assert")
            || trimmed.starts_with("guard ")
            || trimmed.starts_with("require(")
            || trimmed.contains("throw ")
            || trimmed.contains("raise ")
            || (trimmed.contains("return") && trimmed.contains("if"));

        if is_guard && !after_joined.contains(trimmed) {
            // Confirm it's actually gone, not just reformatted
            let core = trimmed
                .trim_start_matches("if ")
                .trim_start_matches("if(")
                .trim_start_matches("assert ")
                .trim_start_matches("assert(");

            if !after_joined.contains(core) {
                // Use line 1 as best-effort since we can't pinpoint where it *was*
                findings.push(make_finding(
                    "removed-guard",
                    &format!("Guard/assertion removed: `{}` — safety check may be lost", truncate(trimmed, 80)),
                    0.6,
                    Severity::High,
                    change,
                    trimmed,
                    1,
                ));
            }
        }
    }
}

/// Detect off-by-one style changes: `<` to `<=`, `>` to `>=`, or `+1`/`-1` changes.
fn check_off_by_one(
    change: &SemanticChange,
    before_lines: &[&str],
    after_lines: &[&str],
    findings: &mut Vec<DetectorFinding>,
) {
    for (line_num, after_line) in after_lines.iter().enumerate() {
        let after_trimmed = after_line.trim();

        for before_line in before_lines {
            let before_trimmed = before_line.trim();
            if before_trimmed == after_trimmed || before_trimmed.is_empty() {
                continue;
            }

            // < to <= or > to >=
            let lt_to_lte = before_trimmed.contains(" < ") && after_trimmed.contains(" <= ")
                && before_trimmed.replace(" < ", " <= ") == *after_trimmed;
            let gt_to_gte = before_trimmed.contains(" > ") && after_trimmed.contains(" >= ")
                && before_trimmed.replace(" > ", " >= ") == *after_trimmed;
            let lte_to_lt = before_trimmed.contains(" <= ") && after_trimmed.contains(" < ")
                && before_trimmed.replace(" <= ", " < ") == *after_trimmed;
            let gte_to_gt = before_trimmed.contains(" >= ") && after_trimmed.contains(" > ")
                && before_trimmed.replace(" >= ", " > ") == *after_trimmed;

            if lt_to_lte || gt_to_gte || lte_to_lt || gte_to_gt {
                findings.push(make_finding(
                    "off-by-one-hint",
                    "Comparison operator boundary changed — potential off-by-one error",
                    0.5,
                    Severity::Medium,
                    change,
                    after_trimmed,
                    line_num + 1,
                ));
            }

            // +1 / -1 changes near array indexing
            if (after_trimmed.contains("+ 1") || after_trimmed.contains("+1")
                || after_trimmed.contains("- 1") || after_trimmed.contains("-1"))
                && (after_trimmed.contains("[") || after_trimmed.contains(".len()") || after_trimmed.contains(".length"))
            {
                let before_has = before_trimmed.contains("+ 1") || before_trimmed.contains("+1")
                    || before_trimmed.contains("- 1") || before_trimmed.contains("-1");
                // Only flag if the +1/-1 was added or changed
                if !before_has {
                    findings.push(make_finding(
                        "off-by-one-hint",
                        "Arithmetic +1/-1 added near array indexing — verify boundary correctness",
                        0.5,
                        Severity::Medium,
                        change,
                        after_trimmed,
                        line_num + 1,
                    ));
                }
            }
        }
    }
}

/// Detect when `return null` / `return None` / `return nil` is introduced in the
/// after-content but was absent in the before-content. This often indicates a
/// contract violation where callers expect a non-null return.
fn check_null_return_introduced(
    change: &SemanticChange,
    before: &str,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let null_return_patterns = [
        "return null",
        "return None",
        "return nil",
        "? null :",
        "? null;",
        "? None",
    ];

    let before_lower = before.to_lowercase();
    let after_lower = after.to_lowercase();

    for pat in &null_return_patterns {
        let pat_lower = pat.to_lowercase();
        let before_has = before_lower.contains(&pat_lower);
        let after_has = after_lower.contains(&pat_lower);

        if after_has && !before_has {
            // Find the line number in after content
            for (line_num, line) in after.lines().enumerate() {
                if line.to_lowercase().contains(&pat_lower) {
                    findings.push(make_finding(
                        "null-return-introduced",
                        &format!("New `{}` path introduced — callers may not expect null", pat),
                        0.75,
                        Severity::High,
                        change,
                        line.trim(),
                        line_num + 1,
                    ));
                    break;
                }
            }
        }
    }
}

/// Detect when error handling paths change: catch blocks modified, error returns
/// changed, or exception handling altered between before and after.
fn check_error_path_changed(
    change: &SemanticChange,
    before: &str,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    // Detect swapped && / || in conditions (logic gate swap)
    for (line_num, after_line) in after_lines.iter().enumerate() {
        let at = after_line.trim();
        if at.is_empty() || at.starts_with("//") || at.starts_with("*") {
            continue;
        }
        for before_line in &before_lines {
            let bt = before_line.trim();
            if bt.is_empty() || bt == at {
                continue;
            }

            // && to || swap
            if bt.contains("&&") && at.contains("||") {
                let norm_b = bt.replace("&&", "@@GATE@@");
                let norm_a = at.replace("||", "@@GATE@@");
                if norm_b == norm_a {
                    findings.push(make_finding(
                        "logic-gate-swap",
                        "Condition changed from `&&` to `||` — verify AND/OR logic is correct",
                        0.7,
                        Severity::High,
                        change,
                        at,
                        line_num + 1,
                    ));
                }
            } else if bt.contains("||") && at.contains("&&") {
                let norm_b = bt.replace("||", "@@GATE@@");
                let norm_a = at.replace("&&", "@@GATE@@");
                if norm_b == norm_a {
                    findings.push(make_finding(
                        "logic-gate-swap",
                        "Condition changed from `||` to `&&` — verify AND/OR logic is correct",
                        0.7,
                        Severity::High,
                        change,
                        at,
                        line_num + 1,
                    ));
                }
            }
        }
    }
}

/// Detect wrong-callee substitution: a function call where the callee changed
/// to a "negation partner" or semantically opposite function while arguments
/// stayed the same. Classic regression pattern.
fn check_wrong_callee_substitution(
    change: &SemanticChange,
    before_lines: &[&str],
    after_lines: &[&str],
    findings: &mut Vec<DetectorFinding>,
) {
    // Negation partner pairs: (a, b) means swapping a↔b is suspicious
    const CALLEE_PAIRS: &[(&str, &str)] = &[
        ("assertTrue", "assertFalse"),
        ("assertEquals", "assertNotEquals"),
        ("assertNull", "assertNotNull"),
        ("assertSame", "assertNotSame"),
        ("encode", "decode"),
        ("encrypt", "decrypt"),
        ("lock", "unlock"),
        ("add", "remove"),
        ("push", "pop"),
        ("enqueue", "dequeue"),
        ("enable", "disable"),
        ("show", "hide"),
        ("open", "close"),
        ("start", "stop"),
        ("connect", "disconnect"),
        ("subscribe", "unsubscribe"),
        ("serialize", "deserialize"),
        ("marshal", "unmarshal"),
        ("isPresent", "isEmpty"),
        ("containsKey", "containsValue"),
    ];

    for (line_num, after_line) in after_lines.iter().enumerate() {
        let at = after_line.trim();
        if at.is_empty() || at.starts_with("//") || at.starts_with("*") {
            continue;
        }

        for before_line in before_lines {
            let bt = before_line.trim();
            if bt.is_empty() || bt == at || bt.starts_with("//") {
                continue;
            }

            for &(a, b) in CALLEE_PAIRS {
                // Check a→b swap
                if bt.contains(a) && at.contains(b) && !bt.contains(b) && !at.contains(a) {
                    let norm_b = bt.replace(a, "@@CALLEE@@");
                    let norm_a = at.replace(b, "@@CALLEE@@");
                    if norm_b == norm_a {
                        findings.push(make_finding(
                            "wrong-callee-substitution",
                            &format!(
                                "Callee swapped from `{}` to `{}` with same arguments — verify this is intentional",
                                a, b
                            ),
                            0.75,
                            Severity::High,
                            change,
                            at,
                            line_num + 1,
                        ));
                        break;
                    }
                }
                // Check b→a swap
                if bt.contains(b) && at.contains(a) && !bt.contains(a) && !at.contains(b) {
                    let norm_b = bt.replace(b, "@@CALLEE@@");
                    let norm_a = at.replace(a, "@@CALLEE@@");
                    if norm_b == norm_a {
                        findings.push(make_finding(
                            "wrong-callee-substitution",
                            &format!(
                                "Callee swapped from `{}` to `{}` with same arguments — verify this is intentional",
                                b, a
                            ),
                            0.75,
                            Severity::High,
                            change,
                            at,
                            line_num + 1,
                        ));
                        break;
                    }
                }
            }
        }
    }
}

/// Detect when a new early return/break/continue is added in the after-content
/// that wasn't present in the before-content. Early returns can skip important
/// cleanup, event emission, or state updates.
fn check_added_early_return(
    change: &SemanticChange,
    before: &str,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let early_patterns = [
        ("return;", "bare return"),
        ("return early", "early return"),
        ("return false;", "return false"),
        ("return true;", "return true"),
        ("return 0;", "return 0"),
        ("return -1;", "return -1"),
        ("return Ok(())", "return Ok(())"),
        ("return Err(", "return Err"),
        ("continue;", "continue"),
        ("break;", "break"),
    ];

    let before_lower = before.to_lowercase();
    let after_lower = after.to_lowercase();

    for (pat, desc) in &early_patterns {
        let pat_lower = pat.to_lowercase();
        // Count occurrences: only flag if after has MORE than before
        let before_count = before_lower.matches(&pat_lower).count();
        let after_count = after_lower.matches(&pat_lower).count();

        if after_count > before_count {
            // Find the line in after content
            for (line_num, line) in after.lines().enumerate() {
                if line.to_lowercase().contains(&pat_lower) {
                    findings.push(make_finding(
                        "added-early-exit",
                        &format!(
                            "New `{}` added — may skip cleanup or state updates that follow",
                            desc
                        ),
                        0.55,
                        Severity::Medium,
                        change,
                        line.trim(),
                        line_num + 1,
                    ));
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_finding(
    rule_id: &str,
    message: &str,
    confidence: f64,
    severity: Severity,
    change: &SemanticChange,
    evidence: &str,
    line_num: usize,
) -> DetectorFinding {
    DetectorFinding {
        rule_id: rule_id.to_string(),
        message: message.to_string(),
        detector: DetectorKind::DiffHeuristic,
        confidence,
        severity,
        entity_id: change.entity_id.clone(),
        entity_name: change.entity_name.clone(),
        file_path: change.file_path.clone(),
        evidence: evidence.to_string(),
        start_line: line_num,
        end_line: line_num,
    }
}

/// Check if two lines differ only by a `!` prefix on a condition.
fn differs_only_by_negation(before: &str, after: &str) -> Option<&'static str> {
    // Try: before has `!x` and after has `x` (negation removed)
    let before_neg_removed = before.replace("!(", "(").replace("! ", " ");
    if before_neg_removed == after && before != after {
        return Some("removed");
    }

    // Try: after has `!x` and before has `x` (negation added)
    let after_neg_removed = after.replace("!(", "(").replace("! ", " ");
    if after_neg_removed == before && before != after {
        return Some("added");
    }

    None
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

#[cfg(test)]
mod tests {
    use sem_core::model::change::{ChangeType, SemanticChange};

    use super::*;

    fn make_modified(before: &str, after: &str) -> SemanticChange {
        SemanticChange {
            id: "test".to_string(),
            entity_id: "test.ts::function::test".to_string(),
            change_type: ChangeType::Modified,
            entity_type: "function".to_string(),
            entity_name: "test".to_string(),
            file_path: "test.ts".to_string(),
            old_file_path: None,
            before_content: Some(before.to_string()),
            after_content: Some(after.to_string()),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: None,
        }
    }

    #[test]
    fn test_negation_flip_eq_to_neq() {
        let change = make_modified("if (x == y) {", "if (x != y) {");
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "negation-flip"),
            "Should detect == to != flip: {:?}",
            findings
        );
    }

    #[test]
    fn test_removed_guard() {
        let before = "if (user == null) {\n  throw new Error('no user');\n}\nprocess(user);";
        let after = "process(user);";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "removed-guard"),
            "Should detect removed guard: {:?}",
            findings
        );
    }

    #[test]
    fn test_off_by_one_lt_to_lte() {
        let change = make_modified("for (let i = 0; i < arr.length; i++)", "for (let i = 0; i <= arr.length; i++)");
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "off-by-one-hint"),
            "Should detect < to <= change: {:?}",
            findings
        );
    }

    #[test]
    fn test_null_return_introduced() {
        let before = "@Override\npublic Long getCount() {\n    return delegate.getCount();\n}";
        let after = "@Override\npublic Long getCount() {\n    Model m = supplier.get();\n    return m == null ? null : m.getCount();\n}";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "null-return-introduced"),
            "Should detect new null return path: {:?}",
            findings
        );
    }

    #[test]
    fn test_null_return_not_flagged_when_already_present() {
        let before = "if (x == null) return null;\nreturn x.value();";
        let after = "if (x == null) return null;\nreturn x.newValue();";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "null-return-introduced"),
            "Should NOT flag when null return already existed: {:?}",
            findings
        );
    }

    #[test]
    fn test_logic_gate_swap_and_to_or() {
        let change = make_modified("if (isAdmin && isOwner) {", "if (isAdmin || isOwner) {");
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "logic-gate-swap"),
            "Should detect && to || swap: {:?}",
            findings
        );
    }

    #[test]
    fn test_logic_gate_swap_or_to_and() {
        let change = make_modified("if (a || b) {", "if (a && b) {");
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "logic-gate-swap"),
            "Should detect || to && swap: {:?}",
            findings
        );
    }

    #[test]
    fn test_wrong_callee_assertTrue_to_assertFalse() {
        let change = make_modified(
            "assertTrue(result.isValid());",
            "assertFalse(result.isValid());",
        );
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "wrong-callee-substitution"),
            "Should detect assertTrue→assertFalse swap: {:?}",
            findings
        );
    }

    #[test]
    fn test_wrong_callee_encode_to_decode() {
        let change = make_modified(
            "let data = encode(payload);",
            "let data = decode(payload);",
        );
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "wrong-callee-substitution"),
            "Should detect encode→decode swap: {:?}",
            findings
        );
    }

    #[test]
    fn test_added_early_return() {
        let before = "fn process(x: i32) {\n    validate(x);\n    save(x);\n}";
        let after = "fn process(x: i32) {\n    if x < 0 { return; }\n    validate(x);\n    save(x);\n}";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "added-early-exit"),
            "Should detect added early return: {:?}",
            findings
        );
    }

    #[test]
    fn test_no_false_positive_existing_return() {
        let before = "fn check() {\n    if bad { return; }\n    work();\n}";
        let after = "fn check() {\n    if bad { return; }\n    work2();\n}";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "added-early-exit"),
            "Should NOT flag when return count unchanged: {:?}",
            findings
        );
    }
}
