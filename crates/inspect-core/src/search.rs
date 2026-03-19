use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SearchMatch {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Grep in-memory file contents (used for remote/GitHub-fetched files).
pub fn grep_files(
    files: &[(String, String)],
    pattern: &str,
    case_sensitive: bool,
    context_lines: usize,
) -> Vec<SearchMatch> {
    let pattern_lower = if case_sensitive {
        pattern.to_string()
    } else {
        pattern.to_lowercase()
    };
    let mut matches = Vec::new();

    for (filepath, content) in files {
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let haystack = if case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };
            if haystack.contains(&pattern_lower) {
                let start = i.saturating_sub(context_lines);
                let end = (i + context_lines + 1).min(lines.len());
                matches.push(SearchMatch {
                    file: filepath.clone(),
                    line: i + 1,
                    column: haystack.find(&pattern_lower).unwrap_or(0) + 1,
                    text: line.to_string(),
                    context_before: lines[start..i].iter().map(|s| s.to_string()).collect(),
                    context_after: lines[i + 1..end].iter().map(|s| s.to_string()).collect(),
                });
            }
        }
    }
    matches
}

/// SIMD-accelerated grep on local filesystem files using fff-search.
/// Uses FilePicker to index the repo and then performs grep_search with
/// SIMD-optimized plain text or regex matching.
pub fn grep_local(
    repo_path: &std::path::Path,
    pattern: &str,
    case_sensitive: bool,
    context_lines: usize,
    regex: bool,
) -> Vec<SearchMatch> {
    use fff_search::file_picker::FilePicker;
    use fff_search::grep::{grep_search, GrepMode, GrepSearchOptions};
    use fff_search::{FFFMode, QueryParser, SharedFrecency, SharedPicker};

    let shared_picker: SharedPicker = Default::default();
    let shared_frecency: SharedFrecency = Default::default();

    if FilePicker::new_with_shared_state(
        repo_path.to_string_lossy().into_owned(),
        true, // warmup mmap caches for grep
        FFFMode::Ai,
        shared_picker.clone(),
        shared_frecency,
    )
    .is_err()
    {
        return grep_local_fallback(repo_path, pattern, case_sensitive, context_lines);
    }

    FilePicker::wait_for_scan(&shared_picker);

    let picker_guard = shared_picker.read().unwrap();
    let picker = match picker_guard.as_ref() {
        Some(p) => p,
        None => return vec![],
    };

    let parser = QueryParser::default();
    let query = parser.parse(pattern);

    let options = GrepSearchOptions {
        max_file_size: 10 * 1024 * 1024, // 10MB
        max_matches_per_file: 50,
        smart_case: !case_sensitive,
        file_offset: 0,
        page_limit: 500,
        mode: if regex { GrepMode::Regex } else { GrepMode::PlainText },
        time_budget_ms: 5000,
        before_context: context_lines,
        after_context: context_lines,
        classify_definitions: true,
    };

    let result = grep_search(picker.get_files(), &query, &options);

    result
        .matches
        .into_iter()
        .map(|m| {
            let file = result
                .files
                .get(m.file_index)
                .map(|f| f.relative_path.clone())
                .unwrap_or_default();
            SearchMatch {
                file,
                line: m.line_number as usize,
                column: m.col + 1,
                text: m.line_content,
                context_before: m.context_before,
                context_after: m.context_after,
            }
        })
        .collect()
}

/// Fallback for grep_local when fff-search can't initialize.
fn grep_local_fallback(
    repo_path: &std::path::Path,
    pattern: &str,
    case_sensitive: bool,
    context_lines: usize,
) -> Vec<SearchMatch> {
    let output = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(repo_path)
        .output();

    let files: Vec<(String, String)> = match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .filter_map(|f| {
                    let path = repo_path.join(f);
                    std::fs::read_to_string(&path)
                        .ok()
                        .map(|content| (f.to_string(), content))
                })
                .collect()
        }
        _ => return vec![],
    };

    grep_files(&files, pattern, case_sensitive, context_lines)
}

pub fn format_matches(matches: &[SearchMatch]) -> String {
    if matches.is_empty() {
        return "No matches found.".to_string();
    }

    let mut lines = Vec::new();
    let mut last_file = "";

    for m in matches {
        if m.file != last_file {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            last_file = &m.file;
        }

        for (j, ctx) in m.context_before.iter().enumerate() {
            let ctx_line = m.line - m.context_before.len() + j;
            lines.push(format!("{}:{}- {}", m.file, ctx_line, ctx));
        }

        lines.push(format!("{}:{}:{}", m.file, m.line, m.text));

        for (j, ctx) in m.context_after.iter().enumerate() {
            lines.push(format!("{}:{}- {}", m.file, m.line + 1 + j, ctx));
        }
    }

    lines.push(format!(
        "\n{} matches across {} files",
        matches.len(),
        {
            let mut files: Vec<&str> = matches.iter().map(|m| m.file.as_str()).collect();
            files.sort();
            files.dedup();
            files.len()
        }
    ));

    lines.join("\n")
}

/// Fuzzy-match an entity name against a list of candidates.
/// Returns the best match if the score exceeds a minimum threshold.
pub fn fuzzy_find_entity<'a>(
    query: &str,
    candidates: impl Iterator<Item = &'a (String, String)>,
) -> Option<(String, String)> {
    let query_lower = query.to_lowercase();
    let mut best: Option<(usize, String, String)> = None;

    for (name, file) in candidates {
        let name_lower = name.to_lowercase();

        // Exact match — return immediately
        if name_lower == query_lower {
            return Some((name.clone(), file.clone()));
        }

        // Simple subsequence score: count matching chars in order
        let mut score = 0usize;
        let mut qi = query_lower.chars().peekable();
        for ch in name_lower.chars() {
            if qi.peek() == Some(&ch) {
                score += 1;
                qi.next();
            }
        }

        // Must match all query chars as subsequence
        if qi.peek().is_none() && score > 0 {
            // Prefer shorter names (tighter match)
            let adjusted = score * 100 / name.len().max(1);
            if best.as_ref().map_or(true, |(s, _, _)| adjusted > *s) {
                best = Some((adjusted, name.clone(), file.clone()));
            }
        }
    }

    // Require at least 50% density
    best.filter(|(score, _, _)| *score >= 50)
        .map(|(_, name, file)| (name, file))
}
