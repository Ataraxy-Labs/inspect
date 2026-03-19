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
        check_argument_order_swap(change, &before_lines, &after_lines, &mut findings);
        check_boolean_polarity_flip(change, before, after, &mut findings);
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

/// Detect if/while condition polarity flips where branch bodies remain unchanged.
/// This is stricter than line-based negation checks and aims to reduce noise.
fn check_boolean_polarity_flip(
    change: &SemanticChange,
    before: &str,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    for (i, pair) in before_lines.windows(2).enumerate() {
        let b_head = pair[0].trim();
        let b_next = pair[1].trim();

        if !(b_head.starts_with("if ") || b_head.starts_with("if(") || b_head.starts_with("while ") || b_head.starts_with("while(")) {
            continue;
        }
        if b_next != "{" {
            continue;
        }

        if i + 1 >= after_lines.len() {
            continue;
        }
        let a_head = after_lines[i].trim();
        let a_next = after_lines[i + 1].trim();
        if a_next != "{" {
            continue;
        }

        let b_cond = extract_condition_expr(b_head);
        let a_cond = extract_condition_expr(a_head);
        let (Some(b_cond), Some(a_cond)) = (b_cond, a_cond) else {
            continue;
        };

        if !are_boolean_negations(&b_cond, &a_cond) {
            continue;
        }

        // Validate that the block body itself is unchanged nearby.
        if !neighboring_block_body_unchanged(&before_lines, &after_lines, i + 1, 4) {
            continue;
        }

        findings.push(make_finding(
            "boolean-polarity-flip",
            "Condition polarity flipped without branch-body changes — verify intent",
            0.72,
            Severity::High,
            change,
            a_head,
            i + 1,
        ));
    }
}

/// Detect suspicious call-site argument reordering (e.g. `foo(a, b)` -> `foo(b, a)`).
/// This flags positional-argument swaps where callee and argument set are unchanged.
fn check_argument_order_swap(
    change: &SemanticChange,
    before_lines: &[&str],
    after_lines: &[&str],
    findings: &mut Vec<DetectorFinding>,
) {
    let paired = before_lines.len().min(after_lines.len());
    for line_num in 0..paired {
        let before_line = before_lines[line_num].trim();
        let after_line = after_lines[line_num].trim();
        if before_line.is_empty() || after_line.is_empty() || before_line == after_line {
            continue;
        }

        let Some((before_callee, before_args)) = parse_simple_call(before_line) else {
            continue;
        };
        let Some((after_callee, after_args)) = parse_simple_call(after_line) else {
            continue;
        };

        if before_callee != after_callee
            || before_args.len() != after_args.len()
            || before_args.len() < 2
            || before_args == after_args
        {
            continue;
        }

        if before_args.iter().any(|a| looks_named_argument(a))
            || after_args.iter().any(|a| looks_named_argument(a))
        {
            continue;
        }

        let mut before_sorted = before_args.clone();
        let mut after_sorted = after_args.clone();
        before_sorted.sort();
        after_sorted.sort();
        if before_sorted != after_sorted {
            continue;
        }

        let Some((left, right)) = detect_single_swap(&before_args, &after_args) else {
            continue;
        };

        findings.push(make_finding(
            "argument-order-swap",
            &format!(
                "Call arguments swapped for `{}`: `{}` <-> `{}` — verify parameter order",
                before_callee, before_args[left], before_args[right]
            ),
            0.68,
            Severity::High,
            change,
            after_line,
            line_num + 1,
        ));
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

fn parse_simple_call(line: &str) -> Option<(String, Vec<String>)> {
    if line.starts_with("if ")
        || line.starts_with("if(")
        || line.starts_with("while ")
        || line.starts_with("while(")
        || line.starts_with("for ")
        || line.starts_with("for(")
        || line.starts_with("switch ")
        || line.starts_with("switch(")
        || line.starts_with("return ")
    {
        return None;
    }

    let open = line.find('(')?;
    let close = find_matching_paren(line, open)?;
    if close <= open {
        return None;
    }

    let callee_raw = line[..open].trim();
    if callee_raw.is_empty() {
        return None;
    }
    let callee = extract_callee_token(callee_raw)?;

    let args_src = &line[open + 1..close];
    let args = split_top_level_args(args_src)
        .into_iter()
        .map(|arg| normalize_inline_whitespace(arg.trim()))
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();

    Some((callee.to_string(), args))
}

fn extract_callee_token(s: &str) -> Option<&str> {
    let mut end = s.len();
    while end > 0 {
        let ch = s[..end].chars().next_back()?;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == ':' {
            break;
        }
        end -= ch.len_utf8();
    }
    if end == 0 {
        return None;
    }

    let mut start = end;
    while start > 0 {
        let ch = s[..start].chars().next_back()?;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == ':' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }

    let token = &s[start..end];
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn find_matching_paren(line: &str, open_idx: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in line.char_indices().skip(open_idx) {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn split_top_level_args(args: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    for (idx, ch) in args.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if !in_double_quote && ch == '\'' {
            in_single_quote = !in_single_quote;
            continue;
        }
        if !in_single_quote && ch == '"' {
            in_double_quote = !in_double_quote;
            continue;
        }
        if in_single_quote || in_double_quote {
            continue;
        }

        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                out.push(&args[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(&args[start..]);
    out
}

fn normalize_inline_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_named_argument(arg: &str) -> bool {
    arg.contains("=")
        && !arg.contains("==")
        && !arg.contains("!=")
        && !arg.contains("<=")
        && !arg.contains(">=")
}

fn detect_single_swap(before: &[String], after: &[String]) -> Option<(usize, usize)> {
    let mismatches: Vec<usize> = (0..before.len())
        .filter(|&i| before[i] != after[i])
        .collect();

    if mismatches.len() != 2 {
        return None;
    }

    let i = mismatches[0];
    let j = mismatches[1];
    if before[i] == after[j] && before[j] == after[i] {
        Some((i, j))
    } else {
        None
    }
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

fn extract_condition_expr(head: &str) -> Option<String> {
    let start = head.find('(')?;
    let end = head.rfind(')')?;
    if end <= start {
        return None;
    }
    Some(head[start + 1..end].trim().to_string())
}

fn are_boolean_negations(a: &str, b: &str) -> bool {
    normalized_condition(a) == negate_condition(&normalized_condition(b))
        || normalized_condition(b) == negate_condition(&normalized_condition(a))
}

fn normalized_condition(cond: &str) -> String {
    cond
        .replace("==", " @@EQ@@ ")
        .replace("!=", " @@NEQ@@ ")
        .replace("<=", " @@LE@@ ")
        .replace(">=", " @@GE@@ ")
        .replace('<', " @@LT@@ ")
        .replace('>', " @@GT@@ ")
        .replace("&&", " @@AND@@ ")
        .replace("||", " @@OR@@ ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn negate_condition(cond: &str) -> String {
    if let Some(stripped) = cond.strip_prefix('!') {
        return stripped.trim().to_string();
    }

    if let Some(swapped) = swap_cmp(cond, " @@EQ@@ ", " @@NEQ@@ ") {
        return swapped;
    }
    if let Some(swapped) = swap_cmp(cond, " @@LT@@ ", " @@GE@@ ") {
        return swapped;
    }
    if let Some(swapped) = swap_cmp(cond, " @@GT@@ ", " @@LE@@ ") {
        return swapped;
    }

    format!("!{}", cond)
}

fn swap_cmp(cond: &str, a: &str, b: &str) -> Option<String> {
    if cond.contains(a) {
        Some(cond.replace(a, b))
    } else if cond.contains(b) {
        Some(cond.replace(b, a))
    } else {
        None
    }
}

fn neighboring_block_body_unchanged(before_lines: &[&str], after_lines: &[&str], brace_line: usize, max_lines: usize) -> bool {
    let mut checked = 0usize;
    for offset in 1..=max_lines {
        let i = brace_line + offset;
        if i >= before_lines.len() || i >= after_lines.len() {
            break;
        }
        let b = before_lines[i].trim();
        let a = after_lines[i].trim();
        if b == "}" || a == "}" {
            break;
        }
        checked += 1;
        if b != a {
            return false;
        }
    }
    checked > 0
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

    #[test]
    fn test_boolean_polarity_flip_detected() {
        let before = "if (count == 0)\n{\n    run();\n}";
        let after = "if (count != 0)\n{\n    run();\n}";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "boolean-polarity-flip"),
            "Should detect boolean polarity flip: {:?}",
            findings
        );
    }

    #[test]
    fn test_boolean_polarity_flip_not_flagged_when_body_changes() {
        let before = "if (count == 0)\n{\n    run();\n}";
        let after = "if (count != 0)\n{\n    stop();\n}";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "boolean-polarity-flip"),
            "Should not detect polarity flip when body also changed: {:?}",
            findings
        );
    }

    #[test]
    fn test_argument_order_swap_detected() {
        let before = "notify(userId, projectId);";
        let after = "notify(projectId, userId);";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "argument-order-swap"),
            "Should detect argument-order swap: {:?}",
            findings
        );
    }

    #[test]
    fn test_argument_order_swap_not_flagged_for_callee_change() {
        let before = "notify(userId, projectId);";
        let after = "emit(projectId, userId);";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "argument-order-swap"),
            "Should not detect argument-order swap when callee changed: {:?}",
            findings
        );
    }

    #[test]
    fn test_argument_order_swap_not_flagged_for_named_args() {
        let before = "notify(user_id=userId, project_id=projectId);";
        let after = "notify(project_id=projectId, user_id=userId);";
        let change = make_modified(before, after);
        let findings = run_diff_heuristics(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "argument-order-swap"),
            "Should not detect argument-order swap for named args: {:?}",
            findings
        );
    }

}
