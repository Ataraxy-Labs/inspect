# System Prompt

You are an expert senior engineer performing a thorough code review.

Review the provided code entities and their diffs. For each entity:
1. Understand what changed and why.
2. Read any other relevant files (callers, interfaces, tests, related modules) using tools to fully understand the context.
3. Call out concrete correctness bugs: logic errors, race conditions, null safety, API misuse, argument mismatches, missing awaits, broken control flow, type errors, security issues.

Use grep and read freely to explore callers, callees, interfaces, and related code. Follow dependency chains — if a function signature changed, check all callers. If a method overrides an interface, read the interface contract.

Do NOT report: style, naming, missing tests, documentation, suggestions, or issues in deleted-only code. Only report bugs you are confident are real.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no bugs.

---

# User Prompt

# PR: Code review
Top 30 entities across 10 files (from 36 total)

## Detector Findings (10)
- **[HIGH] signature-change-with-callers** `passwordLoginWithNonDiscoverableKey` testsuite/integration-arquillian/tests/base/src/test/java/org/keycloak/testsuite/webauthn/passwordless/PasskeysUsernameFormTest.java:164
  Public function `passwordLoginWithNonDiscoverableKey` changed signature but has 1 caller(s) that may need updating
  `Callers: PasskeysUsernameFormTest (testsuite/integration-arquillian/tests/base/src/test/java/org/keycloak/testsuite/webauthn/passwordless/PasskeysUsernameFormTest.java)` | risk=High(0.64) deps=1
- **[HIGH] signature-change-with-callers** `passwordLoginWithNonDiscoverableKey` testsuite/integration-arquillian/tests/base/src/test/java/org/keycloak/testsuite/webauthn/passwordless/PasskeysOrganizationAuthenticationTest.java:168
  Public function `passwordLoginWithNonDiscoverableKey` changed signature but has 1 caller(s) that may need updating
  `Callers: PasskeysOrganizationAuthenticationTest (testsuite/integration-arquillian/tests/base/src/test/java/org/keycloak/testsuite/webauthn/passwordless/PasskeysOrganizationAuthenticationTest.java)` | risk=Critical(0.97) deps=1
- **[HIGH] synchronized-missing** `AuthenticatorUtils` services/src/main/java/org/keycloak/authentication/authenticators/util/AuthenticatorUtils.java:44
  Shared mutable static field without synchronization — potential race condition
  `public static Map<String, Integer> parseCompletedExecutions(String note){` | risk=Critical(1.00) deps=17
- **[MEDIUM] type-change-propagation** `AbstractUsernameFormAuthenticator` services/src/main/java/org/keycloak/authentication/authenticators/browser/AbstractUsernameFormAuthenticator.java:49
  Type `AbstractUsernameFormAuthenticator` was modified but 33 dependent(s) were not updated in this diff: AuthenticationProcessor, attachSession, Logger, getUserFromForm, OTPFormAuthenticator and 28 more
  `Unchanged dependents: AuthenticationProcessor, attachSession, Logger, getUserFromForm, OTPFormAuthenticator and 28 more` | risk=Low(0.15) deps=34
- **[MEDIUM] type-change-propagation** `PasswordForm` services/src/main/java/org/keycloak/authentication/authenticators/browser/PasswordForm.java:33
  Type `PasswordForm` was modified but 12 dependent(s) were not updated in this diff: AuthenticationSelectionResolver, PasswordFormFactory, create, SelectAuthenticatorPage, BrokerRunOnServerUtil and 7 more
  `Unchanged dependents: AuthenticationSelectionResolver, PasswordFormFactory, create, SelectAuthenticatorPage, BrokerRunOnServerUtil and 7 more` | risk=Critical(1.00) deps=12
- **[MEDIUM] type-change-propagation** `AuthenticatorUtils` services/src/main/java/org/keycloak/authentication/authenticators/util/AuthenticatorUtils.java:40
  Type `AuthenticatorUtils` was modified but 13 dependent(s) were not updated in this diff: DefaultAuthenticationFlow, processResult, testInvalidUser, CookieAuthenticator, authenticate and 8 more
  `Unchanged dependents: DefaultAuthenticationFlow, processResult, testInvalidUser, CookieAuthenticator, authenticate and 8 more` | risk=Critical(1.00) deps=17
- **[HIGH] removed-guard** `badPasswordHandler` services/src/main/java/org/keycloak/authentication/authenticators/browser/AbstractUsernameFormAuthenticator.java:1
  Guard/assertion removed: `if (isUserAlreadySetBeforeUsernamePasswordAuth(context)) {` — safety check may be lost
  `if (isUserAlreadySetBeforeUsernamePasswordAuth(context)) {` | risk=Critical(1.56) deps=2
- **[HIGH] removed-guard** `challenge` services/src/main/java/org/keycloak/authentication/authenticators/browser/UsernamePasswordForm.java:1
  Guard/assertion removed: `if (context.getUser() == null && webauthnAuth != null && webauthnAuth.isPasskeys` — safety check may be lost
  `if (context.getUser() == null && webauthnAuth != null && webauthnAuth.isPasskeysEnabled()) {` | risk=Critical(1.39) deps=2
- **[HIGH] removed-guard** `UsernamePasswordForm` services/src/main/java/org/keycloak/authentication/authenticators/browser/UsernamePasswordForm.java:1
  Guard/assertion removed: `if (webauthnAuth != null && webauthnAuth.isPasskeysEnabled()) {` — safety check may be lost
  `if (webauthnAuth != null && webauthnAuth.isPasskeysEnabled()) {` | risk=Low(0.21) deps=0
- **[HIGH] removed-guard** `authenticate` services/src/main/java/org/keycloak/authentication/authenticators/browser/UsernamePasswordForm.java:1
  Guard/assertion removed: `if (webauthnAuth != null && webauthnAuth.isPasskeysEnabled()) {` — safety check may be lost
  `if (webauthnAuth != null && webauthnAuth.isPasskeysEnabled()) {` | risk=Low(0.12) deps=1

## services/src/main/java/org/keycloak/authentication/authenticators/browser/AbstractUsernameFormAuthenticator.java (Critical, 2 entities)
### `badPasswordHandler` (method, modified) :218-235
risk=Critical(1.56) blast=10000 deps=2 | callers: AbstractUsernameFormAuthenticator, validatePassword
⚠️ **Review cue:** Method name contains "handler" but implementation body does not reference it. Verify the implementation matches the contract.
```
private boolean badPasswordHandler(AuthenticationFlowContext context, UserModel user, boolean clearUser,boolean isEmptyPassword) {
        context.getEvent().user(user);
        context.getEvent().error(Errors.INVALID_USER_CREDENTIALS);

        AuthenticatorUtils.setupReauthenticationInUsernamePasswordFormError(context);

        Response challengeResponse = challenge(context, getDefaultChallengeMessage(context), FIELD_PASSWORD);
        if(isEmptyPassword) {
            context.forceChallenge(challengeResponse);
        }else{
            context.failureChallenge(AuthenticationFlowError.INVALID_CREDENTIALS, challengeResponse);
        }

        if (clearUser) {
            context.clearUser();
        }
        return false;
    }
```

### `AbstractUsernameFormAuthenticator` (class, modified) :49-261
risk=Low(0.15) blast=10000 deps=34 **PUBLIC** | callers: AuthenticationProcessor, attachSession, Logger, getUserFromForm +30
```
public abstract class AbstractUsernameFormAuthenticator extends AbstractFormAuthenticator {

    private static final Logger logger = Logger.getLogger(AbstractUsernameFormAuthenticator.class);

    public static final String REGISTRATION_FORM_ACTION = "registration_form";
    public static final String ATTEMPTED_USERNAME = "ATTEMPTED_USERNAME";
    public static final String SESSION_INVALID = "SESSION_INVALID";

    // Flag is true if user was already set in the authContext before this authenticator was triggered. In this case we skip clearing of the user after unsuccessful password authentica
... (9164 more chars)
```

## services/src/main/java/org/keycloak/authentication/authenticators/browser/UsernamePasswordForm.java (Critical, 4 entities)
### `challenge` (method, modified) :135-142
risk=Critical(1.39) blast=2 deps=2 | callers: UsernamePasswordForm, authenticate
```
@Override
    protected Response challenge(AuthenticationFlowContext context, String error, String field) {
        if (isConditionalPasskeysEnabled(context.getUser())) {
            // setup webauthn data when possible
            webauthnAuth.fillContextForm(context);
        }
        return super.challenge(context, error, field);
    }
```

### `UsernamePasswordForm` (class, modified) :41-165
risk=Low(0.21) blast=0 deps=0 **PUBLIC**
```
public class UsernamePasswordForm extends AbstractUsernameFormAuthenticator implements Authenticator {

    protected final WebAuthnConditionalUIAuthenticator webauthnAuth;

    public UsernamePasswordForm() {
        webauthnAuth = null;
    }

    public UsernamePasswordForm(KeycloakSession session) {
        webauthnAuth = new WebAuthnConditionalUIAuthenticator(session, (context) -> createLoginForm(context.form()));
    }

    @Override
    public void action(AuthenticationFlowContext context) {
        MultivaluedMap<String, String> formData = context.getHttpRequest().getDecodedFormParamet
... (4599 more chars)
```

### `authenticate` (method, modified) :85-120
risk=Low(0.12) blast=1 deps=1 **PUBLIC** | callers: UsernamePasswordForm
```
@Override
    public void authenticate(AuthenticationFlowContext context) {
        MultivaluedMap<String, String> formData = new MultivaluedHashMap<>();
        String loginHint = context.getAuthenticationSession().getClientNote(OIDCLoginProtocol.LOGIN_HINT_PARAM);

        String rememberMeUsername = AuthenticationManager.getRememberMeUsername(context.getSession());

        if (context.getUser() != null) {
            if (alreadyAuthenticatedUsingPasswordlessCredential(context)) {
                // if already authenticated using passwordless webauthn just success
                context.su
... (1179 more chars)
```

### `isConditionalPasskeysEnabled` (method, added) :160-163
risk=Low(0.11) blast=10000 deps=6 | callers: PasswordForm, authenticate, configuredFor, UsernamePasswordForm +2
```
protected boolean isConditionalPasskeysEnabled(UserModel currentUser) {
        return webauthnAuth != null && webauthnAuth.isPasskeysEnabled() &&
                (currentUser == null || currentUser.credentialManager().isConfiguredFor(webauthnAuth.getCredentialType()));
    }
```

## services/src/main/java/org/keycloak/authentication/authenticators/browser/PasswordForm.java (Critical, 3 entities)
### `PasswordForm` (class, modified) :33-86
risk=Critical(1.00) blast=10000 deps=12 **PUBLIC** | callers: AuthenticationSelectionResolver, PasswordFormFactory, create, SelectAuthenticatorPage +8
```
public class PasswordForm extends UsernamePasswordForm implements CredentialValidator<PasswordCredentialProvider> {

    public PasswordForm(KeycloakSession session) {
        super(session);
    }

    @Override
    protected boolean validateForm(AuthenticationFlowContext context, MultivaluedMap<String, String> formData) {
        return validatePassword(context, context.getUser(), formData, false);
    }

    @Override
    public void authenticate(AuthenticationFlowContext context) {
        if (alreadyAuthenticatedUsingPasswordlessCredential(context)) {
            context.success();
      
... (1292 more chars)
```

### `authenticate` (method, modified) :44-58
risk=Low(0.12) blast=10000 deps=1 **PUBLIC** | callers: PasswordForm
```
@Override
    public void authenticate(AuthenticationFlowContext context) {
        if (alreadyAuthenticatedUsingPasswordlessCredential(context)) {
            context.success();
            return;
        }

        // setup webauthn data when passkeys enabled
        if (isConditionalPasskeysEnabled(context.getUser())) {
            webauthnAuth.fillContextForm(context);
        }

        Response challengeResponse = context.form().createLoginPassword();
        context.challenge(challengeResponse);
    }
```

### `configuredFor` (method, modified) :60-65
risk=Low(0.11) blast=10000 deps=1 **PUBLIC** | callers: PasswordForm
```
@Override
    public boolean configuredFor(KeycloakSession session, RealmModel realm, UserModel user) {
        return user.credentialManager().isConfiguredFor(getCredentialProvider(session).getType())
                || (isConditionalPasskeysEnabled(user))
                || alreadyAuthenticatedUsingPasswordlessCredential(session.getContext().getAuthenticationSession());
    }
```

## services/src/main/java/org/keycloak/authentication/authenticators/util/AuthenticatorUtils.java (Critical, 2 entities)
### `AuthenticatorUtils` (class, modified) :40-135
risk=Critical(1.00) blast=10000 deps=17 **PUBLIC** | callers: DefaultAuthenticationFlow, processResult, AbstractUsernameFormAuthenticator, testInvalidUser +13
```
public final class AuthenticatorUtils {
    private static final Logger logger = Logger.getLogger(AuthenticatorUtils.class);

    public static String getDisabledByBruteForceEventError(BruteForceProtector protector, KeycloakSession session, RealmModel realm, UserModel user) {
        if (realm.isBruteForceProtected()) {
            if (protector.isPermanentlyLockedOut(session, realm, user)) {
                return Errors.USER_DISABLED;
            }
            else if (protector.isTemporarilyDisabled(session, realm, user)) {
                return Errors.USER_TEMPORARILY_DISABLED;
          
... (4254 more chars)
```

### `setupReauthenticationInUsernamePasswordFormError` (method, added) :125-133
risk=Low(0.13) blast=10000 deps=5 **PUBLIC** | callers: AbstractUsernameFormAuthenticator, badPasswordHandler, WebAuthnConditionalUIAuthenticator, createErrorResponse +1
⚠️ **Review cue:** Method name contains "setup, reauthentication, error" but implementation body does not reference them. Verify the implementation matches the contract.
```
public static void setupReauthenticationInUsernamePasswordFormError(AuthenticationFlowContext context) {
        String userAlreadySetBeforeUsernamePasswordAuth = context.getAuthenticationSession().getAuthNote(USER_SET_BEFORE_USERNAME_PASSWORD_AUTH);

        if (Boolean.parseBoolean(userAlreadySetBeforeUsernamePasswordAuth)) {
            LoginFormsProvider form = context.form();
            form.setAttribute(LoginFormsProvider.USERNAME_HIDDEN, true);
            form.setAttribute(LoginFormsProvider.REGISTRATION_DISABLED, true);
        }
    }
```

## testsuite/integration-arquillian/tests/base/src/test/java/org/keycloak/testsuite/webauthn/passwordless/PasskeysOrganizationAuthenticationTest.java (Critical, 3 entities)
### `passwordLoginWithNonDiscoverableKey` (method, modified) :168-220
risk=Critical(0.97) blast=1 deps=1 **PUBLIC** | callers: PasskeysOrganizationAuthenticationTest
```
@Test
    public void passwordLoginWithNonDiscoverableKey() throws Exception {
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.PASSKEYS.getOptions());

        // set passwordless policy not specified, key will not be discoverable
        try (Closeable c = getWebAuthnRealmUpdater()
                .setWebAuthnPolicyRpEntityName("localhost")
                .setWebAuthnPolicyRequireResidentKey(Constants.DEFAULT_WEBAUTHN_POLICY_NOT_SPECIFIED)
                .setWebAuthnPolicyUserVerificationRequirement(Constants.DEFAULT_WEBAUTHN_POLICY_NOT_SPECIFIED)
                
... (2272 more chars)
```

### `PasskeysOrganizationAuthenticationTest` (class, modified) :58-326
risk=Low(0.16) blast=0 deps=0 **PUBLIC**
```
@EnableFeature(value = Profile.Feature.PASSKEYS, skipRestart = true)
@IgnoreBrowserDriver(FirefoxDriver.class) // See https://github.com/keycloak/keycloak/issues/10368
public class PasskeysOrganizationAuthenticationTest extends AbstractWebAuthnVirtualTest {

    @Override
    public void addTestRealms(List<RealmRepresentation> testRealms) {
        RealmRepresentation realmRepresentation = AbstractAdminTest.loadJson(getClass().getResourceAsStream("/webauthn/testrealm-webauthn.json"), RealmRepresentation.class);

        makePasswordlessRequiredActionDefault(realmRepresentation);
        switch
... (12192 more chars)
```

### `webauthnLoginWithDiscoverableKey_reauthentication` (method, added) :269-325
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysOrganizationAuthenticationTest
```
@Test
    public void webauthnLoginWithDiscoverableKey_reauthentication() throws IOException {
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.PASSKEYS.getOptions());

        // set passwordless policy for discoverable keys
        try (Closeable c = getWebAuthnRealmUpdater()
                .setWebAuthnPolicyRpEntityName("localhost")
                .setWebAuthnPolicyRequireResidentKey(Constants.WEBAUTHN_POLICY_OPTION_YES)
                .setWebAuthnPolicyUserVerificationRequirement(Constants.WEBAUTHN_POLICY_OPTION_REQUIRED)
                .setWebAuthnPolicyPassk
... (1951 more chars)
```

## testsuite/integration-arquillian/tests/base/src/test/java/org/keycloak/testsuite/webauthn/passwordless/PasskeysUsernameFormTest.java (High, 5 entities)
### `passwordLoginWithNonDiscoverableKey` (method, modified) :164-218
risk=High(0.64) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernameFormTest
```
@Test
    public void passwordLoginWithNonDiscoverableKey() throws IOException {
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.PASSKEYS.getOptions());

        // set passwordless policy not specified, key will not be discoverable
        try (Closeable c = getWebAuthnRealmUpdater()
                .setWebAuthnPolicyRpEntityName("localhost")
                .setWebAuthnPolicyRequireResidentKey(Constants.DEFAULT_WEBAUTHN_POLICY_NOT_SPECIFIED)
                .setWebAuthnPolicyUserVerificationRequirement(Constants.DEFAULT_WEBAUTHN_POLICY_NOT_SPECIFIED)
              
... (2302 more chars)
```

### `PasskeysUsernameFormTest` (class, modified) :62-397
risk=Low(0.14) blast=0 deps=0 **PUBLIC**
```
@EnableFeature(value = Profile.Feature.PASSKEYS, skipRestart = true)
@IgnoreBrowserDriver(FirefoxDriver.class) // See https://github.com/keycloak/keycloak/issues/10368
public class PasskeysUsernameFormTest extends AbstractWebAuthnVirtualTest {

    @Override
    public void addTestRealms(List<RealmRepresentation> testRealms) {
        RealmRepresentation realmRepresentation = AbstractAdminTest.loadJson(getClass().getResourceAsStream("/webauthn/testrealm-webauthn.json"), RealmRepresentation.class);

        makePasswordlessRequiredActionDefault(realmRepresentation);
        switchExecutionInBro
... (15315 more chars)
```

### `addTestRealms` (method, modified) :66-75
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernameFormTest
```
@Override
    public void addTestRealms(List<RealmRepresentation> testRealms) {
        RealmRepresentation realmRepresentation = AbstractAdminTest.loadJson(getClass().getResourceAsStream("/webauthn/testrealm-webauthn.json"), RealmRepresentation.class);

        makePasswordlessRequiredActionDefault(realmRepresentation);
        switchExecutionInBrowser(realmRepresentation);

        configureTestRealm(realmRepresentation);
        testRealms.add(realmRepresentation);
    }
```

### `passwordLogin_reauthenticationOfUserWithoutPasskey` (method, added) :336-396
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernameFormTest
```
@Test
    public void passwordLogin_reauthenticationOfUserWithoutPasskey() throws Exception {
        // use a default resident key which is not shown in conditional UI
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.DEFAULT_RESIDENT_KEY.getOptions());

        // set passwordless policy for discoverable keys
        try (Closeable c = getWebAuthnRealmUpdater()
                .setWebAuthnPolicyRpEntityName("localhost")
                .setWebAuthnPolicyRequireResidentKey(Constants.WEBAUTHN_POLICY_OPTION_YES)
                .setWebAuthnPolicyUserVerificationRequirem
... (2455 more chars)
```

### `webauthnLoginWithDiscoverableKey_reauthentication` (method, added) :267-333
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernameFormTest
```
@Test
    public void webauthnLoginWithDiscoverableKey_reauthentication() throws IOException {
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.PASSKEYS.getOptions());

        // set passwordless policy for discoverable keys
        try (Closeable c = getWebAuthnRealmUpdater()
                .setWebAuthnPolicyRpEntityName("localhost")
                .setWebAuthnPolicyRequireResidentKey(Constants.WEBAUTHN_POLICY_OPTION_YES)
                .setWebAuthnPolicyUserVerificationRequirement(Constants.WEBAUTHN_POLICY_OPTION_REQUIRED)
                .setWebAuthnPolicyPassk
... (2448 more chars)
```

## testsuite/integration-arquillian/tests/base/src/test/java/org/keycloak/testsuite/webauthn/passwordless/PasskeysUsernamePasswordFormTest.java (High, 8 entities)
### `PasskeysUsernamePasswordFormTest` (class, modified) :60-424
risk=High(0.62) blast=0 deps=0 **PUBLIC**
```
@EnableFeature(value = Profile.Feature.PASSKEYS, skipRestart = true)
@IgnoreBrowserDriver(FirefoxDriver.class) // See https://github.com/keycloak/keycloak/issues/10368
public class PasskeysUsernamePasswordFormTest extends AbstractWebAuthnVirtualTest {

    @Page
    protected SelectOrganizationPage selectOrganizationPage;

    @Override
    public void addTestRealms(List<RealmRepresentation> testRealms) {
        RealmRepresentation realmRepresentation = AbstractAdminTest.loadJson(getClass().getResourceAsStream("/webauthn/testrealm-webauthn.json"), RealmRepresentation.class);

        makePass
... (15211 more chars)
```

### `passwordLoginWithExternalKey` (method, modified) :173-211
risk=Low(0.12) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernamePasswordFormTest
```
@Test
    public void passwordLoginWithExternalKey() throws Exception {
        // use a default resident key which is not shown in conditional UI
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.DEFAULT_RESIDENT_KEY.getOptions());

        // set passwordless policy for discoverable keys
        try (Closeable c = setPasswordlessPolicyForExternalKey()) {

            checkWebAuthnConfiguration(Constants.WEBAUTHN_POLICY_OPTION_YES, Constants.WEBAUTHN_POLICY_OPTION_REQUIRED);

            registerDefaultUser();

            UserRepresentation user = userResource().toRe
... (1089 more chars)
```

### `passwordLoginWithNonDiscoverableKey` (method, modified) :122-171
risk=Low(0.12) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernamePasswordFormTest
```
@Test
    public void passwordLoginWithNonDiscoverableKey() throws IOException {
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.PASSKEYS.getOptions());

        // set passwordless policy not specified, key will not be discoverable
        try (Closeable c = getWebAuthnRealmUpdater()
                .setWebAuthnPolicyRpEntityName("localhost")
                .setWebAuthnPolicyRequireResidentKey(Constants.DEFAULT_WEBAUTHN_POLICY_NOT_SPECIFIED)
                .setWebAuthnPolicyUserVerificationRequirement(Constants.DEFAULT_WEBAUTHN_POLICY_NOT_SPECIFIED)
              
... (1700 more chars)
```

### `addTestRealms` (method, modified) :67-76
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernamePasswordFormTest
```
@Override
    public void addTestRealms(List<RealmRepresentation> testRealms) {
        RealmRepresentation realmRepresentation = AbstractAdminTest.loadJson(getClass().getResourceAsStream("/webauthn/testrealm-webauthn.json"), RealmRepresentation.class);

        makePasswordlessRequiredActionDefault(realmRepresentation);
        switchExecutionInBrowserFormToProvider(realmRepresentation, UsernamePasswordFormFactory.PROVIDER_ID);

        configureTestRealm(realmRepresentation);
        testRealms.add(realmRepresentation);
    }
```

### `reauthenticationOfUserWithoutPasskey` (method, added) :279-324
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernamePasswordFormTest
⚠️ **Review cue:** Method name contains "reauthentication, without" but implementation body does not reference them. Verify the implementation matches the contract.
```
@Test
    public void reauthenticationOfUserWithoutPasskey() throws Exception {
        // set passwordless policy for discoverable keys
        try (Closeable c = getWebAuthnRealmUpdater()
                .setWebAuthnPolicyPasskeysEnabled(Boolean.FALSE)
                .update()) {

            // Login with password
            oauth.openLoginForm();
            WaitUtils.waitForPageToLoad();

            // WebAuthn elements not available
            loginPage.assertCurrent();
            Assert.assertThrows(NoSuchElementException.class, () -> driver.findElement(By.xpath("//form[@id='webaut
... (1340 more chars)
```

### `webauthnLoginWithExternalKey_reauthentication` (method, added) :215-275
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernamePasswordFormTest
```
@Test
    public void webauthnLoginWithExternalKey_reauthentication() throws Exception {
        // use a default resident key which is not shown in conditional UI
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.DEFAULT_RESIDENT_KEY.getOptions());

        // set passwordless policy for discoverable keys
        try (Closeable c = setPasswordlessPolicyForExternalKey()) {

            checkWebAuthnConfiguration(Constants.WEBAUTHN_POLICY_OPTION_YES, Constants.WEBAUTHN_POLICY_OPTION_REQUIRED);

            registerDefaultUser();

            UserRepresentation user = us
... (2106 more chars)
```

### `webauthnLoginWithExternalKey_reauthenticationWithPasswordOrPasskey` (method, added) :328-413
risk=Low(0.11) blast=1 deps=1 **PUBLIC** | callers: PasskeysUsernamePasswordFormTest
```
@Test
    public void webauthnLoginWithExternalKey_reauthenticationWithPasswordOrPasskey() throws Exception {
        // use a default resident key which is not shown in conditional UI
        getVirtualAuthManager().useAuthenticator(DefaultVirtualAuthOptions.DEFAULT_RESIDENT_KEY.getOptions());

        // set passwordless policy for discoverable keys
        try (Closeable c = setPasswordlessPolicyForExternalKey()) {

            checkWebAuthnConfiguration(Constants.WEBAUTHN_POLICY_OPTION_YES, Constants.WEBAUTHN_POLICY_OPTION_REQUIRED);

            registerDefaultUser();

            UserRep
... (3130 more chars)
```

### `setPasswordlessPolicyForExternalKey` (method, added) :415-422
risk=Low(0.10) blast=4 deps=4 | callers: PasskeysUsernamePasswordFormTest, passwordLoginWithExternalKey, webauthnLoginWithExternalKey_reauthentication, webauthnLoginWithExternalKey_reauthenticationWithPasswordOrPasskey
⚠️ **Review cue:** Method name contains "passwordless, external" but implementation body does not reference them. Verify the implementation matches the contract.
```
private Closeable setPasswordlessPolicyForExternalKey() {
        return getWebAuthnRealmUpdater()
                .setWebAuthnPolicyRpEntityName("localhost")
                .setWebAuthnPolicyRequireResidentKey(Constants.WEBAUTHN_POLICY_OPTION_YES)
                .setWebAuthnPolicyUserVerificationRequirement(Constants.WEBAUTHN_POLICY_OPTION_REQUIRED)
                .setWebAuthnPolicyPasskeysEnabled(Boolean.TRUE)
                .update();
    }
```

## services/src/main/java/org/keycloak/authentication/authenticators/browser/WebAuthnConditionalUIAuthenticator.java (High, 1 entities)
### `WebAuthnConditionalUIAuthenticator` (class, modified) :32-63
risk=High(0.59) blast=0 deps=0 **PUBLIC**
```
public class WebAuthnConditionalUIAuthenticator extends WebAuthnPasswordlessAuthenticator {

    private final Function<AuthenticationFlowContext, Response> errorChallenge;

    public WebAuthnConditionalUIAuthenticator(KeycloakSession session, Function<AuthenticationFlowContext, Response> errorChallenge) {
        super(session);
        this.errorChallenge = errorChallenge;
    }

    @Override
    public LoginFormsProvider fillContextForm(AuthenticationFlowContext context) {
        context.form().setAttribute(WebAuthnConstants.ENABLE_WEBAUTHN_CONDITIONAL_UI, Boolean.TRUE);
        return s
... (783 more chars)
```

## themes/src/main/resources/theme/base/login/passkeys.ftl (Medium, 1 entities)
### `lines 21-36` (chunk, deleted) :0-0
risk=Medium(0.36) blast=0 deps=0
```
           };

           document.addEventListener("DOMContentLoaded", (event) => initAuthenticate({errmsg : "${msg("passkey-unsupported-browser-text")?no_esc}", ...args}));
           const authButton = document.getElementById('authenticateWebAuthnButton');
           if (authButton) {
               authButton.addEventListener("click", (event) => {
                   event.preventDefault();
                   authenticateByWebAuthn({errmsg : "${msg("webauthn-unsupported-browser-text")?no_esc}", ...args});
               });
           }
        </script>
        <a id="authenticateWebAuthnB
... (227 more chars — deleted)
```

## themes/src/main/resources/theme/keycloak.v2/login/login-password.ftl (Medium, 1 entities)
### `lines 21-21` (chunk, added) :0-0
risk=Medium(0.32) blast=0 deps=0
```
</@layout.registrationLayout>
```
