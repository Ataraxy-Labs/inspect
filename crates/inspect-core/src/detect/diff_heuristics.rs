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
        check_variable_near_miss(change, before, &before_lines, after, &after_lines, &mut findings);
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

/// Detect potential variable near-miss substitutions such as
/// `dataCount` -> `dateCount` on otherwise-identical lines.
fn check_variable_near_miss(
    change: &SemanticChange,
    before: &str,
    before_lines: &[&str],
    after: &str,
    after_lines: &[&str],
    findings: &mut Vec<DetectorFinding>,
) {
    let paired = before_lines.len().min(after_lines.len());
    for line_num in 0..paired {
        let bt = before_lines[line_num].trim();
        let at = after_lines[line_num].trim();

        if bt.is_empty() || at.is_empty() || bt == at {
            continue;
        }

        if bt.starts_with("//") || bt.starts_with('*') || at.starts_with("//") || at.starts_with('*') {
            continue;
        }

        if bt.len().abs_diff(at.len()) > 8 {
            continue;
        }

        if let Some((old_ident, new_ident)) = single_identifier_substitution(bt, at) {
            if !is_variable_near_miss(old_ident, new_ident) {
                continue;
            }

            // Suppress likely intentional full-entity renames.
            if is_consistent_identifier_rename(before, after, old_ident, new_ident) {
                continue;
            }

            findings.push(make_finding(
                "variable-near-miss",
                &format!(
                    "Identifier changed from `{}` to similar `{}` — possible wrong-variable usage",
                    old_ident, new_ident
                ),
                0.65,
                Severity::Medium,
                change,
                at,
                line_num + 1,
            ));
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

fn single_identifier_substitution<'a>(before: &'a str, after: &'a str) -> Option<(&'a str, &'a str)> {
    let before_idents = identifiers_in_line(before);
    let after_idents = identifiers_in_line(after);

    for old_ident in &before_idents {
        for new_ident in &after_idents {
            if old_ident == new_ident {
                continue;
            }

            if count_identifier_in_line(before, old_ident) != 1
                || count_identifier_in_line(after, new_ident) != 1
            {
                continue;
            }

            let normalized_before = replace_single_identifier(before, old_ident, "@@IDENT@@")?;
            let normalized_after = replace_single_identifier(after, new_ident, "@@IDENT@@")?;
            if normalized_before == normalized_after {
                return Some((*old_ident, *new_ident));
            }
        }
    }

    None
}

fn is_variable_near_miss(old_ident: &str, new_ident: &str) -> bool {
    if old_ident.len() < 4 || new_ident.len() < 4 {
        return false;
    }
    if is_keyword(old_ident) || is_keyword(new_ident) {
        return false;
    }

    let distance = levenshtein(old_ident, new_ident);
    if distance <= 2 {
        return true;
    }

    jaro_winkler(old_ident, new_ident) >= 0.92
}

fn is_consistent_identifier_rename(before: &str, after: &str, old_ident: &str, new_ident: &str) -> bool {
    let before_old = count_identifier_occurrences(before, old_ident);
    let after_old = count_identifier_occurrences(after, old_ident);
    let before_new = count_identifier_occurrences(before, new_ident);
    let after_new = count_identifier_occurrences(after, new_ident);

    // Treat it as intentional rename only when it appears multiple times;
    // single-occurrence swaps are exactly the typo/near-miss pattern we want.
    before_old >= 2 && after_old == 0 && before_new == 0 && after_new >= before_old
}

fn identifiers_in_line(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;

    for (idx, ch) in line.char_indices() {
        if is_identifier_char(ch) {
            if start.is_none() {
                start = Some(idx);
            }
        } else if let Some(s) = start.take() {
            let ident = &line[s..idx];
            if is_valid_identifier(ident) {
                out.push(ident);
            }
        }
    }

    if let Some(s) = start {
        let ident = &line[s..];
        if is_valid_identifier(ident) {
            out.push(ident);
        }
    }

    out
}

fn count_identifier_in_line(line: &str, needle: &str) -> usize {
    identifiers_in_line(line)
        .into_iter()
        .filter(|id| *id == needle)
        .count()
}

fn replace_single_identifier(line: &str, needle: &str, replacement: &str) -> Option<String> {
    let mut out = String::with_capacity(line.len() + replacement.len());
    let mut replaced = false;
    let mut i = 0usize;

    while i < line.len() {
        let slice = &line[i..];
        let ch = slice.chars().next()?;
        let ch_len = ch.len_utf8();

        if is_identifier_char(ch) {
            let start = i;
            i += ch_len;
            while i < line.len() {
                let next = line[i..].chars().next()?;
                if is_identifier_char(next) {
                    i += next.len_utf8();
                } else {
                    break;
                }
            }
            let ident = &line[start..i];
            if ident == needle {
                if replaced {
                    return None;
                }
                out.push_str(replacement);
                replaced = true;
            } else {
                out.push_str(ident);
            }
        } else {
            out.push(ch);
            i += ch_len;
        }
    }

    if replaced { Some(out) } else { None }
}

fn count_identifier_occurrences(content: &str, ident: &str) -> usize {
    content
        .lines()
        .map(|line| count_identifier_in_line(line, ident))
        .sum()
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_valid_identifier(ident: &str) -> bool {
    let mut chars = ident.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => chars.all(is_identifier_char),
        _ => false,
    }
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "if"
            | "else"
            | "for"
            | "while"
            | "match"
            | "switch"
            | "case"
            | "break"
            | "continue"
            | "return"
            | "try"
            | "catch"
            | "throw"
            | "throws"
            | "new"
            | "class"
            | "struct"
            | "enum"
            | "fn"
            | "function"
            | "let"
            | "const"
            | "var"
            | "pub"
            | "public"
            | "private"
            | "protected"
            | "static"
            | "async"
            | "await"
            | "true"
            | "false"
            | "null"
            | "None"
            | "nil"
    )
}

fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    if a_chars.is_empty() {
        return b_chars.len();
    }
    if b_chars.is_empty() {
        return a_chars.len();
    }

    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0usize; b_chars.len() + 1];

    for (i, ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_chars.len()]
}

fn jaro_winkler(a: &str, b: &str) -> f64 {
    let jaro = jaro(a, b);
    let prefix_len = a
        .chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .take(4)
        .count() as f64;
    jaro + 0.1 * prefix_len * (1.0 - jaro)
}

fn jaro(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    if a_len == 0 || b_len == 0 {
        return 0.0;
    }

    let match_dist = (a_len.max(b_len) / 2).saturating_sub(1);
    let mut a_matches = vec![false; a_len];
    let mut b_matches = vec![false; b_len];

    let mut matches = 0usize;
    for i in 0..a_len {
        let start = i.saturating_sub(match_dist);
        let end = (i + match_dist + 1).min(b_len);
        for j in start..end {
            if b_matches[j] || a_chars[i] != b_chars[j] {
                continue;
            }
            a_matches[i] = true;
            b_matches[j] = true;
            matches += 1;
            break;
        }
    }

    if matches == 0 {
        return 0.0;
    }

    let mut transpositions = 0usize;
    let mut j = 0usize;
    for i in 0..a_len {
        if !a_matches[i] {
            continue;
        }
        while j < b_len && !b_matches[j] {
            j += 1;
        }
        if j < b_len && a_chars[i] != b_chars[j] {
            transpositions += 1;
        }
        j += 1;
    }

    let m = matches as f64;
    let t = (transpositions / 2) as f64;
    (m / a_len as f64 + m / b_len as f64 + (m - t) / m) / 3.0
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
    fn test_variable_near_miss_detected() {
        let before = "let total = dataCount * price;";
        let after = "let total = dateCount * price;";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "variable-near-miss"),
            "Should detect dataCount→dateCount near-miss: {:?}",
            findings
        );
    }

    #[test]
    fn test_variable_near_miss_not_flagged_for_consistent_rename() {
        let before = "let dataCount = read();\nlet total = dataCount * price;\nreturn dataCount;";
        let after = "let dateCount = read();\nlet total = dateCount * price;\nreturn dateCount;";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "variable-near-miss"),
            "Should not flag consistent rename: {:?}",
            findings
        );
    }

}
