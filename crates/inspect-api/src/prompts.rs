use inspect_core::types::EntityReview;
use sem_core::model::change::ChangeType;

pub const SYSTEM_REVIEW: &str = "You are a precise code reviewer. Only report real bugs you are confident about. Always respond with valid JSON.";

pub const SYSTEM_VALIDATE: &str = "You are a precise reviewer. Verify each issue against the actual diff. Only keep confirmed bugs. Always respond with valid JSON.";

pub const PROMPT_DEEP: &str = r#"You are a world-class code reviewer. Review this PR and find ONLY real, concrete bugs.

PR Title: {pr_title}

{triage_section}

PR Diff:
{diff}

Look specifically for these categories of issues:
1. Logic errors: wrong conditions, off-by-one, incorrect algorithms, broken control flow, inverted booleans
2. Concurrency bugs: race conditions, missing locks, unsafe shared state, deadlocks, unhandled async promises
3. Null/undefined safety: missing null checks, possible NPE, Optional.get() without isPresent(), uninitialized variables
4. Error handling: swallowed exceptions, missing error propagation, wrong error types
5. Data correctness: wrong translations, wrong constants, incorrect mappings, copy-paste errors, stale cache data
6. Security: SSRF, XSS, injection, auth bypass, exposed secrets, unsafe deserialization, origin validation bypass
7. Type mismatches: wrong return types, incompatible casts, API contract violations, schema errors
8. Breaking changes: removed public APIs without migration, changed behavior silently
9. State consistency: asymmetric cache trust, orphaned data, inconsistent updates across related fields
10. Naming/contract bugs: method name typos that break interfaces, property names that don't match expected contracts

Rules:
- ONLY report issues you are highly confident about (>90% sure)
- Be specific: name the file, function/variable, and exactly what's wrong
- Naming typos ARE bugs if they would cause a runtime error or break an API contract
- Do NOT report: style preferences, missing tests, docs, "could be improved"
- Do NOT report issues about code that was only deleted/removed
- Maximum 10 issues. Quality over quantity.

For each issue, provide it as a JSON object with "issue" (description) and "evidence" (quote the specific code lines from the diff that prove this is a bug).

Respond with ONLY a JSON object:
{{"issues": [{{"issue": "description", "evidence": "the specific code"}}]}}"#;

pub const PROMPT_VALIDATE: &str = r#"You are a senior code reviewer doing final validation. You have the PR diff and candidate issues.

PR Title: {pr_title}

PR Diff (for verification):
{diff}

Candidate Issues:
{candidates}

For each candidate, verify against the actual diff:
1. Can you find the specific code that's buggy? If yes, keep it.
2. Is this a real bug that would cause incorrect behavior in production? If yes, keep it.
3. Is this about deleted/removed code being replaced? If so, DROP it.
4. Is this speculative or theoretical ("could potentially...")? If so, DROP it.
5. Is this about style, naming conventions, or missing tests? If so, DROP it.

Return ONLY the issues that are verified real bugs with evidence in the diff.

Respond with ONLY a JSON object:
{{"issues": ["verified issue 1", "verified issue 2", ...]}}"#;

/// Smart diff truncation that deprioritizes tests, docs, configs.
pub fn truncate_diff(diff: &str, max_chars: usize) -> String {
    if diff.len() <= max_chars {
        return diff.to_string();
    }

    let parts: Vec<&str> = diff.split("diff --git ").collect();
    if parts.is_empty() {
        return diff[..max_chars].to_string();
    }

    let mut scored: Vec<(f64, &str)> = Vec::new();
    for part in &parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let adds = part.matches("\n+").count().saturating_sub(part.matches("\n+++").count());
        let dels = part.matches("\n-").count().saturating_sub(part.matches("\n---").count());
        let mod_bonus = adds.min(dels) * 2;
        let mut score = (adds + dels + mod_bonus) as f64;

        let first_line = part.lines().next().unwrap_or("").to_lowercase();

        // Deprioritize test files
        if ["test", "spec", "mock", "__test__", "fixture"]
            .iter()
            .any(|kw| first_line.contains(kw))
        {
            score *= 0.3;
        }
        // Deprioritize docs
        if [".md", ".adoc", ".txt", ".rst", "changelog", "readme"]
            .iter()
            .any(|kw| first_line.contains(kw))
        {
            score *= 0.2;
        }
        // Deprioritize snapshots/lockfiles
        if [".snap", ".lock", "package-lock", "yarn.lock"]
            .iter()
            .any(|kw| first_line.contains(kw))
        {
            score *= 0.1;
        }
        // Deprioritize config files
        if [".json", ".yaml", ".yml", ".toml", ".xml"]
            .iter()
            .any(|kw| first_line.contains(kw))
        {
            score *= 0.5;
        }

        scored.push((score, part));
    }

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut result = String::new();
    for (_, part) in &scored {
        let candidate = format!("diff --git {}", part);
        if result.len() + candidate.len() > max_chars {
            break;
        }
        result.push_str(&candidate);
    }

    if result.is_empty() {
        diff[..max_chars].to_string()
    } else {
        result
    }
}

/// Build entity-grouped triage section from entity reviews.
pub fn build_rich_triage(entities: &[EntityReview]) -> String {
    if entities.is_empty() {
        return String::new();
    }

    let mut meaningful: Vec<&EntityReview> = entities
        .iter()
        .filter(|e| {
            matches!(
                e.change_type,
                ChangeType::Modified | ChangeType::Added
            ) && e.entity_type != "chunk"
        })
        .collect();

    meaningful.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap_or(std::cmp::Ordering::Equal));
    let top: Vec<&EntityReview> = meaningful.into_iter().take(20).collect();

    if top.is_empty() {
        return String::new();
    }

    // Group by file
    let mut by_file: std::collections::HashMap<&str, Vec<&EntityReview>> =
        std::collections::HashMap::new();
    for e in &top {
        by_file.entry(e.file_path.as_str()).or_default().push(e);
    }

    let mut file_entries: Vec<(&str, Vec<&EntityReview>)> = by_file.into_iter().collect();
    file_entries.sort_by(|a, b| {
        let a_max = a.1.iter().map(|e| e.risk_score).fold(0.0_f64, f64::max);
        let b_max = b.1.iter().map(|e| e.risk_score).fold(0.0_f64, f64::max);
        b_max.partial_cmp(&a_max).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines = vec!["## Entity-level triage (highest-risk changes):".to_string()];
    for (fp, ents) in &file_entries {
        lines.push(format!("\n**{}**:", fp));
        for e in ents {
            let public = if e.is_public_api { " [PUBLIC API]" } else { "" };
            lines.push(format!(
                "  - {} ({}, {:?}, {}){public}",
                e.entity_name, e.entity_type, e.change_type, e.classification,
            ));
        }
    }

    lines.join("\n")
}

/// Format the PROMPT_DEEP template with actual values.
pub fn format_deep_prompt(pr_title: &str, triage_section: &str, diff: &str) -> String {
    PROMPT_DEEP
        .replace("{pr_title}", pr_title)
        .replace("{triage_section}", triage_section)
        .replace("{diff}", diff)
}

/// Format the PROMPT_VALIDATE template with actual values.
pub fn format_validate_prompt(pr_title: &str, diff: &str, candidates: &str) -> String {
    PROMPT_VALIDATE
        .replace("{pr_title}", pr_title)
        .replace("{diff}", diff)
        .replace("{candidates}", candidates)
}
