use std::collections::HashSet;

use sem_core::model::change::SemanticChange;

use crate::types::ChangeClassification;

/// Classify a semantic change using ConGra taxonomy.
/// Compares before/after content line-by-line to determine
/// which dimensions (text, syntax, functional) changed.
pub fn classify_change(change: &SemanticChange) -> ChangeClassification {
    let before = change.before_content.as_deref().unwrap_or("");
    let after = change.after_content.as_deref().unwrap_or("");

    // Added or deleted entities are always functional
    if before.is_empty() || after.is_empty() {
        return ChangeClassification::Functional;
    }

    // If structural_change is explicitly false, it's cosmetic only
    if change.structural_change == Some(false) {
        return ChangeClassification::Text;
    }

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    let mut has_text = false;
    let mut has_syntax = false;
    let mut has_functional = false;

    // Build hash sets of non-empty trimmed lines for O(1) lookup
    let before_set: HashSet<&str> = before_lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let after_set: HashSet<&str> = after_lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

    // Lines only in before (removed)
    for line in &before_set {
        if !after_set.contains(line) {
            categorize_line(line, &mut has_text, &mut has_syntax, &mut has_functional);
        }
    }

    // Lines only in after (added)
    for line in &after_set {
        if !before_set.contains(line) {
            categorize_line(line, &mut has_text, &mut has_syntax, &mut has_functional);
        }
    }

    // If nothing detected but content differs, it's functional
    if !has_text && !has_syntax && !has_functional {
        if before.trim() != after.trim() {
            has_functional = true;
        } else {
            has_text = true; // whitespace-only
        }
    }

    match (has_text, has_syntax, has_functional) {
        (true, true, true) => ChangeClassification::TextSyntaxFunctional,
        (true, true, false) => ChangeClassification::TextSyntax,
        (true, false, true) => ChangeClassification::TextFunctional,
        (false, true, true) => ChangeClassification::SyntaxFunctional,
        (true, false, false) => ChangeClassification::Text,
        (false, true, false) => ChangeClassification::Syntax,
        (false, false, true) => ChangeClassification::Functional,
        (false, false, false) => ChangeClassification::Text,
    }
}

fn categorize_line(line: &str, has_text: &mut bool, has_syntax: &mut bool, has_functional: &mut bool) {
    if is_comment_line(line) {
        *has_text = true;
    } else if is_syntax_line(line) {
        *has_syntax = true;
    } else {
        *has_functional = true;
    }
}

fn is_comment_line(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with("/*")
        || line.starts_with('*')
        || line.starts_with("///")
        || line.starts_with("/**")
        || line.starts_with("\"\"\"")
        || (line.starts_with('#') && !line.starts_with("#["))
}

fn is_syntax_line(line: &str) -> bool {
    line.starts_with("fn ")
        || line.starts_with("pub fn ")
        || line.starts_with("pub(crate) fn ")
        || line.starts_with("def ")
        || line.starts_with("class ")
        || line.starts_with("struct ")
        || line.starts_with("enum ")
        || line.starts_with("trait ")
        || line.starts_with("impl ")
        || line.starts_with("interface ")
        || line.starts_with("type ")
        || line.starts_with("pub struct ")
        || line.starts_with("pub enum ")
        || line.starts_with("pub trait ")
        || line.starts_with("async fn ")
        || line.starts_with("pub async fn ")
        || line.starts_with("function ")
        || line.starts_with("export function ")
        || line.starts_with("export default ")
        || line.contains("->")
        || line.contains("=> ")
        || line.contains(": &")
        || line.contains(": Vec<")
        || line.contains(": Option<")
        || line.contains(": Result<")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sem_core::model::change::{ChangeType, SemanticChange};

    fn make_change(before: &str, after: &str, structural: Option<bool>) -> SemanticChange {
        SemanticChange {
            id: "test".into(),
            entity_id: "test::fn::foo".into(),
            change_type: ChangeType::Modified,
            entity_type: "function".into(),
            entity_name: "foo".into(),
            file_path: "test.rs".into(),
            old_file_path: None,
            before_content: Some(before.into()),
            after_content: Some(after.into()),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: structural,
        }
    }

    #[test]
    fn text_only_change() {
        let change = make_change(
            "fn foo() {\n    // old comment\n    x + 1\n}",
            "fn foo() {\n    // new comment\n    x + 1\n}",
            Some(false),
        );
        assert_eq!(classify_change(&change), ChangeClassification::Text);
    }

    #[test]
    fn functional_change() {
        let change = make_change(
            "fn foo() {\n    x + 1\n}",
            "fn foo() {\n    x + 2\n}",
            Some(true),
        );
        assert_eq!(classify_change(&change), ChangeClassification::Functional);
    }

    #[test]
    fn mixed_text_functional() {
        let change = make_change(
            "fn foo() {\n    // old comment\n    x + 1\n}",
            "fn foo() {\n    // new comment\n    x + 2\n}",
            Some(true),
        );
        assert_eq!(classify_change(&change), ChangeClassification::TextFunctional);
    }
}
