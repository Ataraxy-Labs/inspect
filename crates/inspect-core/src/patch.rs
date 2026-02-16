use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct DiffLine {
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    pub kind: String,
    pub content: String,
    pub commentable: bool,
}

#[derive(Debug, Serialize)]
pub struct DiffHunk {
    pub old_start: u64,
    pub old_count: u64,
    pub new_start: u64,
    pub new_count: u64,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

pub fn parse_patch(patch: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut old_line: u64 = 0;
    let mut new_line: u64 = 0;

    for raw_line in patch.lines() {
        if raw_line.starts_with("@@") {
            if let Some(h) = current_hunk.take() {
                hunks.push(h);
            }

            let (os, oc, ns, nc) = parse_hunk_header(raw_line);
            old_line = os;
            new_line = ns;

            current_hunk = Some(DiffHunk {
                old_start: os,
                old_count: oc,
                new_start: ns,
                new_count: nc,
                header: raw_line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }

        let Some(hunk) = current_hunk.as_mut() else {
            continue;
        };

        if let Some(content) = raw_line.strip_prefix('+') {
            hunk.lines.push(DiffLine {
                old_line: None,
                new_line: Some(new_line),
                kind: "add".to_string(),
                content: content.to_string(),
                commentable: true,
            });
            new_line += 1;
        } else if let Some(content) = raw_line.strip_prefix('-') {
            hunk.lines.push(DiffLine {
                old_line: Some(old_line),
                new_line: None,
                kind: "delete".to_string(),
                content: content.to_string(),
                commentable: false,
            });
            old_line += 1;
        } else {
            let content = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            hunk.lines.push(DiffLine {
                old_line: Some(old_line),
                new_line: Some(new_line),
                kind: "context".to_string(),
                content: content.to_string(),
                commentable: true,
            });
            old_line += 1;
            new_line += 1;
        }
    }

    if let Some(h) = current_hunk {
        hunks.push(h);
    }

    hunks
}

pub fn commentable_lines(hunks: &[DiffHunk]) -> Vec<u64> {
    hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.commentable)
        .filter_map(|l| l.new_line)
        .collect()
}

fn parse_hunk_header(header: &str) -> (u64, u64, u64, u64) {
    let parts: Vec<&str> = header.split_whitespace().collect();

    let old = parts.get(1).unwrap_or(&"-0,0");
    let new = parts.get(2).unwrap_or(&"+0,0");

    let (os, oc) = parse_range(&old[1..]);
    let (ns, nc) = parse_range(&new[1..]);

    (os, oc, ns, nc)
}

fn parse_range(s: &str) -> (u64, u64) {
    if let Some((start, count)) = s.split_once(',') {
        (
            start.parse().unwrap_or(0),
            count.parse().unwrap_or(0),
        )
    } else {
        (s.parse().unwrap_or(0), 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_patch() {
        let patch = "@@ -10,3 +10,4 @@ some context\n old line\n-removed\n+added1\n+added2\n unchanged";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 10);
        assert_eq!(hunks[0].new_start, 10);
        assert_eq!(hunks[0].lines.len(), 5);

        let commentable = commentable_lines(&hunks);
        assert_eq!(commentable, vec![10, 11, 12, 13]);
    }

    #[test]
    fn test_multiple_hunks() {
        let patch = "@@ -1,3 +1,3 @@\n context\n-old\n+new\n context\n@@ -20,2 +20,3 @@\n ctx\n+inserted\n end";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].new_start, 1);
        assert_eq!(hunks[1].new_start, 20);

        let cl = commentable_lines(&hunks);
        assert!(cl.contains(&2));
        assert!(cl.contains(&21));
    }

    #[test]
    fn test_addition_only() {
        let patch = "@@ -0,0 +1,3 @@\n+line1\n+line2\n+line3";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 1);
        let cl = commentable_lines(&hunks);
        assert_eq!(cl, vec![1, 2, 3]);
    }
}
