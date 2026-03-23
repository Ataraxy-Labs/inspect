# V7 Manual Review — 2026-03-24 Run (20260324_000242)

## Pipeline Stats
- All 47 PRs completed (including keycloak PR41249 which usually times out)
- No `<final_findings>` structured block — v7 uses free-form review + extraction from full text
- Dedup by canonical_key, no validation filter, cap at 8
- 14 PRs capped at 8 (vs 2 in v6)
- Total extracted candidates: ~290 (across 47 PRs)
- Total final candidates after dedup+cap: ~255

---

## cal_dot_com (10 PRs, 31 golden)

### PR#8087 (2 golden, 5 candidates)
- G1 (try-catch around await): Not in candidates → **FN**
- G2 (forEach async fire-and-forget): C1-C4 identify forEach+async in vital, wipemycal, bookings.tsx, handleCancelBooking → **TP**
- C5 (getCalendarCredentials .calendar is Promise): Not a golden → **FP**
- **TP=1, FP=1, FN=1**

### PR#10600 (4 golden, 8 candidates)
- G1 (TwoFactor naming in BackupCode.tsx): C5 identifies "default export function named TwoFactor instead of BackupCode" → **TP**
- G2 (error message says 'login' in disable): C7 identifies "says 'backup code login' but this is the disable endpoint" → **TP**
- G3 (case-sensitive backup code): C1 identifies case-sensitivity in disable.ts and next-auth-options.ts → **TP**
- G4 (concurrent backup code race): Not explicitly in candidates → **FN**
- C2 (useState type never[]): Not a golden → **FP**
- C3 (no rate limiting on disable): Not a golden → **FP**
- C4 (no timing-safe comparison): Not a golden → **FP**
- C6 (blob URL leak): Not a golden → **FP**
- C8 (test Promise assertion always true): Not a golden → **FP**
- **TP=3, FP=5, FN=1**

### PR#10967 (5 golden, 8 candidates)
- G1 (null reference on mainHostDestinationCalendar): C1 identifies TypeError crash → **TP**
- G2 (redundant optional chaining): Not in candidates → **FN**
- G3 (externalCalendarId logic error): C2 identifies "externalCalendarId is falsy, .find() never matches" → **TP**
- G4 (logic inversion IS_TEAM_BILLING_ENABLED): C3 identifies slug logic inverted → **TP**
- G5 (Calendar interface contract break): C7 identifies "only GoogleCalendarService was updated to accept credentialId" → **TP**
- C4 (credential undefined in updateAllCalendarEvents): Not a golden → **FP**
- C5 (removed organization.slug from Prisma select): Not a golden → **FP**
- C6 (destinationCalendar null push no-op): Not a golden → **FP**
- C8 (booking only connects first destination calendar): Not a golden → **FP**
- **TP=4, FP=4, FN=1**

### PR#22345 (2 golden, 3 candidates)
- G1 (unreachable branches): C2 identifies "else if (filterConditions) branch and else / NOTHING_CONDITION branch are unreachable" → **TP**
- G2 (org-level member exclusion): Not in candidates → **FN**
- C1 (InsightsBookingServicePublicOptions type duplication): Not a golden → **FP**
- C3 (test assertion ordering fragility): Not a golden → **FP**
- **TP=1, FP=2, FN=1**

### PR#7232 (2 golden, 8 candidates)
- G1 (forEach async without await): C5-C8 identify forEach+async in handleCancelBooking, handleNewBooking, bookings.tsx, workflows.tsx → **TP**
- G2 (orphaned WorkflowReminder on immediateDelete): C1 identifies "immediateDelete path cancels SendGrid batch but never deletes WorkflowReminder DB row" → **TP**
- C2 (cron deleteMany before cancellation): Not a golden → **FP**
- C3 (single try/catch for loop): Not a golden → **FP**
- C4 (null referenceId crash): Not a golden → **FP**
- **TP=2, FP=3, FN=0**

### PR#8330 (2 golden, 4 candidates)
- G1 (incorrect end time slotStartTime vs slotEndTime): C2 identifies "end computed from slotStartTime instead of slotEndTime" → **TP**
- G2 (=== comparison of dayjs objects): C1 identifies "reference comparison always evaluates to false" → **TP**
- C3 (dateOverride skips busy check): Not a golden → **FP**
- C4 (workingHours.find implicit undefined): Not a golden → **FP**
- **TP=2, FP=2, FN=0**

### PR#11059 (5 golden, 8 candidates)
- G1 (hardcoded 'refresh_token' string): C5 identifies "sets literal string 'refresh_token' as placeholder" → **TP**
- G2 (invalid Zod schema computed keys): C3 identifies "z.string().toString() evaluates to 'ZodString'" → **TP**
- G3 (parseRefreshTokenResponse returns wrapper): C1+C2 identify "returns SafeParseReturnType wrapper instead of parsed data" → **TP**
- G4 (refreshOAuthTokens returns fetch Response): C4 identifies "returns native fetch Response, callers expect different types" → **TP**
- G5 (res.data undefined on fetch Response): Covered by C4 → **TP**
- C6 (Salesforce stale access_token): Not a golden → **FP**
- C7 (Zoho Bigin wrong userId): Not a golden → **FP**
- C8 (webhook no HTTP method check): Not a golden → **FP**
- **TP=5, FP=3, FN=0**

### PR#14943 (2 golden, 2 candidates)
- G1 (non-atomic retryCount increment): C2 mentions "retryCount: reminder.retryCount + 1" in catch block without try-catch wrapping — tangentially related but doesn't identify the race condition specifically → **FN**
- G2 (deletion targets non-SMS reminders): C1 identifies "lacks method: WorkflowMethods.SMS filter" → **TP**
- **TP=1, FP=0, FN=1**

### PR#14740 (5 golden, 8 candidates)
- G1 (case sensitivity email blacklist): C3 identifies "Blacklist email comparison is case-sensitive" → **TP**
- G2 (AND vs OR permissions): C1 identifies "uses && instead of ||" → **TP**
- G3 (notifications sent to existing attendees): C2 identifies "passes raw guests instead of filtered uniqueGuests" → **TP**
- G4 (no input dedup): C4 mentions blacklist bypass via sub-addressing — different from dedup issue → **FN**
- G5 (empty string in array): Not directly in candidates → **FN**
- C4 (blacklist bypass via + sub-addressing): Not a golden → **FP**
- C5 (no booking status check): Not a golden → **FP**
- C6 (sendAddGuestsEmails ignores disable prefs): Not a golden → **FP**
- C7 (case-insensitive in sendAddGuestsEmails): Duplicate of G1 mechanism → **FP**
- C8 (schema lacks min(1)): Not a golden → **FP**
- **TP=3, FP=5, FN=2**

### PR#22532 (2 golden, 8 candidates)
- G1 (@updatedAt not triggered by empty data): C5+C6 identify "passing empty object {} to updateMany" and "@updatedAt fragile pattern" → **TP**
- G2 (macOS-specific sed syntax): C8 identifies "macOS-specific sed syntax" → **TP**
- C1 (bypasses feature flag): Not a golden → **FP**
- C2 (plain Error instead of TRPCError): Not a golden → **FP**
- C3 (deleteCache bypasses feature flag): Not a golden → **FP**
- C4 (deleteCacheMutation no query invalidation): Not a golden → **FP**
- C7 (hardcoded en-US locale): Not a golden → **FP**
- **TP=2, FP=5, FN=0**

### cal_dot_com TOTAL: TP=24, FP=30, FN=7

---

## discourse (10 PRs, 28 golden)

### PRffbaf8c5 (3 golden, 6 candidates)
- G1 (downsize defined twice): C1 identifies "second 4-param definition silently replaces first 5-param definition" → **TP**
- G2 (hardcoded maxSizeKB 10MB): C2 identifies "hardcoded 10MB client-side limit applies to ALL upload types" → **TP**
- G3 (percentage dimensions fail for animated GIFs): C4 identifies "passing '80%' as dimensions to gifsicle --resize-fit which expects WxH" → **TP**
- C3 (downsize return unchecked in loop): Not a golden → **FP**
- C5 (API download limit hardcoded 10MB): Duplicate of G2 mechanism → **FP**
- C6 (413 handler also hardcoded): Duplicate of G2 → **FP**
- **TP=3, FP=3, FN=0**

### PR6669a2d9 (2 golden, 7 candidates)
- G1 (nil pointer if TopicUser missing): C1 identifies "NoMethodError on nil tu" → **TP**
- G2 (typo stopNotificiationsText): C6 identifies "stopNotificiationsText has a double 'i'" → **TP**
- C2 (GET state mutation, email prefetch): Not a golden → **FP**
- C3 (unsubscribe_link requires unsubscribe_url): Not a golden → **FP**
- C4 (XSS via unescaped fancyTitle): Not a golden → **FP**
- C5 (redundant API call): Not a golden → **FP**
- C7 (test doesn't assert unsubscribe URL in body): Not a golden → **FP**
- **TP=2, FP=5, FN=0**

### PR5f8a1302 (2 golden, 8 candidates)
- G1 (side effects in should_block_email?): C4 identifies "should_block? mutates the database by incrementing match_count" → **TP**
- G2 (regex matches domain suffixes): C7 identifies "regex lacks end-of-string anchor, matches domain suffixes" → **TP**
- C1 (case-sensitive email lookup): Not a golden → **FP**
- C2 (client-side case-sensitive): Not a golden → **FP**
- C3 (race on match_count increment): Not a golden → **FP**
- C5 (record.save silently swallows failures): Not a golden → **FP**
- C6 (regex metacharacters not sanitized): Not a golden → **FP**
- C8 (client shows wrong error message): Not a golden → **FP**
- **TP=2, FP=6, FN=0**

### PR4f8aed29 (6 golden, 8 candidates)
- G1 (SSRF via open(url)): C2 identifies "SSRF and command injection via Kernel#open(url)" → **TP**
- G2 (origin validation bypass indexOf): C5 identifies "weak postMessage origin validation using indexOf" → **TP**
- G3 (postMessage targetOrigin): Not explicitly discussed as targetOrigin issue → **FN**
- G4 (X-Frame-Options ALLOWALL): Not in candidates → **FN**
- G5 (TopicEmbed.import NoMethodError / XSS): C8 identifies "HTML injection via unescaped URL" → **TP**
- G6 (invalid ERB 'end if'): C4 identifies "ERB syntax error" → **TP**
- C1 (XSS via request.referer in JS): Not a golden → **FP**
- C3 (SSRF via embed_url in background job): Duplicate of G1 mechanism → **FP**
- C6 (RSS poll_feed nil content): Not a golden → **FP**
- C7 (unhandled exceptions in poll_feed): Not a golden → **FP**
- **TP=4, FP=4, FN=2**

### PR5b229316 (2 golden, 3 candidates)
- G1 (mixing float/flexbox header breakage): Not in candidates → **FN**
- G2 (-ms-align-items non-existent): C3 identifies "-ms-align-items is not a real CSS property. Dead code." → **TP**
- C1 (box-ordinal-group off-by-one): Not a golden → **FP**
- C2 (-ms-flex-align expects start/end not flex-start/flex-end): Not a golden → **FP**
- **TP=1, FP=2, FN=1**

### PR267d8be1 (1 golden, 0 candidates)
- G1 (include_website_name missing '?' suffix / string mutation): Not in candidates → **FN**
- **TP=0, FP=0, FN=1**

### PRd38c4d5f (3 golden, 6 candidates)
- G1 (lightness inversion 30% to 70%): C2 identifies "desktop/topic-post.scss light-theme value changed from 30% to 70%" → **TP**
- G2 (inconsistent $primary change 30% to 50%): C4 identifies "desktop/user.scss .name light-theme value changed from 30% to 50%" → **TP**
- G3 (text too dark 70% to 30%): C1 identifies "mobile/modal.scss light-theme value changed from 70% to 30%" → **TP**
- C3 (mobile h3 20% to 50%): Additional lightness issue → **FP**
- C5 (mobile/user.scss same as C4): Duplicate → **FP**
- C6 (badge-notification not converted): Not a golden → **FP**
- **TP=3, FP=3, FN=0**

### PR060cda77 (3 golden, 7 candidates)
- G1 (async findMembers() race): Not in candidates → **FN**
- G2 (offset capping produces empty page): C2 identifies "Math.floor + 1 produces one extra page" → **TP**
- G3 (HTTP method mismatch PUT vs DELETE): C7 identifies "test uses :put but route defined as delete" → **TP**
- C1 (group.users.delete passes integer ID): Not a golden → **FP**
- C3 (next action offset = user_count → empty page): Related to G2, elaboration → not separate FP
- C4 (limit parameter no upper bound): Not a golden → **FP**
- C5 (visible reset on partial update): Not a golden → **FP**
- C6 (silently skipped non-existent usernames): Not a golden → **FP**
- **TP=2, FP=4, FN=1**

### PRecfa17b5 (2 golden, 7 candidates)
- G1 (thread-safety in @loaded_locales): C5 identifies "@loaded_locales ||= [] not thread-safe, can race with reload!" → **TP**
- G2 (double-loading via symbol/string mismatch): Not in candidates → **FN**
- C1 (SiteSetting.default_locale.to_sym NPE on nil): Not a golden → **FP**
- C2 (background jobs don't call ensure_loaded!): Not a golden → **FP**
- C3 (I18n.with_locale doesn't trigger ensure_loaded!): Not a golden → **FP**
- C4 (LRU cache key missing default_locale): Not a golden → **FP**
- C6 (SiteSetting.default_locale during early boot): Not a golden → **FP**
- C7 (fallbacks active in dev/test): Not a golden → **FP**
- **TP=1, FP=6, FN=1**

### PRd1c69189 (4 golden, 8 candidates)
- G1 (NoMethodError in before_validation): C4 identifies "self.host.sub! called on nil" → **TP**
- G2 (missing existence check in update/destroy): C5 identifies "nil.destroy raises NoMethodError" → **TP**
- G3 (case normalization missing in record_for_host): Not in candidates (C1 discusses record_for_host returning false instead of nil, different issue) → **FN**
- G4 (migration raw SQL lacks normalization): C2 identifies "SQL injection in migration: host value interpolated directly" → **TP** (captures the raw SQL problem, though focuses on injection vs normalization — the underlying issue of raw SQL without sanitization is the same)
- C1 (record_for_host returns false instead of nil): Not a golden → **FP**
- C3 (migration crashes on fresh installs): Not a golden → **FP**
- C6 (String.replace only first underscore): Not a golden → **FP**
- C7 (_hydrateEmbedded deletes obj[k] unconditionally): Not a golden → **FP**
- C8 (_hydrateEmbedded doesn't filter undefined): Not a golden → **FP**
- **TP=3, FP=5, FN=1**

### discourse TOTAL: TP=21, FP=38, FN=7

---

## grafana (9 PRs, 20 golden)

### PR79265 (5 golden, 8 candidates)
- G1 (race condition device count): C6 identifies "TOCTOU race condition...concurrent requests can all pass count check" → **TP**
- G2 (anonymous auth fails on limit): Not explicitly discussed as auth failure → **FN**
- G3 (dbSession.Exec compilation error): Not in candidates → **FN**
- G4 (misleading ErrDeviceLimitReached): C7 mentions "ErrDeviceLimitReached returned raw...generic 500 or obscure error" → **TP** (identifies the misleading error propagation)
- G5 (UTC time window inconsistency): Not in candidates → **FN**
- C1 (nil connector stored in socialMap): Not a golden → **FP**
- C2 (resp.Body.Close before nil check): Not a golden → **FP**
- C3 (appendUniqueScope ignores scope param): Not a golden → **FP**
- C4 (Wire injection broken): Not a golden → **FP**
- C5 (SocialMap dead code): Not a golden → **FP**
- C8 (commont_test.go filename typo): Not a golden → **FP**
- **TP=2, FP=6, FN=3**

### PR103633 (2 golden, 4 candidates)
- G1 (asymmetric cache trust): C3 identifies "Stale denial cache blocks newly granted permissions...denial cache checked before permission cache" → **TP**
- G2 (test comment/map mismatch): Not directly identified — C4 discusses metrics inaccuracy, not the test comment mismatch → **FN**
- C1 (denial cache key collision with permission cache): Not a golden → **FP**
- C2 (denial cache key collision between tuples): Not a golden → **FP**
- C4 (metrics inaccuracy): Not a golden → **FP**
- **TP=1, FP=3, FN=1**

### PR76186 (2 golden, 3 candidates)
- G1 (panic on nil request): C1 identifies "access req.PluginContext without nil-checking req, a nil request will cause a panic" → **TP**
- G2 (traceID missing from logs): Not in candidates → **FN**
- C2 (TestLogger.FromContext returns disconnected instance): Not a golden → **FP**
- C3 (test function naming inconsistency): Not a golden → **FP**
- **TP=1, FP=2, FN=1**

### PR107534 (1 golden, 6 candidates)
- G1 (request.filters unused in test setup): C1 identifies "applyTemplateVariables called twice...double interpolation" and C3 identifies "test mocks runQuery so super.query() never called, hiding the double interpolation bug" → **TP** (identifies the consequence of the unused filters — double application)
- C2 (triple interpolation in shard-split): Elaboration of C1 → not separate FP
- C4 (test mock unconditionally replaces $__auto): Not a golden → **FP**
- C5 (shard test same $__auto issue): Not a golden → **FP**
- C6 (mock mutates input query object): Not a golden → **FP**
- **TP=1, FP=3, FN=0**

### PR106778 (2 golden, 8 candidates)
- G1 (missing key prop): C3 identifies "Missing key prop on GrafanaRuleListItem in FilterView.tsx" → **TP**
- G2 (silence notifications no effect for Prom rules): C4 discusses "recording rules can no longer be modify-exported through useAllGrafanaPromRuleAbilities" — related to abilities/silencing regression → **TP** (captures the functional breakage in alerting rule abilities)
- C1 (LegacyTokenRest.New returns wrong type): Not a golden → **FP**
- C2 (toSAItem missing InternalID): Not a golden → **FP**
- C5 (TeamBindingSpec.TeamRef renamed): Not a golden → **FP**
- C6 (removed serviceaccounts:read): Not a golden → **FP**
- C7 (filename typo types_servier_account): Not a golden → **FP**
- C8 (leftover debug comment): Not a golden → **FP**
- **TP=2, FP=6, FN=0**

### PR90045 (3 golden, 8 candidates)
- G1 (context created with d.Log instead of log): C8 identifies "klog.NewContext uses d.Log instead of enriched log variable" → **TP**
- G2 (recordLegacyDuration used for storage failure): C1+C2 identify "recordLegacyDuration called instead of recordStorageDuration" → **TP**
- G3 (name used instead of options.Kind): C4 identifies "object name passed as 'kind' label, creating high-cardinality metrics" → **TP**
- C3 (DeleteCollection swapped recording): Not a golden (inverse swap) → **FP**
- C5 (obj captured by reference in goroutine): Not a golden → **FP**
- C6 (TestMode3_Delete no legacy mock): Not a golden → **FP**
- C7 (TestMode3_DeleteCollection no legacy mock): Not a golden → **FP**
- **TP=3, FP=4, FN=0**

### PR80329 (1 golden, 7 candidates)
- G1 (error log level for debugging): C2 identifies "r.log.Error() used for routine debug/operational logging" → **TP**
- C1 (ticker 10min→1min overlapping cleanup): Not a golden → **FP**
- C3 (logging full ID slices): Not a golden → **FP**
- C4 (logging nil err): Not a golden → **FP**
- C5 (SQLite parameter limit check wrong variable): Not a golden → **FP**
- C6 (require.NoError in callback panics goroutine): Not a golden → **FP**
- C7 (misleading test case name): Not a golden → **FP**
- **TP=1, FP=6, FN=0**

### PR94942 (2 golden, 4 candidates)
- G1 (enableSqlExpressions always returns false): C1 identifies "both branches return false" → **TP**
- G2 (stub methods return 'not implemented'): Not directly captured — C2 discusses unguarded entry point, C4 discusses test checking wrong error message. Neither identifies the core issue that stub methods are incomplete/broken. → **FN**
- C2 (UnmarshalSQLCommand unguarded entry point): Not a golden → **FP**
- C3 (DB.TablesList dead code): Not a golden → **FP**
- C4 (test checks for wrong error string): Not a golden → **FP**
- **TP=1, FP=3, FN=1**

### PR97529 (2 golden, 2 candidates)
- G1 (race condition in BuildIndex): C2 identifies "TOCTOU race: concurrent callers for same key both see nil and both build" → **TP**
- G2 (TotalDocs() race condition): C1 identifies "data race on totalBatchesIndexed variable" — different race than TotalDocs, but captures a concurrency issue → Actually this is about totalBatchesIndexed not TotalDocs. TotalDocs() race is about iterating b.cache without sync → **FN**
- C1 (totalBatchesIndexed race): Not exactly the golden (different race condition) → **FP**
- **TP=1, FP=1, FN=1**

### grafana TOTAL: TP=13, FP=34, FN=7

---

## keycloak (10 PRs, 24 golden)

### PR41249 (2 golden, 2 candidates)
- G1 (ConditionalPasskeysEnabled() called without UserModel parameter): Not in candidates — C1 discusses missing passkeys macro in base theme, C2 discusses test missing configureTestRealm → **FN**
- G2 (authenticate() skips fillContextForm for null users): Not in candidates → **FN**
- C1 (base theme login-password.ftl missing passkeys macro): Not a golden → **FP**
- C2 (test missing configureTestRealm): Not a golden → **FP**
- **TP=0, FP=2, FN=2**

### PR32918 (2 golden, 1 candidate)
- G1 (recursive caching call session vs delegate): Not in candidates → **FN**
- G2 (incorrect alias in cleanup): C1 identifies "cleanup registers removal for hardcoded 'alias' instead of actual IDP aliases" → **TP**
- **TP=1, FP=0, FN=1**

### PR33832 (2 golden, 7 candidates)
- G1 (wrong provider returned): Not in candidates (C1 is about breaking API, C2 about removed safety guard) → **FN**
- G2 (dead code in ASN1Encoder): C4 identifies "two ASN1Encoder.create().write() calls...dead code" → **TP**
- C1 (CryptoProvider.order() breaks external impls): Not a golden → **FP**
- C2 (removed safety guard for multiple providers): Not a golden → **FP**
- C3 (logger.debugf treats % as format specifier): Not a golden → **FP**
- C5 (indefinite-length encoding returns -1): Not a golden → **FP**
- C6 (readSequence doesn't verify CONSTRUCTED bit): Not a golden → **FP**
- C7 (test uses single P-256 key for all algorithms): Not a golden → **FP**
- **TP=1, FP=6, FN=1**

### PR36882 (1 golden, 7 candidates)
- G1 (picocli.exit() calls System.exit()): Not in candidates → **FN**
- C1 (NPE in valueOrSecret): Not a golden → **FP**
- C2 (valueOrSecret only compares secret name not key): Not a golden → **FP**
- C3 (consent replay after denial): Not a golden → **FP**
- C4 (getConfigProperties returns null): Not a golden → **FP**
- C5 (createInitialDeployment inconsistency): Not a golden → **FP**
- C6 (testFeatureNotEnabled incomplete): Not a golden → **FP**
- C7 (unused import OrganizationModel): Not a golden → **FP**
- **TP=0, FP=7, FN=1**

### PR36880 (3 golden, 5 candidates)
- G1 (orphaned permissions V1 vs V2 flag): Not in candidates → **FN**
- G2 (hasPermission findByName owner mismatch): Not in candidates → **FN**
- G3 (getClientsWithPermission returns name not IDs): Not in candidates → **FN**
- C1 (NPE when getResourceTypeResource returns null): Not a golden → **FP**
- C2 (getResourceName doesn't handle CLIENTS): Not a golden → **FP**
- C3 (operator README wrong property name): Not a golden → **FP**
- C4 (dead code getEvaluationContext): Not a golden → **FP**
- C5 (Javadoc error in requireView): Not a golden → **FP**
- **TP=0, FP=5, FN=3** (same regression as v6!)

### PR37038 (2 golden, 4 candidates)
- G1 (incorrect permission check in canManage()): C1 identifies "VIEW scope in canManage, privilege escalation" → **TP**
- G2 (getGroupIdsWithViewPermission ID/name mismatch): C2 identifies "groupResource.getId() instead of getName()" → **TP**
- C3 (findByType won't discover individual group resources): Elaboration of G2 → not separate FP
- C4 (getResourceName doesn't handle GROUPS): Not a golden → **FP**
- **TP=2, FP=1, FN=0**

### PR37429 (4 golden, 8 candidates)
- G1 (Italian translation in Lithuanian file): C1 identifies "Lithuanian files contain Italian text" → **TP**
- G2 (Traditional Chinese in Simplified Chinese file): C2 identifies "zh_CN uses Traditional Chinese characters" → **TP**
- G3 (anchor sanitization logic error): C3+C6 identify StringIndexOutOfBoundsException and stale matcher data → **TP**
- G4 (typo santizeAnchors): C8 identifies "santizeAnchors should be sanitizeAnchors" → **TP**
- C4 (verifySafeHtml throws RuntimeException): Not a golden → **FP**
- C5 (santizeAnchors returns partially-stripped value): Not a golden → **FP**
- C7 (regex character class ambiguous dash): Not a golden → **FP**
- **TP=4, FP=3, FN=0**

### PR37634 (4 golden, 8 candidates)
- G1 (wrong parameter in null check grantType vs rawTokenId): C1 identifies "null check for rawTokenId incorrectly checks grantType again" → **TP**
- G2 (isAccessTokenId logic inverted): C3 identifies "equality check is inverted" → **TP**
- G3 (Javadoc/implementation shortcut length mismatch): C2 identifies "substring(3,5) extracts characters at indices 3-4 instead of substring(4,6)" → **TP**
- G4 (catching broad RuntimeException): Not in candidates → **FN**
- C4 (ClientCredentialsGrantType never sets GRANT_TYPE): Not a golden → **FP**
- C5 (AuthorizationTokenService same issue): Not a golden → **FP**
- C6 (OAuth2GrantTypeFactory breaks third-party impls): Not a golden → **FP**
- C7 (TokenManager.initToken NPE on null provider): Not a golden → **FP**
- C8 (test assertEquals reversed order): Not a golden → **FP**
- **TP=3, FP=5, FN=1**

### PR38446 (2 golden, 8 candidates)
- G1 (unsafe raw List deserialization / Optional.get()): C4+C5 identify "Optional.get() without isPresent() check, throws NoSuchElementException" → **TP**
- G2 (credential ID not set after creation): Not directly in candidates — C1 discusses plaintext codes in federated storage, C2 discusses removeStoredCredentialById on wrong store → partially related to G2's concern about ID → **FN**
- C1 (raw unhashed codes sent to federated storage): Not a golden → **FP**
- C2 (removeStoredCredentialById on local instead of federated): Not a golden → **FP**
- C3 (NPE when RecoveryAuthnCodes feature disabled): Not a golden → **FP**
- C6 (test completeLogin never called): Not a golden → **FP**
- C7 (any stored code accepted, not sequential): Not a golden → **FP**
- C8 (typo in test config alias): Not a golden → **FP**
- **TP=1, FP=6, FN=1**

### PR40940 (2 golden, 8 candidates)
- G1 (getSubGroupsCount() returns null): C4 identifies "adjacent methods getSubGroupsStream all still call modelSupplier.get() without null checks" — related to the null model issue, though discusses sibling methods rather than getSubGroupsCount itself → **TP** (captures the core null model problem)
- G2 (reader thread race condition in test): Not in candidates → **FN**
- C1 (DefaultTokenManager null guard removal): Not a golden → **FP**
- C2 (wrong header X-Forwarded vs X-Forwarded-Proto): Not a golden → **FP**
- C3 (kc.sh quoting regression): Not a golden → **FP**
- C5 (PreAuthorizedCode.interval primitive vs NON_NULL): Not a golden → **FP**
- C6 (displayText→displayTest reversion): Not a golden → **FP**
- C7 (Facebook token as query param): Not a golden → **FP**
- C8 (doc theme SPI double dash): Not a golden → **FP**
- **TP=1, FP=7, FN=1**

### keycloak TOTAL: TP=13, FP=42, FN=11

---

## sentry (8 PRs, 25 golden)

### PR92393 (3 golden — UNFAIR: CursorPaginator bugs not in diff)
- G1 (OptimizedCursorPaginator negative slicing): NOT in diff → **FN (unfair)**
- G2 (BasePaginator negative slicing): NOT in diff → **FN (unfair)**
- G3 (get_item_key TypeError): NOT in diff → **FN (unfair)**
- C1 (__reduce__ parameter order mismatch): Not a golden → **FP**
- C2 (max_segment_spans dead code): Not a golden → **FP**
- C3 (Lua hardcoded 1000 vs Python 1001): Not a golden → **FP**
- C4 (ZUNIONSTORE default SUM): Not a golden → **FP**
- **TP=0, FP=4, FN=3 (all 3 FN are unfair)**

### PR67876 (3 golden, 5 candidates)
- G1 (null reference github_authenticated_user): Not explicitly in candidates — C1 discusses unhandled HTTPError, different issue → **FN**
- G2 (static OAuth state): C4 identifies "pipeline.signature is static deterministic value, not per-session random nonce" → **TP**
- G3 (KeyError on integration.metadata): C2 identifies "KeyError on legacy integrations lacking 'sender' in metadata" → **TP**
- C1 (unhandled HTTPError in get_user_info): Not a golden → **FP**
- C3 (test uses wrong state hash): Not a golden → **FP**
- C5 (redirect_uri not URL-encoded): Not a golden → **FP**
- **TP=2, FP=3, FN=1**

### PR5 (3 golden, 7 candidates)
- G1 (breaking changes in error response): Not clearly in candidates → **FN**
- G2 (detector validator wrong key): Not in candidates → **FN**
- G3 (zip() assumes dict order): C1 identifies "zip(error_ids, events.values()) order mismatch" → **TP**
- C2 (validate_timestamp falsy check for age=0): Not a golden → **FP**
- C3 (TableWidgetVisualization empty data): Not a golden → **FP**
- C4 (get_environment_info N+1 queries): Not a golden → **FP**
- C5 (logging bug fix event_data): Not a golden → **FP**
- C6 (age lacks min_value=0): Not a golden → **FP**
- C7 (analytics event name mismatch): Not a golden → **FP**
- **TP=1, FP=6, FN=2**

### PR93824 (5 golden, 6 candidates)
- G1 (inconsistent metric tagging shard/shards): C3 identifies "one uses 'shard', another 'shards'" → **TP**
- G2 (flaky fixed sleep): Not explicitly discussed as the golden intends → **FN**
- G3 (isinstance SpawnProcess always false): C1+C2 identify "isinstance(process, multiprocessing.Process) is False for SpawnProcess instances" → **TP**
- G4 (monkeypatched sleep won't wait): C5 identifies "time.sleep(0.1) is a no-op because time.sleep is monkeypatched" → **TP**
- G5 (deadline skips process termination): Not in candidates → **FN**
- C4 (_create_process_for_shard dead code): Not a golden → **FP**
- C6 (memory_info N× Redis overhead): Not a golden → **FP**
- **TP=3, FP=2, FN=2**

### PR77754 (4 golden, 7 candidates)
- G1 (shared mutable default timestamp): C1 identifies "timezone.now() evaluated once at module import time" → **TP**
- G2 (typo 'inalid'): C6 identifies "test_from_dict_inalid_data should be test_from_dict_invalid_data" → **TP**
- G3 (empty_array name vs empty dict test): C7 identifies "tests empty dict {}, not empty array" → **TP**
- G4 (JSON serialization failure datetime): C2 identifies "raw datetime object into dict passed as Celery task kwargs" → **TP**
- C3 (ExampleIntegration missing **kwargs): Not a golden → **FP**
- C4 (source_name and queued dead data): Not a golden → **FP**
- C5 (test_to_dict doesn't catch timestamp bug): Not a golden → **FP**
- **TP=4, FP=3, FN=0**

### PR80528 (2 golden, 1 candidate)
- G1 (returns original config instead of modified): C1 identifies "returns original unmodified config instead of the local copy" → **TP**
- G2 (unnecessary DB query for MonitorCheckIn): Not in candidates → **FN**
- **TP=1, FP=0, FN=1**

### PR80168 (2 golden, 4 candidates)
- G1 (MetricAlertDetectorHandler missing abstract methods): C1 identifies "does not implement 4 required abstract methods, TypeError on instantiation" → **TP**
- G2 (docstring return type mismatch): Not in candidates → **FN**
- C2 (test_dedupe builds expected with wrong group_key): Not a golden → **FP**
- C3 (build_mock_occurrence_and_event value parameter unused): Not a golden → **FP**
- C4 (all test comparisons use hardcoded id): Not a golden → **FP**
- **TP=1, FP=3, FN=1**

### PR95633 (3 golden, 8 candidates)
- G1 (queue.shutdown() AttributeError): C1 identifies "shutdown() sets worker.shutdown=True before q.shutdown(), causing workers to exit without processing remaining items" — captures a related but different shutdown bug → **TP** (identifies concrete shutdown ordering bug)
- G2 (magic number 50 max_wait): Not in candidates → **FN**
- G3 (docstring mismatch / test behavioral issue): C4 identifies "test asserts offsets NOT committed, but complete_offset is called in finally...assertion only passes due to timing" → **TP**
- C2 (silent data loss on failed processing): Not a golden → **FP**
- C3 (join calls close, double shutdown): Not a golden → **FP**
- C5 (dead code second except queue.ShutDown): Not a golden → **FP**
- C6 (TOCTOU race in _get_partition_lock): Not a golden → **FP**
- C7 (send_result hardcodes partition/offset): Not a golden → **FP**
- C8 (if/if/if/else chain instead of elif): Not a golden → **FP**
- **TP=2, FP=6, FN=1**

### sentry TOTAL: TP=14, FP=27, FN=11 (3 unfair FNs in PR92393)

---

## GRAND TOTAL

| Repo | PRs | TP | FP | FN | P | R | F1 |
|------|-----|----|----|----|----|----|----|
| cal_dot_com | 10 | 24 | 30 | 7 | 44.4% | 77.4% | 56.5% |
| discourse | 10 | 21 | 38 | 7 | 35.6% | 75.0% | 48.3% |
| grafana | 9 | 13 | 34 | 7 | 27.7% | 65.0% | 38.8% |
| keycloak | 10 | 13 | 42 | 11 | 23.6% | 54.2% | 32.9% |
| sentry | 8 | 14 | 27 | 11 | 34.1% | 56.0% | 42.4% |
| **TOTAL** | **47** | **85** | **171** | **43** | **33.2%** | **66.4%** | **44.3%** |

### Excluding unfair sentry PR#92393 (3 FN for code not in diff):
| **TOTAL (fair)** | **47** | **85** | **171** | **40** | **33.2%** | **68.0%** | **44.6%** |

---

## Comparison vs Baselines

| Approach | TP | FP | FN | P | R | F1 | Δ F1 vs v3 |
|----------|----|----|----|----|----|----|------|
| Old slice pipeline | 50 | 132 | 73 | 27.5% | 40.7% | 32.8% | -21.1 |
| v1 raw review | 80 | 225 | 48 | 26.2% | 62.5% | 37.0% | -16.9 |
| v2 entity+extract | 77 | 155 | 51 | 33.2% | 60.2% | 42.8% | -11.1 |
| **v3 (best)** | **87** | **108** | **41** | **44.6%** | **68.0%** | **53.9%** | **—** |
| v4 full | 71 | 123 | 57 | 36.6% | 55.5% | 44.1% | -9.8 |
| v5 (inclusive) | 81 | 236 | 47 | 25.6% | 63.3% | 36.4% | -17.5 |
| v6 (strict filter) | 54 | 47 | 74 | 53.5% | 42.2% | 47.2% | -6.7 |
| **v7 (this run)** | **85** | **171** | **43** | **33.2%** | **66.4%** | **44.3%** | **-9.6** |

---

## Analysis

### Recall nearly restored: 42.2% → 66.4% (+24.2pp)
Removing the validation filter and raising the cap from 5→8 brought TPs from 54 back to 85 (v3 had 87). Recall is nearly identical to v3 (66.4% vs 68.0%). This confirms the v6 validation filter was the primary recall killer.

### Precision collapsed: 53.5% → 33.2% (−20.3pp)
FPs exploded from 47 to 171 (+264%). The extraction step with "when in doubt, INCLUDE" combined with cap-8 and no validation filter produces massive FP counts. This is worse than v2 (33.2%) and approaching v1 territory.

### F1 regression: 53.9% → 44.3% (−9.6pp)
Despite near-v3 recall, the precision drop pulls F1 below v3, v4, and v6. The tradeoff didn't work — removing the filter entirely was too aggressive.

### Root causes of FP explosion:

1. **Inclusive extraction too aggressive**: "When in doubt, INCLUDE" causes the extraction LLM to capture every concern mentioned in the review, including architectural opinions, style nits disguised as bugs, and theoretical edge cases.

2. **Cap-8 isn't enough with 9-14 extracted**: Many PRs extract 7-14 candidates but only have 1-5 golden bugs. Even with cap-8, the 3-7 non-golden candidates pass through.

3. **No validation filter at all**: V6's filter was too strict, but removing it entirely swung the pendulum too far. Some filtering is clearly needed.

4. **Agent review quality is verbose**: The free-form v3-style review produces many observations. Without structured output to constrain the agent, it reports everything it notices. The extraction then faithfully captures all of it.

### Per-repo analysis:

- **cal_dot_com**: TP=24 (v3=28, v6=16) — recall improved but still 4 TPs short of v3. FP=30 same as v3.
- **discourse**: TP=21 same as v3! FP=38 (v3=30) — 8 more FPs.
- **grafana**: TP=13 (v3=8!) — big improvement, +5 TPs! FP=34 (v3=15) — 19 more FPs though.
- **keycloak**: TP=13 (v3=16, v6=10) — improved from v6 but still 3 short of v3. FP=42 (v3=18) — terrible, 24 more FPs.
- **sentry**: TP=14 same as v3! FP=27 (v3=15) — 12 more FPs.

### Key improvements vs v6:
- grafana PR90045: v6=TP2/FP1, v7=TP3/FP4 — gained all 3 golden bugs!
- grafana PR106778: v6=TP0/FP2, v7=TP2/FP6 — gained both goldens
- grafana PR76186: v6=TP0/FP1, v7=TP1/FP2 — gained nil request panic
- sentry PR93824: v6=TP2/FP0, v7=TP3/FP2 — gained isinstance bug
- sentry PR77754: v6=TP1/FP1, v7=TP4/FP3 — all 4 goldens found!
- cal_dot_com PR10600: v6=TP1/FP2, v7=TP3/FP5 — recovered 2 TPs
- cal_dot_com PR22532: v6=TP0/FP1, v7=TP2/FP5 — recovered both TPs

### Persistent regressions vs v3:
- keycloak PR36880: v3=TP3/FP0, v7=TP0/FP5 — still lost all 3 TPs! The agent doesn't find the V1/V2 flag, owner mismatch, or returns-name-not-IDs bugs.
- keycloak PR41249: v3=FN2 (timeout), v7=TP0/FP2 — completed but found wrong bugs.
- cal_dot_com PR14740 G4/G5: v3 found both, v7 missed both.

---

## Recommendations for v8

1. **Bring back validation filter but make it lenient**: The sweet spot is between v6 (too strict) and v7 (no filter). A validation prompt like "drop only if the issue is clearly a style preference, not a functional concern" would help. Target: reduce FPs by 40% while losing <5 TPs.

2. **Two-tier extraction**: First extract all correctness issues (current approach), then re-rank by confidence. Only report top-N by confidence score, not just severity.

3. **Lower cap from 8 to 6**: Analysis shows most PRs have 2-5 golden bugs. Cap-8 lets too many FPs through on PRs with only 1-2 goldens. Cap-6 would still accommodate 95% of golden-rich PRs.

4. **Severity-weighted filtering**: Critical/high issues pass automatically. Medium issues get the validation check. This preserves recall on high-confidence bugs while filtering medium-confidence noise.

5. **Fix keycloak PR36880 specifically**: This PR has been a consistent 0-TP across v6 and v7. The agent isn't finding permission-related bugs in Keycloak's authorization code. Consider whether the entity triage or diff ordering is pushing these entities to the back.
