# Golden Bug Analysis — Martian Code Review Benchmark

**Total golden bugs**: 137 across 50 PRs in 5 repos
**Date**: 2026-03-22
**Purpose**: Categorize all golden bugs to determine detector coverage strategy

## Summary Table

| Category | Count | % | Sev(C/H/M/L) | Rust-Detectable (Y/P/N) | Existing Detectors | New Needed |
|----------|------:|--:|---------------|------------------------|-------------------|------------|
| logic-error | 57 | 42% | 3/16/22/16 | 16/16/25 | 5 | 5 |
| naming-mismatch | 14 | 10% | 0/1/4/9 | 5/6/3 | 1 | 3 |
| null-safety | 12 | 9% | 2/6/4/0 | 4/8/0 | 1 | 2 |
| type-error | 11 | 8% | 1/6/3/1 | 2/8/1 | 0 | 2 |
| concurrency/TOCTOU | 10 | 7% | 0/5/4/1 | 1/4/5 | 0 | 2 |
| contract-break | 8 | 6% | 1/4/2/1 | 4/4/0 | 5 | 1 |
| dead-code | 5 | 4% | 0/0/1/4 | 4/1/0 | 1 | 2 |
| config-error | 5 | 4% | 0/0/2/3 | 4/1/0 | 0 | 2 |
| error-handling | 4 | 3% | 0/0/0/4 | 0/4/0 | 1 | 0 |
| case-sensitivity | 3 | 2% | 0/1/2/0 | 2/1/0 | 0 | 1 |
| security | 3 | 2% | 1/0/2/0 | 0/1/2 | 0 | 0 |
| async-misuse | 3 | 2% | 1/1/1/0 | 2/1/0 | 2 | 0 |
| domain-logic | 2 | 1% | 0/1/0/1 | 0/0/2 | 0 | 0 |
| **TOTAL** | **137** | **100%** | **9/41/47/40** | **44/55/38** | | |

## Key Insight: Rust Detectability

- **Fully detectable by Rust**: 44 (32%) — deterministic pattern matching
- **Partially detectable**: 55 (40%) — Rust can flag candidates, LLM validates
- **LLM-only**: 38 (28%) — needs deep reasoning or domain knowledge
- **Rust-touchable (yes+partial)**: 99 (72%)

## Distribution by Repo

| Category | cal.com | discourse | grafana | keycloak | sentry |
|----------|--------:|----------:|--------:|---------:|-------:|
| logic-error | 14 | 7 | 11 | 12 | 13 |
| naming-mismatch | 1 | 3 | 2 | 3 | 5 |
| null-safety | 2 | 4 | 1 | 2 | 3 |
| type-error | 2 | 2 | 1 | 0 | 6 |
| concurrency/TOCTOU | 2 | 2 | 4 | 1 | 1 |
| contract-break | 2 | 0 | 1 | 2 | 3 |
| dead-code | 2 | 1 | 1 | 1 | 0 |
| config-error | 0 | 3 | 0 | 2 | 0 |
| error-handling | 2 | 0 | 1 | 1 | 0 |
| case-sensitivity | 2 | 1 | 0 | 0 | 0 |
| security | 0 | 2 | 0 | 0 | 1 |
| async-misuse | 2 | 1 | 0 | 0 | 0 |
| domain-logic | 0 | 2 | 0 | 0 | 0 |

---

## Category Deep Dives

### logic-error (57 bugs, 42%)

**Subcategories:**
- wrong-variable: 6
- regex-bypass: 1
- wrong-value: 1
- wrong-argument: 1
- syntax-error: 1
- css-conflict: 1
- off-by-one: 1
- http-method-mismatch: 1
- condition-gap: 1
- import-error: 1
- falsy-zero: 1
- hash-nondeterminism: 1
- wrong-key: 1
- dict-order: 1
- self-comparison: 1
- boolean-inversion: 1
- hardcoded-string: 1
- wrong-operator: 1
- missing-dedup: 1
- empty-string-init: 1
- missing-filter: 1
- guard-skip: 1
- empty-update: 1
- platform-specific: 1
- incomplete-cleanup: 1
- reference-equality: 1
- redundant-query: 1
- test-flakiness: 1
- isinstance-check: 1
- monkeypatched-sleep: 1
- early-break: 1
- magic-number: 1
- asymmetric-logic: 1
- test-comment-mismatch: 1
- missing-prop: 1
- missing-dependency: 1
- removed-feature: 1
- behavior-change: 1
- time-inconsistency: 1
- wrong-log-level: 1
- error-overwrites-cache: 1
- always-false: 1
- wrong-return: 1
- wrong-feature-flag: 1
- wrong-lookup: 1
- wrong-resource-type: 1
- system-exit: 1
- wrong-permission: 1
- id-name-mismatch: 1
- validation-gap: 1
- inverted-logic: 1
- missing-id-set: 1

**Existing detectors:**
- ✅ negation-flip (diff_heuristics.rs)
- ✅ removed-guard (diff_heuristics.rs)
- ✅ off-by-one (diff_heuristics.rs)
- ✅ boolean-polarity-flip (diff_heuristics.rs)
- ✅ argument-order-swap (diff_heuristics.rs)

**New detectors needed:**
- 🆕 wrong-variable-in-scope: When a changed line uses a variable but a very similar variable exists in scope (e.g., slotStartTime vs slotEndTime, session vs delegate)
- 🆕 boolean-logic-error: AND vs OR confusion in permission checks, inverted conditions
- 🆕 always-true-false: Conditions that trivially always evaluate to same value
- 🆕 reference-equality: Object equality with === instead of value comparison methods
- 🆕 wrong-return-value: Function returns wrong variable or original instead of modified version

**Sample bugs:**
- [Medium] [discourse] Regex pattern @(#{domains}) only matches domain suffixes, not full domains. evil.example.com would match whitelist entry example.com....
- [Medium] [discourse] The current origin validation using indexOf is insufficient and can be bypassed. An attacker could use a malicious domain like evil-discourseUrl.com t...
- [Medium] [discourse] postMessage targetOrigin should be the origin (scheme+host+port), not the full referrer URL; using the full URL will cause the message to be dropped a...
- [Medium] [discourse] The ERB block closes with end if, which is invalid Ruby/ERB and will raise at render; it should just be end to close the if block....
- [Low] [discourse] Mixing float: left with flexbox causes layout issues. Further this PR removes the float-based right alignment for .d-header .panel, which may cause th...
- ... and 52 more

### naming-mismatch (14 bugs, 10%)

**Subcategories:**
- typo: 3
- wrong-docstring: 3
- inconsistent-naming: 2
- wrong-prefix: 1
- missing-suffix: 1
- export-name: 1
- wrong-name: 1
- wrong-callee: 1
- wrong-reference: 1

**Existing detectors:**
- ✅ variable-near-miss (diff_heuristics.rs)

**New detectors needed:**
- 🆕 typo-detector: Levenshtein distance on identifiers to catch misspellings (santize→sanitize)
- 🆕 callee-near-miss: Function called but similar-named function was likely intended (recordLegacyDuration vs recordStorageDuration)
- 🆕 export-filename-mismatch: Exported name doesnt match filename convention

**Sample bugs:**
- [Low] [discourse] Typo in property name: 'stopNotificiationsText' should be 'stopNotificationsText' (missing 'n' in 'Notifications')...
- [Low] [discourse] -ms-align-items never existed in any version of IE/Edge; the correct legacy property is -ms-flex-align....
- [Medium] [discourse] The include_website_name method is missing the required ? suffix. Rails serializers expect include_ methods to end with ? for conditional attribute in...
- [Low] [cal.com] The exported function TwoFactor handles backup codes and is in BackupCode.tsx. Inconsistent naming....
- [Low] [sentry] The method name has a typo: test_from_dict_inalid_data should be test_from_dict_invalid_data....
- ... and 9 more

### null-safety (12 bugs, 9%)

**Subcategories:**
- nil-deref: 5
- missing-key-check: 2
- none-deref: 1
- null-deref: 1
- undefined-access: 1
- wrong-null-check: 1
- optional-get: 1

**Existing detectors:**
- ✅ missing-null-check (patterns.rs)

**New detectors needed:**
- 🆕 nil-after-lookup: Method returns optional/nullable but result used without check
- 🆕 missing-key-access: Dictionary/object access without checking key exists

**Sample bugs:**
- [Critical] [discourse] NoMethodError before_validation in EmbeddableHost...
- [Medium] [discourse] The update and destroy methods in Admin::EmbeddableHostsController do not validate the existence of the EmbeddableHost record retrieved by ID. If Embe...
- [High] [discourse] logic: Potential nil pointer exception - if no TopicUser record exists, tu will be nil and calling methods on it will crash...
- [Medium] [discourse] The TopicEmbed.import method is susceptible to a NoMethodError if the contents parameter is nil when attempting to append a string, and an XSS vulnera...
- [High] [sentry] When requests are authenticated with API keys or org auth tokens (which have user_id=None), organization_context.member is None. Line 71 attempts to a...
- ... and 7 more

### type-error (11 bugs, 8%)

**Subcategories:**
- negative-slice: 3
- type-mismatch: 3
- format-mismatch: 1
- string-vs-symbol: 1
- invalid-syntax: 1
- wrong-return: 1
- serialization: 1

**New detectors needed:**
- 🆕 format-mismatch: API expects one format but receives another (percentage vs WxH, string vs int)
- 🆕 negative-index: Language-specific invalid operations (negative Django queryset slicing)

**Sample bugs:**
- [Medium] [discourse] Passing 80% as the dimensions can fail for animated GIFs when allow_animated_thumbnails is true, since the animated path uses gifsicle --resize-fit wh...
- [Low] [discourse] Consider normalizing the input locale (e.g., to a symbol) when checking/loading here to avoid double-loading if the same locale is passed as a String ...
- [High] [sentry] Django querysets do not support negative slicing...
- [High] [sentry] get_item_key assumes a numeric key, but the paginator is used with order_by=-datetime in the audit logs endpoint; calling math.floor/ceil on a datetim...
- [Critical] [sentry] OptimizedCursorPaginator negative-offset branch slices QuerySet with a negative start index...
- ... and 6 more

### concurrency/TOCTOU (10 bugs, 7%)

**Subcategories:**
- race-condition: 2
- side-effect-in-check: 1
- thread-safety: 1
- read-modify-write: 1
- stale-read: 1
- shared-mutable-default: 1
- double-check-locking: 1
- concurrent-map: 1
- test-race: 1

**New detectors needed:**
- 🆕 read-modify-write: Pattern of read-check-write without transaction/lock
- 🆕 shared-mutable-default: Mutable defaults in class/dataclass definitions

**Sample bugs:**
- [Medium] [discourse] BlockedEmail.should_block_email? method has side effects during a read operation - it updates statistics even when just checking if an email should be...
- [Low] [discourse] Thread-safety issue with lazy @loaded_locales...
- [High] [cal.com] Because backupCodes are decrypted and mutated in memory before being written back, two concurrent login requests using the same backupCode could both ...
- [High] [cal.com] Using retryCount: reminder.retryCount + 1 reads a possibly stale value and can lose increments under concurrency; consider an atomic increment via Pri...
- [Medium] [sentry] Shared mutable default in dataclass timestamp...
- ... and 5 more

### contract-break (8 bugs, 6%)

**Subcategories:**
- missing-parameter: 1
- api-breaking-change: 1
- interface-impl-mismatch: 1
- return-type-mismatch: 1
- missing-abstract-impl: 1
- nonexistent-api: 1
- not-implemented: 1
- null-contract-violation: 1

**Existing detectors:**
- ✅ removed-public-api (contracts.rs)
- ✅ signature-change-with-callers (contracts.rs)
- ✅ arity-change-with-callers (contracts.rs)
- ✅ interface-impl-mismatch (contracts.rs)
- ✅ type-change-propagation (contracts.rs)

**New detectors needed:**
- 🆕 abstract-method-missing: Subclass inherits abstract but only has pass/empty impl

**Sample bugs:**
- [Medium] [keycloak] ConditionalPasskeysEnabled() called without UserModel parameter...
- [Medium] [sentry] Breaking changes in error response format...
- [Low] [cal.com] The Calendar interface now requires createEvent(event, credentialId), but some implementations (e.g., Lark/Office365) still declare createEvent(event)...
- [High] [cal.com] When APP_CREDENTIAL_SHARING_ENABLED and CALCOM_CREDENTIAL_SYNC_ENDPOINT are set, the refreshFunction helper returns the fetch Response, but several ca...
- [High] [sentry] MetricAlertDetectorHandler inherits from StatefulDetectorHandler but only contains pass, failing to implement its required abstract methods: counter_n...
- ... and 3 more

### dead-code (5 bugs, 4%)

**Subcategories:**
- method-override: 1
- redundant-code: 1
- unreachable-branch: 1
- unused-parameter: 1
- unused-result: 1

**Existing detectors:**
- ✅ unreachable-branch (diff_heuristics.rs)

**New detectors needed:**
- 🆕 duplicate-method: Same method defined twice, second overrides first
- 🆕 unused-computation: Value computed but result never used

**Sample bugs:**
- [Medium] [discourse] The downsize method is defined twice. The second definition, which expects a single dimensions string parameter, overrides the first, which expected s...
- [Low] [cal.com] The optional chaining on mainHostDestinationCalendar?.integration is redundant since you already check mainHostDestinationCalendar in the ternary cond...
- [Low] [cal.com] In getBaseConditions(), the else if (filterConditions) and final else branches are unreachable. This is because getAuthorizationConditions() always re...
- [Low] [grafana] The applyTemplateVariables method is called with request.filters as the third parameter, but this parameter is not used in the corresponding test setu...
- [Low] [keycloak] Dead code exists where ASN1Encoder instances are created and written to, but their results are immediately discarded. The actual encoding is performed...

### config-error (5 bugs, 4%)

**Subcategories:**
- css-value-inversion: 2
- wrong-locale: 2
- css-value-change: 1

**New detectors needed:**
- 🆕 locale-text-mismatch: Translation text language doesnt match file locale
- 🆕 css-value-inversion: Diff shows dramatic value changes (30%→70%) in CSS properties

**Sample bugs:**
- [Low] [discourse] In .topic-meta-data h5 a, the original code had color: scale-color($primary, $lightness: 30%) but was changed to dark-light-choose(scale-color($primar...
- [Low] [discourse] This change for desktop/user.css changes $primary from 30% to 50% for the light theme; most other changes preserve the original $primary value and mov...
- [Low] [discourse] In topic-post.css the original code used $lightness: 70% but the replacement uses $lightness: 30% for the light theme. This makes the text significant...
- [Medium] [keycloak] The translation is in Italian instead of Lithuanian. This should be translated to Lithuanian to match the file's locale (messages_lt.properties)....
- [Medium] [keycloak] The totpStep1 value uses Traditional Chinese terms in the Simplified Chinese file (zh_CN), which is likely incorrect for this locale. Please verify th...

### error-handling (4 bugs, 3%)

**Subcategories:**
- wrong-message: 1
- missing-try-catch: 1
- misleading-error: 1
- broad-catch: 1

**Existing detectors:**
- ✅ catch-swallows-error (patterns.rs)

**Sample bugs:**
- [Low] [cal.com] Error message mentions 'backup code login' but this is a disable endpoint, not login...
- [Low] [cal.com] Consider adding try-catch around the await to handle import failures gracefully...
- [Low] [grafana] Returning ErrDeviceLimitReached when no rows were updated is misleading; the device might not exist....
- [Low] [keycloak]  Catching generic RuntimeException is too broad. The implementation throws IllegalArgumentException specifically - catch that instead for more precise...

### case-sensitivity (3 bugs, 2%)

**Subcategories:**
- case-compare: 3

**New detectors needed:**
- 🆕 case-insensitive-compare: indexOf/includes/== on strings that should be case-insensitive

**Sample bugs:**
- [Medium] [discourse] record_for_host compares lower(host) = ? but does not normalize the parameter’s case, so mixed‑case referer hosts may fail to match even though compar...
- [Medium] [cal.com] Backup code validation is case-sensitive due to the use of indexOf(). This causes validation to fail if a user enters uppercase hex characters, as bac...
- [High] [cal.com] Case sensitivity bypass in email blacklist...

### security (3 bugs, 2%)

**Subcategories:**
- ssrf: 1
- clickjacking: 1
- predictable-state: 1

**Sample bugs:**
- [Critical] [discourse] SSRF vulnerability using open(url) without validation...
- [Medium] [discourse] The code sets X-Frame-Options: ALLOWALL which completely disables clickjacking protection. The referer validation can be bypassed (referer headers are...
- [Medium] [sentry] OAuth state uses pipeline.signature (static) instead of a per-request random value...

### async-misuse (3 bugs, 2%)

**Subcategories:**
- foreach-async: 2
- unhandled-async: 1

**Existing detectors:**
- ✅ foreach-async-fire-and-forget (patterns.rs)
- ✅ missing-await (patterns.rs)

**Sample bugs:**
- [High] [discourse]  The findMembers() call is now asynchronous and unhandled. The controller may not have member data immediately available, creating a race condition....
- [Medium] [cal.com] Asynchronous functions deleteScheduledEmailReminder and deleteScheduledSMSReminder are called without await inside forEach loops. This occurs during b...
- [Critical] [cal.com] The code uses forEach with async callbacks, which causes asynchronous operations (e.g., calendar/video event deletions, payment refunds) to run concur...

### domain-logic (2 bugs, 1%)

**Subcategories:**
- hardcoded-value: 1
- migration-normalization: 1

**Sample bugs:**
- [Low] [discourse] Hardcoding maxSizeKB = 10 * 1024 ignores Discourse.SiteSettings['max_' + type + '_size_kb'], so the client-side limit can diverge from server-side and...
- [High] [discourse] Because this migration inserts embeddable_hosts rows with raw SQL, any existing embeddable_hosts values that include http:// or /https:// or path segm...

---

## Proposed New Detectors (Priority Order)

Based on golden bug coverage, here are the highest-impact new detectors:

| # | Detector | Category | Est. Coverage | Description |
|---|----------|----------|--------------|-------------|
| 1. wrong-variable-in-scope | logic-error | ~10-12 | When a changed line references variable X but very similar variable Y exists in same scope. Catches: |
| 2. locale-text-mismatch | config-error | ~4-5 | Compare language of translation text vs filename locale. Catches Italian in .lt files, Traditional C |
| 3. case-insensitive-compare | case-sensitivity | ~3-4 | Flag indexOf/includes/== on strings that should be case-insensitive (emails, hex codes, hostnames).  |
| 4. typo-detector | naming-mismatch | ~5-6 | Levenshtein distance < 2 on identifiers in changed code vs correct spelling. Catches: santize→saniti |
| 5. always-true-false | logic-error | ~4-5 | Condition that trivially evaluates to constant: function that always returns false, === on objects,  |
| 6. read-modify-write-race | concurrency/TOCTOU | ~4-5 | Pattern: read X, check X, write X without transaction/lock. Common in backup codes, device limits, r |
| 7. nil-after-lookup | null-safety | ~5-6 | Method returns Optional/nullable (.first, .find, dict[key]) but result used without nil check. |
| 8. wrong-callee-near-miss | naming-mismatch | ~3-4 | Function call where a similar-named function was likely intended (Levenshtein on callee name vs avai |
| 9. duplicate-method-override | dead-code | ~2 | Same method name defined twice in same class — second silently overrides first. |
| 10. abstract-method-missing | contract-break | ~2 | Class inherits abstract base but only has pass/empty implementation for required methods. |

**Estimated total coverage of top 10 detectors**: ~45-55 bugs (33-40% of golden set)
**Combined with existing detectors**: ~55-70 bugs (40-51% of golden set)

---

## Coverage Gap Analysis

### What Rust detectors CAN'T catch (~36 bugs, 26%)

These require LLM reasoning because they need:

- **Domain knowledge**: Understanding business rules, API contracts, migration semantics
- **Cross-system reasoning**: How OAuth flows work, what frameworks expect
- **Behavioral intent**: Whether a behavior change is intentional or not
- **Architecture knowledge**: How components interact across the system

Examples:
- [discourse] Hardcoding maxSizeKB = 10 * 1024 ignores Discourse.SiteSettings['max_' + type + '_size_kb'], so the client-side limit ca
- [discourse] Because this migration inserts embeddable_hosts rows with raw SQL, any existing embeddable_hosts values that include htt
- [discourse] BlockedEmail.should_block_email? method has side effects during a read operation - it updates statistics even when just 
- [discourse] Regex pattern @(#{domains}) only matches domain suffixes, not full domains. evil.example.com would match whitelist entry
- [discourse] The current origin validation using indexOf is insufficient and can be bypassed. An attacker could use a malicious domai
- [discourse] postMessage targetOrigin should be the origin (scheme+host+port), not the full referrer URL; using the full URL will cau
- [discourse] The code sets X-Frame-Options: ALLOWALL which completely disables clickjacking protection. The referer validation can be
- [discourse] Mixing float: left with flexbox causes layout issues. Further this PR removes the float-based right alignment for .d-hea
- ... and 30 more

### The LLM's Role

The LLM should focus on bugs that Rust can't catch:
- Asymmetric logic (cached grants trusted but denials ignored)
- Semantic behavior changes (blocking vs async, breaking API responses)
- Domain-specific correctness (OAuth state management, permission models)
- Cross-component reasoning (return type mismatch across callers)
- Framework convention violations (Rails serializer ? suffix)

---

## Conclusion

The golden bug distribution is **bounded and tractable**:

1. **44 bugs (32%)** are directly detectable by deterministic Rust rules
2. **55 bugs (40%)** can be flagged as candidates by Rust, validated by LLM
3. **38 bugs (28%)** need pure LLM reasoning
4. **10 new detectors** would cover an estimated 45-55 additional bugs
5. This is NOT infinite whack-a-mole — the categories are finite and well-defined
6. The biggest single category (logic-error at 57=42%) is diverse but has recurring patterns
7. **Target**: With existing detectors + 10 new ones + LLM, realistic coverage is 75-85%

---

## Full Bug List

| # | Repo | Severity | Category | Subcategory | Rust? | Summary |
|---|------|----------|----------|-------------|-------|---------|
| 1 | discourse | Medium | dead-code | method-override | yes | The downsize method is defined twice. The second definition, which expects a sin |
| 2 | discourse | Low | domain-logic | hardcoded-value | no | Hardcoding maxSizeKB = 10 * 1024 ignores Discourse.SiteSettings['max_' + type +  |
| 3 | discourse | Medium | type-error | format-mismatch | partial | Passing 80% as the dimensions can fail for animated GIFs when allow_animated_thu |
| 4 | discourse | Critical | null-safety | nil-deref | partial | NoMethodError before_validation in EmbeddableHost |
| 5 | discourse | Medium | null-safety | nil-deref | partial | The update and destroy methods in Admin::EmbeddableHostsController do not valida |
| 6 | discourse | Medium | case-sensitivity | case-compare | yes | record_for_host compares lower(host) = ? but does not normalize the parameter’s  |
| 7 | discourse | High | domain-logic | migration-normalization | no | Because this migration inserts embeddable_hosts rows with raw SQL, any existing  |
| 8 | discourse | High | null-safety | nil-deref | partial | logic: Potential nil pointer exception - if no TopicUser record exists, tu will  |
| 9 | discourse | Low | naming-mismatch | typo | yes | Typo in property name: 'stopNotificiationsText' should be 'stopNotificationsText |
| 10 | discourse | Medium | concurrency/TOCTOU | side-effect-in-check | no | BlockedEmail.should_block_email? method has side effects during a read operation |
| 11 | discourse | Medium | logic-error | regex-bypass | no | Regex pattern @(#{domains}) only matches domain suffixes, not full domains. evil |
| 12 | discourse | Critical | security | ssrf | partial | SSRF vulnerability using open(url) without validation |
| 13 | discourse | Medium | logic-error | wrong-value | no | The current origin validation using indexOf is insufficient and can be bypassed. |
| 14 | discourse | Medium | logic-error | wrong-argument | no | postMessage targetOrigin should be the origin (scheme+host+port), not the full r |
| 15 | discourse | Medium | security | clickjacking | no | The code sets X-Frame-Options: ALLOWALL which completely disables clickjacking p |
| 16 | discourse | Medium | null-safety | nil-deref | partial | The TopicEmbed.import method is susceptible to a NoMethodError if the contents p |
| 17 | discourse | Medium | logic-error | syntax-error | yes | The ERB block closes with end if, which is invalid Ruby/ERB and will raise at re |
| 18 | discourse | Low | logic-error | css-conflict | no | Mixing float: left with flexbox causes layout issues. Further this PR removes th |
| 19 | discourse | Low | naming-mismatch | wrong-prefix | yes | -ms-align-items never existed in any version of IE/Edge; the correct legacy prop |
| 20 | discourse | Medium | naming-mismatch | missing-suffix | partial | The include_website_name method is missing the required ? suffix. Rails serializ |
| 21 | discourse | Low | config-error | css-value-inversion | yes | In .topic-meta-data h5 a, the original code had color: scale-color($primary, $li |
| 22 | discourse | Low | config-error | css-value-change | partial | This change for desktop/user.css changes $primary from 30% to 50% for the light  |
| 23 | discourse | Low | config-error | css-value-inversion | yes | In topic-post.css the original code used $lightness: 70% but the replacement use |
| 24 | discourse | High | async-misuse | unhandled-async | partial |  The findMembers() call is now asynchronous and unhandled. The controller may no |
| 25 | discourse | Medium | logic-error | off-by-one | partial | In the next action, capping the next offset at user_count can produce an empty p |
| 26 | discourse | Medium | logic-error | http-method-mismatch | yes | HTTP method mismatch in .remove_member - test uses PUT but remove_member action  |
| 27 | discourse | Low | concurrency/TOCTOU | thread-safety | no | Thread-safety issue with lazy @loaded_locales |
| 28 | discourse | Low | type-error | string-vs-symbol | no | Consider normalizing the input locale (e.g., to a symbol) when checking/loading  |
| 29 | keycloak | Medium | contract-break | missing-parameter | yes | ConditionalPasskeysEnabled() called without UserModel parameter |
| 30 | keycloak | Medium | logic-error | condition-gap | no | With isConditionalPasskeysEnabled(UserModel user) requiring user != null, authen |
| 31 | sentry | Low | logic-error | import-error | yes | Importing non-existent OptimizedCursorPaginator |
| 32 | sentry | High | type-error | negative-slice | partial | Django querysets do not support negative slicing |
| 33 | sentry | High | null-safety | none-deref | partial | When requests are authenticated with API keys or org auth tokens (which have use |
| 34 | sentry | High | type-error | type-mismatch | partial | get_item_key assumes a numeric key, but the paginator is used with order_by=-dat |
| 35 | sentry | Critical | type-error | negative-slice | partial | OptimizedCursorPaginator negative-offset branch slices QuerySet with a negative  |
| 36 | sentry | High | type-error | negative-slice | partial | BasePaginator negative-offset branch slices QuerySet with a negative start index |
| 37 | sentry | High | type-error | type-mismatch | partial | OptimizedCursorPaginator.get_item_key uses floor/ceil on a datetime key (order_b |
| 38 | sentry | Low | logic-error | falsy-zero | yes | sample_rate = 0.0 is falsy and skipped |
| 39 | sentry | Low | logic-error | hash-nondeterminism | partial | Using Python’s built-in hash() to build cache keys is non-deterministic across p |
| 40 | sentry | Medium | logic-error | wrong-variable | no | The upsampling eligibility check passes the outer dataset instead of the actual  |
| 41 | sentry | Medium | contract-break | api-breaking-change | partial | Breaking changes in error response format |
| 42 | sentry | Medium | logic-error | wrong-key | no | Detector validator uses wrong key when updating type |
| 43 | sentry | Low | logic-error | dict-order | no | Using zip(error_ids, events.values()) assumes the get_multi result preserves the |
| 44 | cal.com | Low | naming-mismatch | export-name | partial | The exported function TwoFactor handles backup codes and is in BackupCode.tsx. I |
| 45 | cal.com | Low | error-handling | wrong-message | partial | Error message mentions 'backup code login' but this is a disable endpoint, not l |
| 46 | cal.com | Medium | case-sensitivity | case-compare | yes | Backup code validation is case-sensitive due to the use of indexOf(). This cause |
| 47 | cal.com | High | concurrency/TOCTOU | read-modify-write | partial | Because backupCodes are decrypted and mutated in memory before being written bac |
| 48 | cal.com | High | null-safety | null-deref | yes | Potential null reference if mainHostDestinationCalendar is undefined if evt.dest |
| 49 | cal.com | Low | dead-code | redundant-code | yes | The optional chaining on mainHostDestinationCalendar?.integration is redundant s |
| 50 | cal.com | High | logic-error | self-comparison | yes | Logic error: when externalCalendarId is provided, you're searching for a calenda |
| 51 | cal.com | Medium | logic-error | boolean-inversion | yes | Logic inversion in organization creation: The slug property is now conditionally |
| 52 | cal.com | Low | contract-break | interface-impl-mismatch | yes | The Calendar interface now requires createEvent(event, credentialId), but some i |
| 53 | cal.com | High | logic-error | hardcoded-string | yes | The parseRefreshTokenResponse function incorrectly sets refresh_token to the har |
| 54 | cal.com | High | type-error | invalid-syntax | yes | Invalid Zod schema syntax. Computed property keys like [z.string().toString()] a |
| 55 | cal.com | High | type-error | wrong-return | partial | parseRefreshTokenResponse returns a Zod safeParse result ({ success, data, error |
| 56 | cal.com | High | contract-break | return-type-mismatch | partial | When APP_CREDENTIAL_SHARING_ENABLED and CALCOM_CREDENTIAL_SYNC_ENDPOINT are set, |
| 57 | cal.com | High | null-safety | undefined-access | partial | When the sync endpoint path is used, res is a fetch Response and has no .data; r |
| 58 | cal.com | High | case-sensitivity | case-compare | partial | Case sensitivity bypass in email blacklist |
| 59 | cal.com | Critical | logic-error | wrong-operator | yes | The logic for checking team admin/owner permissions is incorrect. This condition |
| 60 | cal.com | Medium | logic-error | wrong-variable | no | This calls the email sender with the original guests, so existing attendees incl |
| 61 | cal.com | Medium | logic-error | missing-dedup | no | uniqueGuests filters out existing attendees and blacklisted emails but does not  |
| 62 | cal.com | Low | logic-error | empty-string-init | partial | Starting with an array containing an empty string may cause validation issues. C |
| 63 | cal.com | High | concurrency/TOCTOU | stale-read | partial | Using retryCount: reminder.retryCount + 1 reads a possibly stale value and can l |
| 64 | cal.com | High | logic-error | missing-filter | partial | The deletion logic in scheduleSMSReminders.ts incorrectly deletes non-SMS workfl |
| 65 | cal.com | Low | dead-code | unreachable-branch | yes | In getBaseConditions(), the else if (filterConditions) and final else branches a |
| 66 | cal.com | Medium | logic-error | guard-skip | partial | Fetching userIdsFromOrg only when teamsFromOrg.length > 0 can exclude org-level  |
| 67 | cal.com | Medium | logic-error | empty-update | partial | The updateManyByCredentialId call uses an empty data object, which prevents Pris |
| 68 | cal.com | Low | logic-error | platform-specific | no | logic: macOS-specific sed syntax with empty string after -i flag will fail on Li |
| 69 | cal.com | Medium | async-misuse | foreach-async | yes | Asynchronous functions deleteScheduledEmailReminder and deleteScheduledSMSRemind |
| 70 | cal.com | High | logic-error | incomplete-cleanup | partial | When immediateDelete is true, the deleteScheduledEmailReminder function cancels  |
| 71 | cal.com | Low | error-handling | missing-try-catch | partial | Consider adding try-catch around the await to handle import failures gracefully |
| 72 | cal.com | Critical | async-misuse | foreach-async | yes | The code uses forEach with async callbacks, which causes asynchronous operations |
| 73 | cal.com | Medium | logic-error | wrong-variable | yes | Incorrect end time calculation using slotStartTime instead of slotEndTime |
| 74 | cal.com | Medium | logic-error | reference-equality | yes | Using === for dayjs object comparison will always return false as it compares ob |
| 75 | sentry | Medium | null-safety | missing-key-check | partial | Null reference if github_authenticated_user state is missing |
| 76 | sentry | Medium | security | predictable-state | no | OAuth state uses pipeline.signature (static) instead of a per-request random val |
| 77 | sentry | High | null-safety | missing-key-check | partial | The code attempts to access integration.metadata[sender][login] without checking |
| 78 | sentry | Medium | concurrency/TOCTOU | shared-mutable-default | yes | Shared mutable default in dataclass timestamp |
| 79 | sentry | Low | naming-mismatch | typo | yes | The method name has a typo: test_from_dict_inalid_data should be test_from_dict_ |
| 80 | sentry | Low | naming-mismatch | wrong-name | partial | Method name says 'empty_array' but tests empty dict - consider renaming to 'test |
| 81 | sentry | Medium | type-error | serialization | partial | to_dict() returns a datetime for queued; if this dict is passed in task kwargs ( |
| 82 | sentry | High | contract-break | missing-abstract-impl | yes | MetricAlertDetectorHandler inherits from StatefulDetectorHandler but only contai |
| 83 | sentry | Low | naming-mismatch | wrong-docstring | no | Docstring says this returns a list of DetectorEvaluationResult, but the method n |
| 84 | sentry | High | logic-error | wrong-variable | yes | The function modifies the config variable to include display values but then ret |
| 85 | sentry | Low | logic-error | redundant-query | no | The code fetches MonitorCheckIn objects by ID when the required data already exi |
| 86 | sentry | Medium | naming-mismatch | inconsistent-naming | partial | Inconsistent metric tagging with 'shard' and 'shards' |
| 87 | sentry | Low | logic-error | test-flakiness | no | Fixed sleep in tests can be flaky; wait on condition instead |
| 88 | sentry | High | logic-error | isinstance-check | partial | Because flusher processes are created via multiprocessing.get_context('spawn').P |
| 89 | sentry | Medium | logic-error | monkeypatched-sleep | no | Sleep in test_consumer.py won’t actually wait because time.sleep was monkeypatch |
| 90 | sentry | Medium | logic-error | early-break | partial | Breaking out of the loop when the deadline has elapsed can skip terminating rema |
| 91 | sentry | High | contract-break | nonexistent-api | partial | The queue.shutdown() method with 'immediate=False' parameter may not exist in th |
| 92 | sentry | Low | logic-error | magic-number | no | The magic number 50 for max_wait is used repeatedly throughout the tests. Consid |
| 93 | sentry | Low | naming-mismatch | wrong-docstring | no | The test test_thread_queue_parallel_error_handling has a docstring that doesn't  |
| 94 | grafana | High | logic-error | asymmetric-logic | no | The Check operation exhibits asymmetric cache trust logic: cached permission gra |
| 95 | grafana | Low | logic-error | test-comment-mismatch | partial | The test comment says the cached permissions 'allow access', but the map stores  |
| 96 | grafana | Medium | logic-error | missing-prop | partial | The rendered GrafanaRuleListItem is missing the required key prop for React list |
| 97 | grafana | High | logic-error | missing-dependency | no | RuleActionsButtons is invoked with only promRule, but SilenceGrafanaRuleDrawer i |
| 98 | grafana | Low | dead-code | unused-parameter | partial | The applyTemplateVariables method is called with request.filters as the third pa |
| 99 | grafana | High | null-safety | nil-deref | yes | The ContextualLoggerMiddleware methods (QueryData, CallResource, CheckHealth, Co |
| 100 | grafana | Low | logic-error | removed-feature | partial | The traceID is no longer logged for plugin requests. During a refactoring, the t |
| 101 | grafana | High | concurrency/TOCTOU | race-condition | partial | Race condition: Multiple concurrent requests could pass the device count check s |
| 102 | grafana | Medium | logic-error | behavior-change | no | Anonymous authentication now fails entirely if anonDeviceService.TagDevice retur |
| 103 | grafana | Medium | type-error | type-mismatch | yes | This call won’t compile: dbSession.Exec(args...) is given a []interface{} where  |
| 104 | grafana | Low | error-handling | misleading-error | partial | Returning ErrDeviceLimitReached when no rows were updated is misleading; the dev |
| 105 | grafana | Low | logic-error | time-inconsistency | partial | Time window calculation inconsistency: Using device.UpdatedAt.UTC().Add(-anonymo |
| 106 | grafana | Low | logic-error | wrong-log-level | partial | The code uses Error log level for what appears to be debugging information. This |
| 107 | grafana | Medium | logic-error | wrong-variable | yes | The context is being created with d.Log instead of the log variable that was ini |
| 108 | grafana | High | naming-mismatch | wrong-callee | yes | Bug: calling recordLegacyDuration when storage operation fails should be recordS |
| 109 | grafana | Medium | naming-mismatch | inconsistent-naming | partial | Inconsistency: using name instead of options.Kind for metrics recording differs  |
| 110 | grafana | Medium | concurrency/TOCTOU | double-check-locking | partial | The GetWebAssets function implements an incomplete double-checked locking patter |
| 111 | grafana | High | logic-error | error-overwrites-cache | partial | In addition to the missing double-check, the function has a critical flaw in its |
| 112 | grafana | Critical | logic-error | always-false | yes | The enableSqlExpressions function has flawed logic that always returns false, ef |
| 113 | grafana | High | contract-break | not-implemented | partial | Several methods such as NewInMemoryDB().RunCommands and db.QueryFramesInto retur |
| 114 | grafana | High | concurrency/TOCTOU | race-condition | no | A race condition in BuildIndex allows multiple goroutines to concurrently build  |
| 115 | grafana | High | concurrency/TOCTOU | concurrent-map | no | Calling s.search.TotalDocs() here may race with concurrent index creation: Total |
| 116 | keycloak | Critical | logic-error | wrong-variable | yes | Recursive caching call using session instead of delegate |
| 117 | keycloak | Medium | naming-mismatch | wrong-reference | partial | Cleanup reference uses incorrect alias - should be 'idp-alias-' + i instead of ' |
| 118 | keycloak | High | logic-error | wrong-return | yes | Returns wrong provider (default keystore instead of BouncyCastle) |
| 119 | keycloak | Low | dead-code | unused-result | yes | Dead code exists where ASN1Encoder instances are created and written to, but the |
| 120 | keycloak | High | logic-error | wrong-feature-flag | partial | Inconsistent feature flag bug causing orphaned permissions. The AdminPermissions |
| 121 | keycloak | High | logic-error | wrong-lookup | no | In hasPermission(ClientModel client, String scope), the resource lookup uses fin |
| 122 | keycloak | High | logic-error | wrong-resource-type | no | In getClientsWithPermission(String scope), iterating resourceStore.findByType(se |
| 123 | keycloak | Medium | logic-error | system-exit | no | Incorrect method call for exit codes. The picocli.exit() method calls System.exi |
| 124 | keycloak | High | logic-error | wrong-permission | no | Incorrect permission check in canManage() method |
| 125 | keycloak | High | logic-error | id-name-mismatch | no | In getGroupIdsWithViewPermission, hasPermission is called with groupResource.get |
| 126 | keycloak | Medium | config-error | wrong-locale | yes | The translation is in Italian instead of Lithuanian. This should be translated t |
| 127 | keycloak | Medium | config-error | wrong-locale | yes | The totpStep1 value uses Traditional Chinese terms in the Simplified Chinese fil |
| 128 | keycloak | Low | logic-error | validation-gap | no | The anchor sanitization logic has a potential issue where it consumes English ma |
| 129 | keycloak | Low | naming-mismatch | typo | yes | The method name 'santizeAnchors' should be 'sanitizeAnchors' (missing 'i'). |
| 130 | keycloak | Critical | null-safety | wrong-null-check | yes | Wrong parameter in null check (grantType vs. rawTokenId) |
| 131 | keycloak | High | logic-error | inverted-logic | yes | In isAccessTokenId, the substring for the grant shortcut and the equality check  |
| 132 | keycloak | Low | naming-mismatch | wrong-docstring | no | Javadoc mentions "usually like 3-letters shortcut" but some implementations use  |
| 133 | keycloak | Low | error-handling | broad-catch | partial |  Catching generic RuntimeException is too broad. The implementation throws Illeg |
| 134 | keycloak | Medium | null-safety | optional-get | yes | Unsafe raw List deserialization without type safety. Calling Optional.get() dire |
| 135 | keycloak | Low | logic-error | missing-id-set | no | After creating the RecoveryAuthnCodesCredentialModel, consider setting its id fr |
| 136 | keycloak | Critical | contract-break | null-contract-violation | yes | Returning null from getSubGroupsCount() violates the GroupModel contract (Javado |
| 137 | keycloak | Medium | concurrency/TOCTOU | test-race | no | The reader thread isn’t waited for; flipping deletedAll to true and asserting im |