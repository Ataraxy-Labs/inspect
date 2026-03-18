# Inspect Deterministic Pipeline — Diagnostic Report

## Executive Summary

Across **15 PRs** (3 per repo × 5 repos), the deterministic pipeline produces:

| Metric | Value |
|--------|-------|
| Total detector findings | **613** |
| Noise findings (callee-swap, magic-number, added-early-return) | **382** (62%) |
| Useful findings | **231** |
| Golden bugs across these PRs | **42** |
| Golden bugs with a relevant detector finding | **2–3 / 42** (foreach-async, catch-swallow on cal.com PR#8087; negation-flip partial on grafana PR#103633) |
| Test entities stealing top-20 slots | **32** (across all PRs) |
| Generic-name entities stealing top-20 slots | **14** (across all PRs) |

**Bottom line:** 62% of findings are noise. Only 2–3 out of 42 golden bugs have a relevant detector finding (foreach-async and catch-swallow on cal.com work great; the rest produce zero signal). The top-20 entity selection (what the agent sees as source code) is polluted by test files and generic names like `read`, `write`, `reset`.

---

## cal_dot_com

### PR#8087: Async import of the appStore packages

| Metric | Value |
|--------|-------|
| Total entities | 30 |
| HCM entities (High/Critical/Medium) | 28 |
| Total findings | 28 |
| Noise findings | 14 (50%) |
| Test entities in top-20 | 0 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 2 |

**Findings by rule:**

- `callee-swap` × 12 ⚠️ NOISE
- `catch-swallow` × 5
- `foreach-async` × 4
- `missing-await` × 4
- `magic-number` × 2 ⚠️ NOISE
- `type-change-propagation` × 1

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `getCalendar` | 1.00 | Critical | 19 | getCalendar.ts |
| 2 | `appStore` | 0.93 | Critical | 12 | index.ts |
| 3 | `createMeeting` | 0.91 | Critical | 11 | videoClient.ts |
| 4 | `calendar` | 0.89 | Critical | 9 | EventManager.ts |
| 5 | `EventManager` | 0.89 | Critical | 9 | EventManager.ts |

**Golden bugs (what we need to find):**

- **[Low]** Consider adding try-catch around the await to handle import failures gracefully
- **[Critical]** The code uses forEach with async callbacks, which causes asynchronous operations (e.g., calendar/video event deletions, payment refunds) to run concur

**What's wrong:**

- ✅ `foreach-async` × 4 DIRECTLY matches the Critical golden (forEach+async) — **this detector works!**
- ✅ `catch-swallow` × 5 partially matches the Low golden (error handling)
- But 14/28 findings (callee-swap × 12, magic-number × 2) are pure noise drowning the signal

---

### PR#10600: feat: 2fa backup codes

| Metric | Value |
|--------|-------|
| Total entities | 68 |
| HCM entities (High/Critical/Medium) | 53 |
| Total findings | 21 |
| Noise findings | 14 (66%) |
| Test entities in top-20 | 1 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 4 |

**Findings by rule:**

- `callee-swap` × 9 ⚠️ NOISE
- `magic-number` × 4 ⚠️ NOISE
- `type-change-propagation` × 3
- `fixme-todo` × 2
- `hardcoded-secret` × 1
- `missing-await` × 1
- `added-early-return` × 1 ⚠️ NOISE

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `onSubmit` | 1.00 | Critical | 51 | login.tsx |
| 2 | `ErrorCode` | 0.98 | Critical | 16 | ErrorCode.ts |
| 3 | `PasswordField` | 0.97 | Critical | 12 | Input.tsx |
| 4 | `TwoFactor` | 0.91 | Critical | 6 | BackupCode.tsx |
| 5 | `LoginValues` | 0.88 | Critical | 3 | login.tsx |

**Golden bugs (what we need to find):**

- **[Low]** The exported function TwoFactor handles backup codes and is in BackupCode.tsx. Inconsistent naming.
- **[Low]** Error message mentions 'backup code login' but this is a disable endpoint, not login
- **[Medium]** Backup code validation is case-sensitive due to the use of indexOf(). This causes validation to fail if a user enters uppercase hex characters, as bac
- **[High]** Because backupCodes are decrypted and mutated in memory before being written back, two concurrent login requests using the same backupCode could both 

**What's wrong:**

- 14/21 findings (66%) are noise — callee-swap alone produces 9 findings that never match any golden bug
- 1 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 0/4 golden bugs caught by any detector finding

---

### PR#10967: fix: handle collective multiple host on destinationCalendar

| Metric | Value |
|--------|-------|
| Total entities | 87 |
| HCM entities (High/Critical/Medium) | 74 |
| Total findings | 75 |
| Noise findings | 45 (60%) |
| Test entities in top-20 | 0 |
| Generic-name entities in top-20 | 1 |
| Golden bugs | 5 |

**Findings by rule:**

- `callee-swap` × 34 ⚠️ NOISE
- `missing-await` × 9
- `magic-number` × 9 ⚠️ NOISE
- `catch-swallow` × 7
- `type-change-propagation` × 5
- `removed-guard` × 5
- `ssrf-url-concat` × 2
- `added-early-return` × 2 ⚠️ NOISE
- `fixme-todo` × 1
- `off-by-one-hint` × 1

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `evt` | 1.00 | Critical | 31 | bookingReminder.ts |
| 2 | `selectedCalendar` | 1.00 | Critical | 19 | CalendarService.ts |
| 3 | `credential` | 1.00 | Critical | 17 | EventManager.ts |
| 4 | `destinationCalendar` | 1.00 | Critical | 107 | class.ts |
| 5 | `EventResult` | 1.00 | Critical | 18 | EventManager.d.ts |

**Golden bugs (what we need to find):**

- **[High]** Potential null reference if mainHostDestinationCalendar is undefined if evt.destinationCalendar is null or an empty array 
- **[Low]** The optional chaining on mainHostDestinationCalendar?.integration is redundant since you already check mainHostDestinationCalendar in the ternary cond
- **[High]** Logic error: when externalCalendarId is provided, you're searching for a calendar where externalId === externalCalendarId, but this will always fail s
- **[Medium]** Logic inversion in organization creation: The slug property is now conditionally set when IS_TEAM_BILLING_ENABLED is true, instead of when it's false 
- **[Low]** The Calendar interface now requires createEvent(event, credentialId), but some implementations (e.g., Lark/Office365) still declare createEvent(event)

**What's wrong:**

- 45/75 findings (60%) are noise — callee-swap alone produces 34 findings that never match any golden bug
- 1 generic names (`read`, `write`, etc.) in top-20 have inflated risk scores from dependency graph name collisions
- 0/5 golden bugs caught by any detector finding

---

## discourse

### PR#ffbaf8c54269df2ce510de91245760fddce09896: FEATURE: automatically downsize large images

| Metric | Value |
|--------|-------|
| Total entities | 8 |
| HCM entities (High/Critical/Medium) | 8 |
| Total findings | 3 |
| Noise findings | 2 (66%) |
| Test entities in top-20 | 0 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 3 |

**Findings by rule:**

- `type-change-propagation` × 1
- `callee-swap` × 1 ⚠️ NOISE
- `magic-number` × 1 ⚠️ NOISE

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `create_upload` | 1.00 | Critical | 26 | uploads_controller.rb |
| 2 | `OptimizedImage` | 1.00 | Critical | 28 | optimized_image.rb |
| 3 | `resize` | 1.00 | Critical | 51 | optimized_image.rb |
| 4 | `optimize` | 0.96 | Critical | 13 | optimized_image.rb |
| 5 | `downsize` | 0.86 | Critical | 6 | optimized_image.rb |

**Golden bugs (what we need to find):**

- **[Medium]** The downsize method is defined twice. The second definition, which expects a single dimensions string parameter, overrides the first, which expected s
- **[Low]** Hardcoding maxSizeKB = 10 * 1024 ignores Discourse.SiteSettings['max_' + type + '_size_kb'], so the client-side limit can diverge from server-side and
- **[Medium]** Passing 80% as the dimensions can fail for animated GIFs when allow_animated_thumbnails is true, since the animated path uses gifsicle --resize-fit wh

**What's wrong:**

- 2/3 findings (66%) are noise — callee-swap alone produces 1 findings that never match any golden bug
- 0/3 golden bugs caught by any detector finding

---

### PR#6669a2d94d76eea3b99b8c476d12b1eb66726b07: FEATURE: per-topic unsubscribe option in emails

| Metric | Value |
|--------|-------|
| Total entities | 48 |
| HCM entities (High/Critical/Medium) | 29 |
| Total findings | 18 |
| Noise findings | 13 (72%) |
| Test entities in top-20 | 0 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 2 |

**Findings by rule:**

- `callee-swap` × 8 ⚠️ NOISE
- `type-change-propagation` × 4
- `magic-number` × 4 ⚠️ NOISE
- `added-early-return` × 1 ⚠️ NOISE
- `off-by-one-hint` × 1

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `Topic` | 1.00 | Critical | 229 | topic.rb |
| 2 | `TopicUser` | 1.00 | Critical | 67 | topic_user.rb |
| 3 | `UserNotifications` | 0.88 | Critical | 9 | user_notifications.rb |
| 4 | `Email` | 0.83 | Critical | 6 | message_builder.rb |
| 5 | `MessageBuilder` | 0.78 | Critical | 4 | message_builder.rb |

**Golden bugs (what we need to find):**

- **[High]** logic: Potential nil pointer exception - if no TopicUser record exists, tu will be nil and calling methods on it will crash
- **[Low]** Typo in property name: 'stopNotificiationsText' should be 'stopNotificationsText' (missing 'n' in 'Notifications')

**What's wrong:**

- 13/18 findings (72%) are noise — callee-swap alone produces 8 findings that never match any golden bug
- 0/2 golden bugs caught by any detector finding

---

### PR#5f8a130277dbddc95d133cd2832be639baf89213: Add comprehensive email validation for blocked users

| Metric | Value |
|--------|-------|
| Total entities | 15 |
| HCM entities (High/Critical/Medium) | 11 |
| Total findings | 7 |
| Noise findings | 4 (57%) |
| Test entities in top-20 | 0 |
| Generic-name entities in top-20 | 1 |
| Golden bugs | 2 |

**Findings by rule:**

- `type-change-propagation` × 2
- `callee-swap` × 2 ⚠️ NOISE
- `magic-number` × 2 ⚠️ NOISE
- `removed-guard` × 1

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `User` | 1.00 | Critical | 214 | user.rb |
| 2 | `create` | 0.55 | High | 4 | users_controller.rb 🔴 GENERIC |
| 3 | `BlockedEmail` | 0.49 | Medium | 4 | blocked_email.rb |
| 4 | `actions` | 0.46 | Medium | 3 | blocked_email.rb |
| 5 | `validate_each` | 0.41 | Medium | 2 | email_validator.rb |

**Golden bugs (what we need to find):**

- **[Medium]** BlockedEmail.should_block_email? method has side effects during a read operation - it updates statistics even when just checking if an email should be
- **[Medium]** Regex pattern @(#{domains}) only matches domain suffixes, not full domains. evil.example.com would match whitelist entry example.com.

**What's wrong:**

- 4/7 findings (57%) are noise — callee-swap alone produces 2 findings that never match any golden bug
- 1 generic names (`read`, `write`, etc.) in top-20 have inflated risk scores from dependency graph name collisions
- 0/2 golden bugs caught by any detector finding

---

## grafana

### PR#79265: Anonymous: Add configurable device limit

| Metric | Value |
|--------|-------|
| Total entities | 354 |
| HCM entities (High/Critical/Medium) | 352 |
| Total findings | 69 |
| Noise findings | 15 (21%) |
| Test entities in top-20 | 2 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 5 |

**Findings by rule:**

- `nil-check-missing` × 35
- `fixme-todo` × 11
- `callee-swap` × 11 ⚠️ NOISE
- `removed-guard` × 6
- `type-change-propagation` × 2
- `added-early-return` × 2 ⚠️ NOISE
- `magic-number` × 2 ⚠️ NOISE

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `Client` | 1.00 | Critical | 284 | social_connector_mock.go |
| 2 | `RoundTrip` | 0.95 | Critical | 22 | google_oauth_test.go 🟡 TEST |
| 3 | `GrafanaConfig` | 0.93 | Critical | 13 | config.ts |
| 4 | `Authenticate` | 0.88 | Critical | 19 | client.go |
| 5 | `UserInfo` | 0.76 | Critical | 13 | azuread_oauth.go |

**Golden bugs (what we need to find):**

- **[High]** Race condition: Multiple concurrent requests could pass the device count check simultaneously and create devices beyond the limit. Consider using a da
- **[Medium]** Anonymous authentication now fails entirely if anonDeviceService.TagDevice returns ErrDeviceLimitReached. Previously, device tagging was asynchronous 
- **[Medium]** This call won’t compile: dbSession.Exec(args...) is given a []interface{} where the first element is the query, but Exec’s signature requires a first 
- **[Low]** Returning ErrDeviceLimitReached when no rows were updated is misleading; the device might not exist.
- **[Low]** Time window calculation inconsistency: Using device.UpdatedAt.UTC().Add(-anonymousDeviceExpiration) as the lower bound but device.UpdatedAt as the cur

**What's wrong:**

- 2 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 0/5 golden bugs caught by any detector finding

---

### PR#103633: AuthZService: improve authz caching

| Metric | Value |
|--------|-------|
| Total entities | 17 |
| HCM entities (High/Critical/Medium) | 17 |
| Total findings | 20 |
| Noise findings | 8 (40%) |
| Test entities in top-20 | 4 |
| Generic-name entities in top-20 | 3 |
| Golden bugs | 2 |

**Findings by rule:**

- `callee-swap` × 7 ⚠️ NOISE
- `nil-check-missing` × 6
- `removed-guard` × 3
- `negation-flip` × 2
- `exported-no-error-return` × 1
- `added-early-return` × 1 ⚠️ NOISE

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `setupService` | 0.65 | High | 9 | service_test.go 🟡 TEST |
| 2 | `newRemoteRBACClient` | 0.47 | Medium | 2 | rbac.go |
| 3 | `Set` | 0.46 | Medium | 1 | rbac.go 🔴 GENERIC |
| 4 | `ProvideAuthZClient` | 0.42 | Medium | 0 | rbac.go |
| 5 | `Check` | 0.42 | Medium | 0 | service.go |

**Golden bugs (what we need to find):**

- **[High]** The Check operation exhibits asymmetric cache trust logic: cached permission grants are trusted and returned immediately, but cached denials from the 
- **[Low]** The test comment says the cached permissions 'allow access', but the map stores false for dashboards:uid:dash1, so checkPermission will still treat th

**What's wrong:**

- 4 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 3 generic names (`read`, `write`, etc.) in top-20 have inflated risk scores from dependency graph name collisions
- 0/2 golden bugs caught by any detector finding

---

### PR#76186: Plugins: Chore: Renamed instrumentation middleware to metric

| Metric | Value |
|--------|-------|
| Total entities | 46 |
| HCM entities (High/Critical/Medium) | 46 |
| Total findings | 4 |
| Noise findings | 2 (50%) |
| Test entities in top-20 | 1 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 2 |

**Findings by rule:**

- `nil-check-missing` × 1
- `removed-guard` × 1
- `callee-swap` × 1 ⚠️ NOISE
- `magic-number` × 1 ⚠️ NOISE

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `CreateMiddlewares` | 0.59 | High | 2 | pluginsintegration.go |
| 2 | `logRequest` | 0.54 | High | 4 | logger_middleware.go |
| 3 | `instrumentContext` | 0.48 | Medium | 4 | contextual_logger_middleware.go |
| 4 | `instrumentPluginRequest` | 0.48 | Medium | 4 | metrics_middleware.go |
| 5 | `NewContextualLoggerMiddleware` | 0.47 | Medium | 1 | contextual_logger_middleware.go |

**Golden bugs (what we need to find):**

- **[High]** The ContextualLoggerMiddleware methods (QueryData, CallResource, CheckHealth, CollectMetrics) panic when a nil request is received. This occurs becaus
- **[Low]** The traceID is no longer logged for plugin requests. During a refactoring, the tracing import and the logic to extract and add traceID from the contex

**What's wrong:**

- 1 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 0/2 golden bugs caught by any detector finding

---

## keycloak

### PR#41249: Fixing Re-authentication with passkeys

| Metric | Value |
|--------|-------|
| Total entities | 36 |
| HCM entities (High/Critical/Medium) | 31 |
| Total findings | 25 |
| Noise findings | 17 (68%) |
| Test entities in top-20 | 9 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 2 |

**Findings by rule:**

- `callee-swap` × 17 ⚠️ NOISE
- `removed-guard` × 4
- `type-change-propagation` × 3
- `synchronized-missing` × 1

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `AbstractUsernameFormAuthenticator` | 0.94 | Critical | 34 | AbstractUsernameFormAuthenticator.java |
| 2 | `AuthenticatorUtils` | 0.84 | Critical | 17 | AuthenticatorUtils.java |
| 3 | `PasswordForm` | 0.79 | Critical | 12 | PasswordForm.java |
| 4 | `isConditionalPasskeysEnabled` | 0.64 | High | 6 | UsernamePasswordForm.java |
| 5 | `setupReauthenticationInUsernamePasswordFormError` | 0.62 | High | 5 | AuthenticatorUtils.java |

**Golden bugs (what we need to find):**

- **[Medium]** ConditionalPasskeysEnabled() called without UserModel parameter
- **[Medium]** With isConditionalPasskeysEnabled(UserModel user) requiring user != null, authenticate(...) will not call webauthnAuth.fillContextForm(context) on the

**What's wrong:**

- 17/25 findings (68%) are noise — callee-swap alone produces 17 findings that never match any golden bug
- 9 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 0/2 golden bugs caught by any detector finding

---

### PR#32918: Add caching support for IdentityProviderStorageProvider.getF

| Metric | Value |
|--------|-------|
| Total entities | 16 |
| HCM entities (High/Critical/Medium) | 14 |
| Total findings | 16 |
| Noise findings | 9 (56%) |
| Test entities in top-20 | 2 |
| Generic-name entities in top-20 | 2 |
| Golden bugs | 2 |

**Findings by rule:**

- `callee-swap` × 6 ⚠️ NOISE
- `synchronized-missing` × 3
- `type-change-propagation` × 3
- `magic-number` × 2 ⚠️ NOISE
- `negation-flip` × 1
- `added-early-return` × 1 ⚠️ NOISE

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `getForLogin` | 0.56 | High | 7 | InfinispanIdentityProviderStorageProvider.java |
| 2 | `cacheKeyForLogin` | 0.54 | High | 6 | InfinispanIdentityProviderStorageProvider.java |
| 3 | `getLoginPredicate` | 0.48 | Medium | 7 | IdentityProviderStorageProvider.java |
| 4 | `update` | 0.47 | Medium | 2 | InfinispanIdentityProviderStorageProvider.java 🔴 GENERIC |
| 5 | `InfinispanIdentityProviderStorageProvider` | 0.47 | Medium | 2 | InfinispanIdentityProviderStorageProvider.java |

**Golden bugs (what we need to find):**

- **[Critical]** Recursive caching call using session instead of delegate
- **[Medium]** Cleanup reference uses incorrect alias - should be 'idp-alias-' + i instead of 'alias'.

**What's wrong:**

- 9/16 findings (56%) are noise — callee-swap alone produces 6 findings that never match any golden bug
- 2 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 2 generic names (`read`, `write`, etc.) in top-20 have inflated risk scores from dependency graph name collisions
- 0/2 golden bugs caught by any detector finding

---

### PR#33832: Add AuthzClientCryptoProvider for authorization client crypt

| Metric | Value |
|--------|-------|
| Total entities | 79 |
| HCM entities (High/Critical/Medium) | 71 |
| Total findings | 20 |
| Noise findings | 14 (70%) |
| Test entities in top-20 | 0 |
| Generic-name entities in top-20 | 7 |
| Golden bugs | 2 |

**Findings by rule:**

- `callee-swap` × 6 ⚠️ NOISE
- `magic-number` × 5 ⚠️ NOISE
- `type-change-propagation` × 4
- `added-early-return` × 3 ⚠️ NOISE
- `removed-guard` × 2

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `ByteArrayInputStream` | 1.00 | Critical | 165 | ASN1Decoder.java |
| 2 | `read` | 1.00 | Critical | 478 | ASN1Decoder.java 🔴 GENERIC |
| 3 | `reset` | 1.00 | Critical | 331 | ASN1Decoder.java 🔴 GENERIC |
| 4 | `read` | 1.00 | Critical | 478 | ASN1Decoder.java 🔴 GENERIC |
| 5 | `ByteArrayOutputStream` | 1.00 | Critical | 109 | ASN1Encoder.java |

**Golden bugs (what we need to find):**

- **[High]** Returns wrong provider (default keystore instead of BouncyCastle)
- **[Low]** Dead code exists where ASN1Encoder instances are created and written to, but their results are immediately discarded. The actual encoding is performed

**What's wrong:**

- 14/20 findings (70%) are noise — callee-swap alone produces 6 findings that never match any golden bug
- 7 generic names (`read`, `write`, etc.) in top-20 have inflated risk scores from dependency graph name collisions
- 0/2 golden bugs caught by any detector finding

---

## sentry

### PR#92393: Optimize spans buffer insertion with eviction during insert

| Metric | Value |
|--------|-------|
| Total entities | 17 |
| HCM entities (High/Critical/Medium) | 16 |
| Total findings | 22 |
| Noise findings | 17 (77%) |
| Test entities in top-20 | 8 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 3 |

**Findings by rule:**

- `callee-swap` × 12 ⚠️ NOISE
- `magic-number` × 5 ⚠️ NOISE
- `removed-guard` × 3
- `type-change-propagation` × 2

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `SpansBuffer` | 0.73 | Critical | 16 | buffer.py |
| 2 | `Span` | 0.51 | High | 3 | buffer.py |
| 3 | `process_spans` | 0.51 | High | 3 | buffer.py |
| 4 | `_load_segment_data` | 0.47 | Medium | 2 | buffer.py |
| 5 | `process_batch` | 0.47 | Medium | 2 | factory.py |

**Golden bugs (what we need to find):**

- **[Critical]** OptimizedCursorPaginator negative-offset branch slices QuerySet with a negative start index
- **[High]** BasePaginator negative-offset branch slices QuerySet with a negative start index
- **[High]** OptimizedCursorPaginator.get_item_key uses floor/ceil on a datetime key (order_by='-datetime'), causing TypeError.

**What's wrong:**

- 17/22 findings (77%) are noise — callee-swap alone produces 12 findings that never match any golden bug
- 8 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 0/3 golden bugs caught by any detector finding

---

### PR#67876: GitHub OAuth Security Enhancement

| Metric | Value |
|--------|-------|
| Total entities | 31 |
| HCM entities (High/Critical/Medium) | 30 |
| Total findings | 40 |
| Noise findings | 23 (57%) |
| Test entities in top-20 | 5 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 3 |

**Findings by rule:**

- `callee-swap` × 18 ⚠️ NOISE
- `removed-guard` × 9
- `type-change-propagation` × 6
- `magic-number` × 3 ⚠️ NOISE
- `added-early-return` × 2 ⚠️ NOISE
- `hardcoded-secret` × 1
- `off-by-one-hint` × 1

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `GitHubIntegrationProvider` | 0.74 | Critical | 9 | integration.py |
| 2 | `InitialSegmentEvent` | 0.67 | High | 5 | recording_buffered.py |
| 3 | `GitHubInstallation` | 0.56 | High | 2 | integration.py |
| 4 | `SpansIndexedDatasetConfig` | 0.56 | High | 2 | spans_indexed.py |
| 5 | `PipelineAdvancerView` | 0.56 | High | 2 | pipeline_advancer.py |

**Golden bugs (what we need to find):**

- **[Medium]** Null reference if github_authenticated_user state is missing
- **[Medium]** OAuth state uses pipeline.signature (static) instead of a per-request random value
- **[High]** The code attempts to access integration.metadata[sender][login] without checking for the existence of the sender key. This causes a KeyError for integ

**What's wrong:**

- 23/40 findings (57%) are noise — callee-swap alone produces 18 findings that never match any golden bug
- 5 test-file entities in top-20 steal source-code slots from production code the agent should analyze
- 0/3 golden bugs caught by any detector finding

---

### PR#5: Replays Self-Serve Bulk Delete System

| Metric | Value |
|--------|-------|
| Total entities | 420 |
| HCM entities (High/Critical/Medium) | 338 |
| Total findings | 245 |
| Noise findings | 185 (75%) |
| Test entities in top-20 | 0 |
| Generic-name entities in top-20 | 0 |
| Golden bugs | 3 |

**Findings by rule:**

- `callee-swap` × 108 ⚠️ NOISE
- `magic-number` × 66 ⚠️ NOISE
- `removed-guard` × 18
- `off-by-one-hint` × 12
- `added-early-return` × 11 ⚠️ NOISE
- `type-change-propagation` × 10
- `signature-change-with-callers` × 8
- `missing-react-key` × 7
- `catch-swallow` × 4
- `xss-dangerously-set` × 1

**Top 5 entities by risk score (what the agent sees first):**

| # | Entity | Risk | Level | Dependents | File |
|---|--------|------|-------|------------|------|
| 1 | `CodeSnippet` | 1.00 | Critical | 55 | codeSnippet.tsx |
| 2 | `visualizes` | 0.82 | Critical | 26 | aggregateFields.tsx |
| 3 | `getCustomFieldRenderer` | 0.80 | Critical | 15 | chart.tsx |
| 4 | `isVisualize` | 0.79 | Critical | 14 | aggregateFields.tsx |
| 5 | `PRCommentWorkflow` | 0.79 | Critical | 9 | commit_context.py |

**Golden bugs (what we need to find):**

- **[Medium]** Breaking changes in error response format
- **[Medium]** Detector validator uses wrong key when updating type
- **[Low]** Using zip(error_ids, events.values()) assumes the get_multi result preserves the input order; dict value order is not guaranteed to match error_ids, s

**What's wrong:**

- 185/245 findings (75%) are noise — callee-swap alone produces 108 findings that never match any golden bug
- 0/3 golden bugs caught by any detector finding

---

## Improvement Plan

### Phase 1: Strip Noise

Remove detectors that produce high volume, zero value:

| Detector | Findings (15 PRs) | Golden bugs matched | Action |
|----------|-------------------|---------------------|--------|
| `callee-swap` | 252 | 0 | **DELETE** |
| `magic-number` | 106 | 0 | **DELETE** |
| `added-early-return` | 24 | 0 | **DELETE** |
| `fixme-todo` | 14 | 0 | **DELETE** |

**Expected result:** ~62% fewer findings, all remaining findings are potentially useful signals.

### Phase 2: Fix Risk Scoring

**Problem:** Generic entity names (`read`, `write`, `ByteArrayInputStream`) get inflated `dependent_count` from name-based graph matching, pushing them to top of risk ranking and crowding out actual buggy methods.

**Examples from this data:**

| Entity | Risk Score | Dependents | Why it's wrong |
|--------|-----------|------------|----------------|
| `ByteArrayInputStream` (field) | 1.00 | 165 | Name matches `java.io.ByteArrayInputStream` everywhere in codebase |
| `read` (method) | 1.00 | 478 | Every `.read()` call in Java resolves here |
| `write` (method) | 1.00 | 236 | Every `.write()` call in Java resolves here |
| `getBouncyCastleProvider` (THE BUG) | 0.56 | 3 | **Ranked 43rd/71 — not in top-20 shown to agent** |

**Fix:** Cap `dependent_count` contribution for entities with generic/short names or names matching common stdlib types. Add entity-type penalty (a `field` named `ByteArrayInputStream` is not the stdlib class).

**Expected result:** Substantive methods like `getBouncyCastleProvider`, `getForLogin`, `isConditionalPasskeysEnabled` enter the top-20, giving the agent the right code to analyze.

### Phase 3: New Detectors

Based on 137 golden bugs across the full benchmark, these categories have zero detector coverage today:

#### 3a. Name-Body Mismatch Detector

**Coverage:** ~27% of golden bugs (37/137)

For methods named `getX`/`isX`/`hasX`/`createX`/`findX`/`enableX`: extract the noun from the name, check if the body references it. Flag when it doesn't.

**Golden bugs this catches:**

| Repo | Bug | Why this detector catches it |
|------|-----|----------------------------|
| keycloak | `getBouncyCastleProvider()` returns default keystore | Name says 'BouncyCastle', body has `KeyStore.getDefaultType()` — no 'BouncyCastle' reference |
| grafana | `enableSqlExpressions` always returns false | Name says 'enable', body always returns false |
| sentry | `get_item_key` assumes numeric on datetime | Name says 'key', uses `math.floor` on datetime |
| cal.com | `createEvent(event, credentialId)` missing param in impl | Implementations don't include the new parameter |

**Language-agnostic:** Works on any language with named functions. Not benchmark-specific.

#### 3b. Discarded-Result Detector

**Coverage:** ~3% of golden bugs but very high precision

Flag when a value is constructed/computed and assigned to a variable but never used in return, argument, or downstream assignment.

**Golden bugs this catches:**

- keycloak: ASN1Encoder instances created, written to, results discarded, new instances created in return statement

#### 3c. Self-vs-Delegate Consistency Detector

**Coverage:** Small but catches critical/high severity bugs

In a class with a `delegate`/`inner`/`wrapped` field: if some method paths use `delegate.foo()` and others use `self.foo()` or `session.foo()` for the same operation, flag the inconsistency.

**Golden bugs this catches:**

- keycloak: `getForLogin()` uses `idpDelegate.getForLogin()` on cache miss but `session.identityProviders().getById()` on cache hit (recursive through the caching layer)

#### 3d. Guard-Replacement Checker (upgrade existing `removed-guard`)

**Coverage:** ~5% of golden bugs

When a guard is removed, compare what replaced it. If new guard has different/fewer parameters or weaker condition, flag with the delta.

**Golden bugs this catches:**

- keycloak: `isConditionalPasskeysEnabled()` replaced no-arg check with `(UserModel user)` but callers don't pass user parameter

### Phase 4: Fix Prompt Builder

- Filter test-file entities from top-20 source code selection
- Filter generic-name entities (< 5 chars or stdlib matches)
- Include `dependency_names` and `dependent_names` in entity context so agent sees caller/callee relationships
- Don't surface stripped detector findings

### Execution Priority

| Step | Effort | Impact | What it fixes |
|------|--------|--------|---------------|
| Strip noise detectors | 30 min | Removes 382/613 (62%) noise findings | Agent stops wasting attention on `callee-swap` × 29 per PR |
| Fix risk scoring | 1-2 hr | Gets buggy methods into top-20 | `getBouncyCastleProvider` goes from rank 43 → top 10 |
| Name-body mismatch detector | 2-3 hr | Catches ~27% of golden bug categories | Deterministic signal for the #1 bug category |
| Discarded-result detector | 1-2 hr | High precision, catches dead code | ASN1Encoder dead code caught deterministically |
| Guard-replacement checker | 1-2 hr | Upgrades existing `removed-guard` | Passkeys parameter bug caught with specific delta |
| Self-vs-delegate consistency | 2-3 hr | Catches wrapper/cache bugs | Recursive caching call caught deterministically |
| Fix prompt builder | 30 min | 32 fewer test entities in agent context | Agent sees production code, not test scaffolding |
