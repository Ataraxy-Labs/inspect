use sem_core::model::change::SemanticChange;

use super::types::{DetectorFinding, DetectorKind, Severity};

/// Detect the language family from a file extension.
enum Lang {
    JsTs,
    Rust,
    Python,
    Go,
    Java,
    Ruby,
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
            "rb" | "rake" => Lang::Ruby,
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

        match lang {
            Lang::JsTs => check_js_ts_patterns(change, after, &mut findings),
            Lang::Rust => check_rust_patterns(change, after, &mut findings),
            Lang::Python => check_python_patterns(change, after, &mut findings),
            Lang::Go => check_go_patterns(change, after, &mut findings),
            Lang::Java => check_java_patterns(change, after, &mut findings),
            Lang::Ruby => check_ruby_patterns(change, after, &mut findings),
            Lang::Other => {}
        }

        // Cross-language content-based patterns
        check_case_insensitive_compare(change, after, &mut findings);
        check_export_filename_mismatch(change, after, &lang, &mut findings);
        check_error_message_context_mismatch(change, after, &mut findings);

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

        // conditional-flag-assignment: same boolean flag controlling multiple property assignments
        // Pattern: `x ? value : undefined` or ternary with boolean flag for property setting
        if trimmed.contains("? ") && trimmed.contains(": undefined")
            && (trimmed.contains("IS_") || trimmed.contains("is_") || trimmed.contains("ENABLE") || trimmed.contains("enable"))
        {
            findings.push(finding(
                "conditional-flag-assignment",
                "Property conditionally set via boolean flag — verify the condition polarity is correct",
                0.5,
                Severity::Medium,
                change,
                trimmed,
                line_num + 1,
            ));
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

        // permission-and-or: using && where || is likely intended in permission checks
        if (trimmed.contains("isAdmin") || trimmed.contains("isOwner") || trimmed.contains("isMember")
            || trimmed.contains("is_admin") || trimmed.contains("is_owner") || trimmed.contains("is_member")
            || trimmed.contains("hasPermission") || trimmed.contains("has_permission")
            || trimmed.contains("canEdit") || trimmed.contains("can_edit")
            || trimmed.contains("hasRole") || trimmed.contains("has_role"))
            && trimmed.contains("&&")
        {
            // Count how many permission-like terms appear
            let perm_terms = ["isAdmin", "isOwner", "isMember", "isTeamAdmin", "isTeamOwner",
                "is_admin", "is_owner", "is_member", "hasPermission", "has_permission",
                "canEdit", "can_edit", "hasRole", "has_role", "isManager", "is_manager",
                "isModerator", "is_moderator"];
            let perm_count = perm_terms.iter().filter(|t| trimmed.contains(*t)).count();
            if perm_count >= 2 {
                findings.push(finding(
                    "permission-and-or",
                    "Multiple permission checks combined with `&&` — should this be `||`? (require ALL vs ANY)",
                    0.6,
                    Severity::High,
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

        // magic-number-repetition: same literal number repeated many times
        // (sign of unmaintainable test code)
        if !trimmed.starts_with("#") && !trimmed.starts_with("def ") {
            // Check for repeated numeric literals in function body
            for num_pat in &["50,", "50)", "50 ", "100,", "100)", "100 ", "1000,", "1000)", "1000 "] {
                if trimmed.contains(num_pat) {
                    // Count occurrences in the full entity
                    let literal = num_pat.trim_end_matches(|c: char| !c.is_ascii_digit());
                    let count = after.matches(literal).count();
                    if count >= 4 {
                        findings.push(finding(
                            "magic-number-repetition",
                            &format!("Magic number `{}` repeated {} times — extract as named constant", literal, count),
                            0.5,
                            Severity::Low,
                            change,
                            trimmed,
                            line_num + 1,
                        ));
                        break;
                    }
                }
            }
        }
    }

    // reduce-init-mismatch: __reduce__ returns args that don't match __init__ parameter order
    if after.contains("__reduce__") && after.contains("__init__") && after.contains("return") {
        findings.push(finding(
            "reduce-init-mismatch",
            "__reduce__ defines pickle reconstruction args — verify argument order matches __init__ parameter order exactly",
            0.7,
            Severity::High,
            change,
            "__reduce__ return tuple",
            1,
        ));
    }
}

// ---------------------------------------------------------------------------
// Ruby
// ---------------------------------------------------------------------------

fn check_ruby_patterns(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    // duplicate-method-def: Ruby has no method overloading — last def wins
    let mut method_defs: Vec<(&str, usize)> = Vec::new();
    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("def ").or_else(|| trimmed.strip_prefix("def self.")) {
            let name = rest.split(&['(', ' ', '\n'][..]).next().unwrap_or("");
            if !name.is_empty() {
                // Check if this method name was already defined
                if let Some((_, prev_line)) = method_defs.iter().find(|(n, _)| *n == name) {
                    findings.push(finding(
                        "duplicate-method-def",
                        &format!("Method `{}` defined twice (first at line {}) — Ruby has no overloading, second definition silently replaces the first", name, prev_line + 1),
                        0.95,
                        Severity::Critical,
                        change,
                        trimmed,
                        line_num + 1,
                    ));
                }
                method_defs.push((name, line_num));
            }
        }
    }
}

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

        // thread-no-join: Thread started but never joined — potential race condition
        if trimmed.contains("new Thread(") || trimmed.contains(".start()")
        {
            // Look for .join() in the surrounding context
            let full_context = collect_lines(after, 0, after.lines().count());
            let has_thread = full_context.iter().any(|l| l.contains("Thread") || l.contains("Runnable"));
            let has_join = full_context.iter().any(|l| l.contains(".join("));
            if has_thread && !has_join {
                findings.push(finding(
                    "thread-no-join",
                    "Thread started but never joined — may cause race conditions or missed exceptions",
                    0.7,
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
// Cross-language content-based patterns
// ---------------------------------------------------------------------------

/// Detect string comparisons (indexOf, includes, find) on user-input-like data
/// (codes, tokens, emails, hex, hashes) without case normalization.
fn check_case_insensitive_compare(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    // Only worthwhile for languages that have these APIs
    let has_case_norm = after.contains("toLowerCase")
        || after.contains("toUpperCase")
        || after.contains("localeCompare")
        || after.contains(".lower()")
        || after.contains(".upper()")
        || after.contains(".casecmp")
        || after.contains("to_lowercase")
        || after.contains("to_uppercase")
        || after.contains("EqualFold")
        || after.contains("equalsIgnoreCase");

    if has_case_norm {
        return;
    }

    let compare_fns = ["indexOf(", "includes(", ".find(", ".index(", ".contains("];

    // Context words that suggest user input or case-insensitive data
    let context_words = [
        "code", "Code", "token", "Token", "key", "Key",
        "hex", "Hex", "hash", "Hash", "email", "Email",
        "host", "Host", "domain", "Domain", "backup",
        "Backup", "secret", "Secret", "otp", "OTP",
    ];

    let has_context = context_words.iter().any(|w| after.contains(w));
    if !has_context {
        return;
    }

    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("#") {
            continue;
        }

        let has_compare = compare_fns.iter().any(|f| trimmed.contains(f));
        if !has_compare {
            continue;
        }

        // Check that this specific line or nearby context references user-input-like data
        let nearby = collect_lines(after, line_num.saturating_sub(2), 5);
        let nearby_has_context = nearby.iter().any(|l| {
            context_words.iter().any(|w| l.contains(w))
        });

        if nearby_has_context {
            findings.push(finding(
                "case-insensitive-compare-needed",
                "String comparison (indexOf/includes/find) without case normalization on user input (codes/tokens/emails) — may fail on mixed-case input",
                0.7,
                Severity::Medium,
                change,
                trimmed,
                line_num + 1,
            ));
            break; // one per entity
        }
    }
}

/// Detect exported function/class names that don't match the filename.
/// e.g., function `TwoFactor` exported from `BackupCode.tsx`.
fn check_export_filename_mismatch(
    change: &SemanticChange,
    after: &str,
    lang: &Lang,
    findings: &mut Vec<DetectorFinding>,
) {
    // Only relevant for JS/TS where filename-export conventions are strong
    if !matches!(lang, Lang::JsTs) {
        return;
    }

    // Extract the base filename (without extension and path)
    let file_name = change.file_path.rsplit('/').next().unwrap_or(&change.file_path);
    let base_name = file_name
        .split('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    // Skip index files and very short names
    if base_name.is_empty() || base_name == "index" || base_name.len() < 3 {
        return;
    }

    let entity_lower = change.entity_name.to_lowercase();

    // Only check if this entity looks like a default/named export (function, class, component)
    let is_export = after.contains("export default")
        || after.contains("export function")
        || after.contains("export const")
        || after.contains("export class")
        || after.contains(&format!("export {{ {} }}", change.entity_name));

    if !is_export {
        return;
    }

    // Check if entity name and base filename are significantly different
    if !base_name.contains(&entity_lower) && !entity_lower.contains(&base_name) {
        // Find the export line for evidence
        let mut evidence_line = 1;
        let mut evidence_text = &*change.entity_name;
        for (line_num, line) in after.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains("export") && trimmed.contains(&change.entity_name) {
                evidence_line = line_num + 1;
                evidence_text = trimmed;
                break;
            }
        }

        findings.push(finding(
            "export-filename-mismatch",
            &format!(
                "Exported name `{}` doesn't match filename `{}` — inconsistent naming may confuse imports",
                change.entity_name, file_name,
            ),
            0.6,
            Severity::Low,
            change,
            evidence_text,
            evidence_line,
        ));
    }
}

/// Detect error/throw messages that reference operations mismatching the function/endpoint context.
/// e.g., error says "backup code login" but function is a "disable" endpoint.
fn check_error_message_context_mismatch(
    change: &SemanticChange,
    after: &str,
    findings: &mut Vec<DetectorFinding>,
) {
    // Operation keywords that frequently appear in error messages
    let operation_words = [
        "login", "logout", "signup", "register", "disable", "enable",
        "create", "delete", "update", "remove", "add", "reset",
        "verify", "activate", "deactivate", "connect", "disconnect",
    ];

    let entity_lower = change.entity_name.to_lowercase();
    let file_lower = change.file_path.to_lowercase();

    for (line_num, line) in after.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("#") {
            continue;
        }

        // Look for error/throw patterns with string literals
        let is_error_line = trimmed.contains("throw ")
            || trimmed.contains("Error(")
            || trimmed.contains("error(")
            || trimmed.contains("Error {")
            || trimmed.contains("raise ")
            || trimmed.contains("panic!(");

        if !is_error_line {
            continue;
        }

        // Extract string literals from the line
        let error_text = extract_string_literals(trimmed).join(" ").to_lowercase();
        if error_text.len() < 10 {
            continue;
        }

        // Find operation words in the error message
        for op in &operation_words {
            if !error_text.contains(op) {
                continue;
            }

            // Check if this operation word matches the entity/file context
            let context_has_op = entity_lower.contains(op) || file_lower.contains(op);
            if context_has_op {
                continue; // matches — no mismatch
            }

            // Check if any OTHER operation word matches the entity/file context
            let context_op = operation_words.iter().find(|other_op| {
                *other_op != op && (entity_lower.contains(*other_op) || file_lower.contains(*other_op))
            });

            if let Some(actual_op) = context_op {
                findings.push(finding(
                    "error-message-context-mismatch",
                    &format!(
                        "Error message mentions '{}' but function/file context suggests '{}' — copy-paste or wrong message?",
                        op, actual_op,
                    ),
                    0.6,
                    Severity::Low,
                    change,
                    trimmed,
                    line_num + 1,
                ));
                return; // one per entity
            }
        }
    }
}

/// Extract string literal contents from a line (between quotes).
fn extract_string_literals(line: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '"' || ch == '\'' || ch == '`' {
            let quote = ch;
            let start = i + 1;
            for (j, c) in chars.by_ref() {
                if c == quote && (j == 0 || line.as_bytes().get(j - 1) != Some(&b'\\')) {
                    if j > start {
                        results.push(&line[start..j]);
                    }
                    break;
                }
            }
        }
    }
    results
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

    // New detector tests

    #[test]
    fn test_case_insensitive_compare_indexof() {
        let change = make_change(
            "src/BackupCode.tsx",
            "function validateCode(backupCodes: string[], code: string) {\n  return backupCodes.indexOf(code) >= 0;\n}",
        );
        let findings = run_pattern_rules(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "case-insensitive-compare-needed"),
            "Should detect case-sensitive indexOf on codes: {:?}", findings
        );
    }

    #[test]
    fn test_case_insensitive_compare_with_tolowercase_ok() {
        let change = make_change(
            "src/BackupCode.tsx",
            "function validateCode(backupCodes: string[], code: string) {\n  return backupCodes.indexOf(code.toLowerCase()) >= 0;\n}",
        );
        let findings = run_pattern_rules(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "case-insensitive-compare-needed"),
            "Should NOT flag when toLowerCase is used: {:?}", findings
        );
    }

    #[test]
    fn test_export_filename_mismatch() {
        let change = make_change(
            "src/components/BackupCode.tsx",
            "export default function TwoFactor() {\n  return <div>2FA</div>;\n}",
        );
        // Override entity_name
        let mut change = change;
        change.entity_name = "TwoFactor".to_string();
        let findings = run_pattern_rules(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "export-filename-mismatch"),
            "Should detect TwoFactor exported from BackupCode.tsx: {:?}", findings
        );
    }

    #[test]
    fn test_export_filename_match_ok() {
        let change = make_change(
            "src/components/BackupCode.tsx",
            "export default function BackupCode() {\n  return <div>Backup</div>;\n}",
        );
        let mut change = change;
        change.entity_name = "BackupCode".to_string();
        let findings = run_pattern_rules(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "export-filename-mismatch"),
            "Should NOT flag when name matches file: {:?}", findings
        );
    }

    #[test]
    fn test_error_message_context_mismatch() {
        let change = make_change(
            "src/api/disable-backup-code.ts",
            "export async function disableBackupCode() {\n  throw new Error(\"backup code login failed\");\n}",
        );
        let mut change = change;
        change.entity_name = "disableBackupCode".to_string();
        let findings = run_pattern_rules(&[change]);
        assert!(
            findings.iter().any(|f| f.rule_id == "error-message-context-mismatch"),
            "Should detect 'login' in error message for 'disable' endpoint: {:?}", findings
        );
    }

    #[test]
    fn test_error_message_context_match_ok() {
        let change = make_change(
            "src/api/login.ts",
            "export async function login() {\n  throw new Error(\"login failed\");\n}",
        );
        let mut change = change;
        change.entity_name = "login".to_string();
        let findings = run_pattern_rules(&[change]);
        assert!(
            !findings.iter().any(|f| f.rule_id == "error-message-context-mismatch"),
            "Should NOT flag when error message matches context: {:?}", findings
        );
    }
}
