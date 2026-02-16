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
