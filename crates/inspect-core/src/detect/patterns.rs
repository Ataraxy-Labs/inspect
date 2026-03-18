use sem_core::model::change::SemanticChange;

use super::types::{DetectorFinding, DetectorKind, Severity};

/// Detect the language family from a file extension.
enum Lang {
    JsTs,
    Rust,
    Python,
    Go,
    Java,
    Other,
}

fn detect_lang(file_path: &str) -> Lang {
    if let Some(ext) = file_path.rsplit('.').next() {
        match ext {
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" => Lang::JsTs,
            "rs" => Lang::Rust,
            "py" | "pyi" => Lang::Python,
            "go" => Lang::Go,
            "java" => Lang::Java,
            _ => Lang::Other,
        }
    } else {
        Lang::Other
    }
}

fn finding(
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
        detector: DetectorKind::Pattern,
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

/// Run all pattern-based rules against semantic changes.
pub fn run_pattern_rules(changes: &[SemanticChange]) -> Vec<DetectorFinding> {
    let mut findings = Vec::new();

    for change in changes {
        let after = match &change.after_content {
            Some(c) => c,
            None => continue,
        };

        let lang = detect_lang(&change.file_path);

        // General patterns (all languages)
        check_fixme_todo(change, after, &mut findings);
        check_magic_number(change, after, &mut findings);

        match lang {
            Lang::JsTs => check_js_ts_patterns(change, after, &mut findings),
            Lang::Rust => check_rust_patterns(change, after, &mut findings),
            Lang::Python => check_python_patterns(change, after, &mut findings),
            Lang::Go => check_go_patterns(change, after, &mut findings),
            Lang::Java => check_java_patterns(change, after, &mut findings),
            Lang::Other => {}
        }

        // Security patterns (language-agnostic where applicable)
        check_security_patterns(change, after, &lang, &mut findings);
    }

    findings
}

// ---------------------------------------------------------------------------
// JavaScript / TypeScript
// ---------------------------------------------------------------------------

fn check_js_ts_patterns(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();

        // foreach-async
        if trimmed.contains(".forEach(async") || trimmed.contains(".forEach( async") {
            findings.push(finding(
                "foreach-async",
                "forEach doesn't await async callbacks — use for...of or Promise.all(arr.map(...))",
                0.9,
                Severity::High,
                change,
                trimmed,
                line_num + 1,
            ));
        }

        // missing-await: async function body calling promise-returning functions without await
        if !trimmed.starts_with("//") && !trimmed.starts_with("*") {
            for fn_call in &[
                "fetch(", ".save(", ".delete(", ".update(", ".create(",
                ".insert(", ".remove(", ".send(", ".post(", ".get(",
                ".put(", ".patch(",
            ] {
                if trimmed.contains(fn_call) && !line_contains_before(line, fn_call, "await") && !trimmed.starts_with("return ") && !trimmed.contains("return ") {
                    // Only flag inside async function bodies — check if entity is async
                    if after.contains("async ") {
                        findings.push(finding(
                            "missing-await",
                            &format!("Possible missing `await` before `{}`", fn_call.trim_end_matches('(')),
                            0.7,
                            Severity::High,
                            change,
                            trimmed,
                            line_num + 1,
                        ));
                        break; // one finding per line
                    }
                }
            }
        }

        // catch-swallow: empty catch or catch with only console.log
        if trimmed.starts_with("catch") || trimmed.contains("} catch") || trimmed.contains("catch (") || trimmed.contains("catch(") {
            if let Some(catch_body) = peek_catch_body(after, line_num) {
                if catch_body.is_empty()
                    || catch_body.iter().all(|l| {
                        let t = l.trim();
                        t.is_empty()
                            || t.starts_with("console.log")
                            || t.starts_with("console.warn")
                            || t == "{"
                            || t == "}"
                    })
                {
                    findings.push(finding(
                        "catch-swallow",
                        "Exception caught but swallowed — error is silently ignored or only logged",
                        0.8,
                        Severity::Medium,
                        change,
                        trimmed,
                        line_num + 1,
                    ));
                }
            }
        }

        // missing-react-key: .map( returning JSX without key
        if trimmed.contains(".map(") && (trimmed.contains("<") || trimmed.contains("=>")) {
            // Look at the next few lines for JSX without key=
            let block = collect_lines(after, line_num, 5);
            let has_jsx = block.iter().any(|l| l.contains("<") && l.contains(">"));
            let has_key = block.iter().any(|l| l.contains("key=") || l.contains("key ="));
            if has_jsx && !has_key {
                findings.push(finding(
                    "missing-react-key",
                    "JSX element in .map() callback is missing a `key` prop",
                    0.75,
                    Severity::Medium,
                    change,
                    trimmed,
                    line_num + 1,
                ));
            }
        }

        // xss-dangerously-set
        if trimmed.contains("dangerouslySetInnerHTML") {
            findings.push(finding(
                "xss-dangerously-set",
                "dangerouslySetInnerHTML usage — potential XSS if input is not sanitized",
                0.85,
                Severity::High,
                change,
                trimmed,
                line_num + 1,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

fn check_rust_patterns(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let is_test = change.file_path.contains("/tests/")
        || change.file_path.contains("_test.rs")
        || change.file_path.ends_with("tests.rs")
        || after.contains("#[test]")
        || after.contains("#[cfg(test)]");
    let is_main = change.entity_name == "main"
        || change.file_path.ends_with("main.rs");

    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*") {
            continue;
        }

        // unwrap-in-lib
        if !is_test && !is_main && trimmed.contains(".unwrap()") {
            findings.push(finding(
                "unwrap-in-lib",
                ".unwrap() in library code — prefer `?` or proper error handling",
                0.8,
                Severity::Medium,
                change,
                trimmed,
                line_num + 1,
            ));
        }

        // todo-in-code
        if !is_test && (trimmed.contains("todo!()") || trimmed.contains("unimplemented!()")) {
            findings.push(finding(
                "todo-in-code",
                "todo!()/unimplemented!() left in non-test code — will panic at runtime",
                0.9,
                Severity::High,
                change,
                trimmed,
                line_num + 1,
            ));
        }

        // unsafe-block
        if trimmed.contains("unsafe {") || trimmed == "unsafe {" || trimmed.starts_with("unsafe {") {
            findings.push(finding(
                "unsafe-block",
                "unsafe block — requires careful review for soundness",
                0.7,
                Severity::High,
                change,
                trimmed,
                line_num + 1,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

fn check_python_patterns(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();

        // mutable-default-arg: def foo(x=[]) or def foo(x={})
        if trimmed.starts_with("def ") && trimmed.contains("(") {
            let args_part = if let Some(start) = trimmed.find('(') {
                &trimmed[start..]
            } else {
                ""
            };
            if args_part.contains("=[]") || args_part.contains("= []") || args_part.contains("={}") || args_part.contains("= {}") {
                findings.push(finding(
                    "mutable-default-arg",
                    "Mutable default argument — default list/dict is shared across calls",
                    0.9,
                    Severity::High,
                    change,
                    trimmed,
                    line_num + 1,
                ));
            }
        }

        // bare-except
        if trimmed == "except:" || trimmed.starts_with("except: ") {
            findings.push(finding(
                "bare-except",
                "Bare `except:` catches all exceptions including KeyboardInterrupt and SystemExit",
                0.85,
                Severity::Medium,
                change,
                trimmed,
                line_num + 1,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

fn check_go_patterns(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let is_test = change.file_path.ends_with("_test.go");

    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }

        // nil-check-missing: using a value from err-returning call without checking err
        // Pattern: `val, err := someFunc(...)` followed by use of val without `if err != nil`
        if trimmed.contains(", err :=") || trimmed.contains(", err =") {
            // Look at the next few lines for err check
            let block = collect_lines(after, line_num + 1, 3);
            let has_err_check = block
                .iter()
                .any(|l| l.contains("if err != nil") || l.contains("if err !="));
            if !has_err_check {
                findings.push(finding(
                    "nil-check-missing",
                    "Error return value not checked — may use nil/zero value from failed call",
                    0.75,
                    Severity::High,
                    change,
                    trimmed,
                    line_num + 1,
                ));
            }
        }

        // exported-function-no-error-return: exported func (uppercase) with no error return
        if !is_test && trimmed.starts_with("func ") && !trimmed.contains("func (") {
            // Check if it's an exported function (starts with uppercase after "func ")
            let after_func = trimmed.strip_prefix("func ").unwrap_or("");
            let func_name_char = after_func.chars().next();
            if let Some(ch) = func_name_char {
                if ch.is_uppercase() {
                    // Check if return type includes error
                    let has_error_return = trimmed.contains("error")
                        || trimmed.contains("Error");
                    // Only flag if it has a body (not interface) and does I/O-like operations
                    let body_block = collect_lines(after, line_num, 10);
                    let does_io = body_block.iter().any(|l| {
                        l.contains("http.") || l.contains("os.") || l.contains("io.")
                            || l.contains("sql.") || l.contains("net.")
                            || l.contains("Read(") || l.contains("Write(")
                            || l.contains("Open(") || l.contains("Dial(")
                    });
                    if !has_error_return && does_io {
                        findings.push(finding(
                            "exported-no-error-return",
                            "Exported function performs I/O but doesn't return an error",
                            0.65,
                            Severity::Medium,
                            change,
                            trimmed,
                            line_num + 1,
                        ));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Java
// ---------------------------------------------------------------------------

fn check_java_patterns(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let is_test = change.file_path.contains("/test/")
        || change.file_path.contains("Test.java")
        || after.contains("@Test");

    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*") {
            continue;
        }

        // resource-leak: AutoCloseable opened without try-with-resources
        if !is_test
            && (trimmed.contains("new FileInputStream(")
                || trimmed.contains("new FileOutputStream(")
                || trimmed.contains("new BufferedReader(")
                || trimmed.contains("new BufferedWriter(")
                || trimmed.contains("new Socket(")
                || trimmed.contains("new ServerSocket(")
                || trimmed.contains(".getConnection(")
                || trimmed.contains("DriverManager.getConnection("))
        {
            // Check if it's inside a try-with-resources (look for "try (" in surrounding lines)
            let context = collect_lines(after, line_num.saturating_sub(3), 4);
            let in_try_with = context.iter().any(|l| {
                let t = l.trim();
                t.starts_with("try (") || t.starts_with("try(") || t.contains("try (")
            });
            if !in_try_with {
                findings.push(finding(
                    "resource-leak",
                    "AutoCloseable resource opened without try-with-resources — may leak",
                    0.75,
                    Severity::High,
                    change,
                    trimmed,
                    line_num + 1,
                ));
            }
        }

        // null-deref: method call on nullable without null check
        // Pattern: Optional.get() without isPresent(), or chained calls on potentially null returns
        if trimmed.contains(".get()") && !trimmed.contains(".isPresent()") {
            // Check if this is Optional usage
            let context = collect_lines(after, line_num.saturating_sub(3), 4);
            let is_optional = context.iter().any(|l| {
                l.contains("Optional") || l.contains("optional")
            }) || trimmed.contains("Optional");
            if is_optional {
                findings.push(finding(
                    "null-deref",
                    "Optional.get() without isPresent()/ifPresent() — will throw NoSuchElementException if empty",
                    0.85,
                    Severity::High,
                    change,
                    trimmed,
                    line_num + 1,
                ));
            }
        }

        // synchronized-missing: shared mutable field access without synchronization hint
        if !is_test
            && (trimmed.contains("volatile ")
                || (trimmed.contains("static ") && trimmed.contains("Map")
                    || trimmed.contains("static ") && trimmed.contains("List")
                    || trimmed.contains("static ") && trimmed.contains("Set")))
        {
            // Check if synchronization is present in the surrounding context
            let full_context = collect_lines(after, 0, after.lines().count());
            let has_sync = full_context.iter().any(|l| {
                l.contains("synchronized") || l.contains("ConcurrentHashMap")
                    || l.contains("Collections.synchronizedMap")
                    || l.contains("AtomicReference") || l.contains("ReentrantLock")
            });
            if !has_sync
                && (trimmed.contains("static ") && !trimmed.contains("final "))
            {
                findings.push(finding(
                    "synchronized-missing",
                    "Shared mutable static field without synchronization — potential race condition",
                    0.6,
                    Severity::High,
                    change,
                    trimmed,
                    line_num + 1,
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Security patterns (cross-language)
// ---------------------------------------------------------------------------

fn check_security_patterns(
    change: &SemanticChange,
    after: &str,
    lang: &Lang,
    findings: &mut Vec<DetectorFinding>,
) {
    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("*") {
            continue;
        }

        // hardcoded-secret
        for secret_pat in &[
            "password = \"", "password =\"", "password=\"",
            "secret = \"", "secret =\"", "secret=\"",
            "api_key = \"", "api_key =\"", "api_key=\"",
            "apikey = \"", "apikey =\"", "apikey=\"",
            "token = \"", "token =\"", "token=\"",
            "PASSWORD = \"", "SECRET = \"", "API_KEY = \"", "TOKEN = \"",
        ] {
            let lower_trimmed = trimmed.to_lowercase();
            let lower_pat = secret_pat.to_lowercase();
            if lower_trimmed.contains(&lower_pat) {
                // Exclude env var lookups and empty strings
                if !trimmed.contains("env") && !trimmed.contains("ENV")
                    && !trimmed.contains("process.env") && !trimmed.contains("os.environ")
                    && !trimmed.contains("std::env") && !ends_with_empty_string(trimmed, secret_pat)
                {
                    findings.push(finding(
                        "hardcoded-secret",
                        "Possible hardcoded secret — use environment variables instead",
                        0.75,
                        Severity::Critical,
                        change,
                        trimmed,
                        line_num + 1,
                    ));
                    break; // one per line
                }
            }
        }

        // ssrf-url-concat (JS/TS and Python)
        match lang {
            Lang::JsTs => {
                if (trimmed.contains("fetch(`") || trimmed.contains("fetch(\"") || trimmed.contains("fetch('"))
                    && trimmed.contains("${")
                {
                    findings.push(finding(
                        "ssrf-url-concat",
                        "URL built with string interpolation passed to fetch — potential SSRF",
                        0.7,
                        Severity::High,
                        change,
                        trimmed,
                        line_num + 1,
                    ));
                }
                if trimmed.contains("new URL(") && (trimmed.contains("+ ") || trimmed.contains("${")) {
                    findings.push(finding(
                        "ssrf-url-concat",
                        "URL constructed with string concatenation — potential SSRF",
                        0.7,
                        Severity::High,
                        change,
                        trimmed,
                        line_num + 1,
                    ));
                }
            }
            _ => {}
        }

        // sql-injection (JS/TS and Python)
        match lang {
            Lang::JsTs | Lang::Python => {
                if (trimmed.contains("query(") || trimmed.contains("execute(") || trimmed.contains("raw("))
                    && (trimmed.contains("${") || (trimmed.contains("\" +") || trimmed.contains("' +")))
                {
                    findings.push(finding(
                        "sql-injection",
                        "SQL query built with string concatenation — use parameterized queries",
                        0.8,
                        Severity::Critical,
                        change,
                        trimmed,
                        line_num + 1,
                    ));
                }
                // Python f-string in query
                if matches!(lang, Lang::Python)
                    && (trimmed.contains("query(f\"") || trimmed.contains("execute(f\"") || trimmed.contains("query(f'") || trimmed.contains("execute(f'"))
                {
                    findings.push(finding(
                        "sql-injection",
                        "SQL query built with f-string — use parameterized queries",
                        0.8,
                        Severity::Critical,
                        change,
                        trimmed,
                        line_num + 1,
                    ));
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// General patterns
// ---------------------------------------------------------------------------

fn check_fixme_todo(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    let before = change.before_content.as_deref().unwrap_or("");

    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();
        for marker in &["FIXME", "TODO", "HACK", "XXX"] {
            if trimmed.contains(marker) && !before.contains(trimmed) {
                findings.push(finding(
                    "fixme-todo",
                    &format!("`{}` marker in new code — should be resolved before merging", marker),
                    0.7,
                    Severity::Low,
                    change,
                    trimmed,
                    line_num + 1,
                ));
                break; // one marker per line is enough
            }
        }
    }
}

fn check_magic_number(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments, imports, consts
        if trimmed.starts_with("//") || trimmed.starts_with("#")
            || trimmed.starts_with("*") || trimmed.starts_with("/*")
            || trimmed.starts_with("import ") || trimmed.starts_with("use ")
            || trimmed.contains("const ") || trimmed.contains("CONST")
            || trimmed.contains("static ") || trimmed.contains("enum ")
        {
            continue;
        }

        // Look for numeric literals > 1 in conditions or assignments
        if trimmed.contains("if ") || trimmed.contains("if(")
            || trimmed.contains("while ") || trimmed.contains("while(")
            || trimmed.contains("== ") || trimmed.contains("!= ")
            || trimmed.contains(">= ") || trimmed.contains("<= ")
        {
            for word in trimmed.split(|c: char| !c.is_ascii_digit() && c != '.') {
                if let Ok(n) = word.parse::<f64>() {
                    if n > 1.0 && n != 2.0 {
                        // Very common: 0, 1, 2 are usually fine
                        findings.push(finding(
                            "magic-number",
                            &format!("Magic number `{}` — consider extracting to a named constant", word),
                            0.4,
                            Severity::Low,
                            change,
                            trimmed,
                            line_num + 1,
                        ));
                        break; // one per line
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if `line` contains `keyword` before the `target` pattern.
fn line_contains_before(line: &str, target: &str, keyword: &str) -> bool {
    if let Some(target_pos) = line.find(target) {
        let before_target = &line[..target_pos];
        before_target.contains(keyword)
    } else {
        false
    }
}

/// Peek at the body of a catch block (next few lines after the catch line).
fn peek_catch_body(content: &str, catch_line_idx: usize) -> Option<Vec<String>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut body = Vec::new();
    let mut brace_depth = 0;
    let mut found_open = false;

    for i in catch_line_idx..lines.len().min(catch_line_idx + 8) {
        let line = lines[i];
        for ch in line.chars() {
            if ch == '{' {
                brace_depth += 1;
                found_open = true;
            } else if ch == '}' {
                brace_depth -= 1;
                if found_open && brace_depth == 0 {
                    return Some(body);
                }
            }
        }
        if found_open && i > catch_line_idx {
            body.push(line.to_string());
        }
    }

    if found_open { Some(body) } else { None }
}

/// Collect a few lines starting from an index.
fn collect_lines(content: &str, start: usize, count: usize) -> Vec<String> {
    content
        .lines()
        .skip(start)
        .take(count)
        .map(|l| l.to_string())
        .collect()
}

/// Check if the pattern ends with an empty string literal `""`.
fn ends_with_empty_string(line: &str, _pat: &str) -> bool {
    // A rough check: if the line has `= ""` pattern
    line.contains("= \"\"") || line.contains("=\"\"")
}

#[cfg(test)]
mod tests {
    use sem_core::model::change::{ChangeType, SemanticChange};

    use super::*;

    fn make_change(file_path: &str, after_content: &str) -> SemanticChange {
        SemanticChange {
            id: "test-id".to_string(),
            entity_id: format!("{}::function::test_fn", file_path),
            change_type: ChangeType::Added,
            entity_type: "function".to_string(),
            entity_name: "test_fn".to_string(),
            file_path: file_path.to_string(),
            old_file_path: None,
            before_content: None,
            after_content: Some(after_content.to_string()),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: None,
        }
    }

    fn make_modified_change(file_path: &str, before: &str, after: &str) -> SemanticChange {
        SemanticChange {
            id: "test-id".to_string(),
            entity_id: format!("{}::function::test_fn", file_path),
            change_type: ChangeType::Modified,
            entity_type: "function".to_string(),
            entity_name: "test_fn".to_string(),
            file_path: file_path.to_string(),
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
    fn test_foreach_async() {
        let change = make_change("src/app.ts", "items.forEach(async (item) => {\n  await save(item);\n});");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "foreach-async"), "Should detect forEach(async): {:?}", findings);
    }

    #[test]
    fn test_catch_swallow() {
        let change = make_change("src/app.ts", "try {\n  doSomething();\n} catch (e) {\n  console.log(e);\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "catch-swallow"), "Should detect swallowed catch: {:?}", findings);
    }

    #[test]
    fn test_catch_with_handling_is_ok() {
        let change = make_change("src/app.ts", "try {\n  doSomething();\n} catch (e) {\n  reportError(e);\n  throw e;\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(!findings.iter().any(|f| f.rule_id == "catch-swallow"), "Should NOT flag catch with real handling: {:?}", findings);
    }

    #[test]
    fn test_unwrap_in_lib() {
        let change = make_change("src/lib.rs", "pub fn parse(input: &str) -> Value {\n    serde_json::from_str(input).unwrap()\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "unwrap-in-lib"), "Should detect .unwrap() in lib: {:?}", findings);
    }

    #[test]
    fn test_unwrap_in_test_is_ok() {
        let change = make_change("src/tests.rs", "#[test]\nfn it_parses() {\n    let v = parse(\"1\").unwrap();\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(!findings.iter().any(|f| f.rule_id == "unwrap-in-lib"), "Should NOT flag .unwrap() in test: {:?}", findings);
    }

    #[test]
    fn test_todo_macro() {
        let change = make_change("src/handler.rs", "pub fn handle() {\n    todo!()\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "todo-in-code"), "Should detect todo!(): {:?}", findings);
    }

    #[test]
    fn test_mutable_default_arg() {
        let change = make_change("app.py", "def process(items=[]):\n    items.append(1)\n    return items");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "mutable-default-arg"), "Should detect mutable default arg: {:?}", findings);
    }

    #[test]
    fn test_bare_except() {
        let change = make_change("app.py", "try:\n    do_stuff()\nexcept:\n    pass");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "bare-except"), "Should detect bare except: {:?}", findings);
    }

    #[test]
    fn test_hardcoded_secret() {
        let change = make_change("src/config.ts", "const password = \"hunter2\";");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "hardcoded-secret"), "Should detect hardcoded secret: {:?}", findings);
    }

    #[test]
    fn test_env_var_secret_is_ok() {
        let change = make_change("src/config.ts", "const password = process.env.PASSWORD;");
        let findings = run_pattern_rules(&[change]);
        assert!(!findings.iter().any(|f| f.rule_id == "hardcoded-secret"), "Should NOT flag env var: {:?}", findings);
    }

    #[test]
    fn test_xss_dangerously_set() {
        let change = make_change("src/component.tsx", "return <div dangerouslySetInnerHTML={{ __html: content }} />;");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "xss-dangerously-set"), "Should detect dangerouslySetInnerHTML: {:?}", findings);
    }

    #[test]
    fn test_ssrf_url_concat() {
        let change = make_change("src/api.ts", "const resp = await fetch(`${baseUrl}/api/${userId}`);");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "ssrf-url-concat"), "Should detect SSRF URL concat: {:?}", findings);
    }

    #[test]
    fn test_sql_injection() {
        let change = make_change("src/db.ts", "const result = await db.query(`SELECT * FROM users WHERE id = ${userId}`);");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "sql-injection"), "Should detect SQL injection: {:?}", findings);
    }

    #[test]
    fn test_fixme_todo_new_only() {
        let before = "// existing code";
        let after = "// existing code\n// TODO: fix this later";
        let change = make_modified_change("src/app.ts", before, after);
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "fixme-todo"), "Should detect new TODO: {:?}", findings);
    }

    #[test]
    fn test_fixme_todo_existing_is_ok() {
        let content = "// TODO: this was already here";
        let change = make_modified_change("src/app.ts", content, content);
        let findings = run_pattern_rules(&[change]);
        assert!(!findings.iter().any(|f| f.rule_id == "fixme-todo"), "Should NOT flag existing TODO: {:?}", findings);
    }

    #[test]
    fn test_unsafe_block() {
        let change = make_change("src/ffi.rs", "pub fn call_c() {\n    unsafe {\n        libc::free(ptr);\n    }\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "unsafe-block"), "Should detect unsafe block: {:?}", findings);
    }

    #[test]
    fn test_python_sql_injection() {
        let change = make_change("app.py", "cursor.execute(f\"SELECT * FROM users WHERE id = {user_id}\")");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "sql-injection"), "Should detect Python SQL injection: {:?}", findings);
    }

    #[test]
    fn test_missing_react_key() {
        let change = make_change("src/List.tsx", "return items.map((item) => <li>{item.name}</li>);");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "missing-react-key"), "Should detect missing React key: {:?}", findings);
    }

    // Go tests

    #[test]
    fn test_go_nil_check_missing() {
        let change = make_change("main.go", "val, err := doStuff()\nfmt.Println(val)");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "nil-check-missing"), "Should detect missing nil check: {:?}", findings);
    }

    #[test]
    fn test_go_nil_check_present_is_ok() {
        let change = make_change("main.go", "val, err := doStuff()\nif err != nil {\n    return err\n}\nfmt.Println(val)");
        let findings = run_pattern_rules(&[change]);
        assert!(!findings.iter().any(|f| f.rule_id == "nil-check-missing"), "Should NOT flag when err is checked: {:?}", findings);
    }

    #[test]
    fn test_go_exported_no_error_return() {
        let change = make_change("server.go", "func FetchData(url string) string {\n    resp := http.Get(url)\n    return resp\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "exported-no-error-return"), "Should detect exported func with I/O but no error return: {:?}", findings);
    }

    // Java tests

    #[test]
    fn test_java_resource_leak() {
        let change = make_change("App.java", "FileInputStream fis = new FileInputStream(\"data.txt\");\nfis.read();");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "resource-leak"), "Should detect resource leak: {:?}", findings);
    }

    #[test]
    fn test_java_resource_in_try_with_is_ok() {
        let change = make_change("App.java", "try (FileInputStream fis = new FileInputStream(\"data.txt\")) {\n    fis.read();\n}");
        let findings = run_pattern_rules(&[change]);
        assert!(!findings.iter().any(|f| f.rule_id == "resource-leak"), "Should NOT flag try-with-resources: {:?}", findings);
    }

    #[test]
    fn test_java_optional_get_without_check() {
        let change = make_change("Service.java", "Optional<String> opt = findUser();\nString name = opt.get();");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "null-deref"), "Should detect Optional.get() without check: {:?}", findings);
    }

    #[test]
    fn test_java_synchronized_missing() {
        let change = make_change("Cache.java", "private static Map<String, Object> cache = new HashMap<>();");
        let findings = run_pattern_rules(&[change]);
        assert!(findings.iter().any(|f| f.rule_id == "synchronized-missing"), "Should detect unsynchronized static mutable: {:?}", findings);
    }
}
