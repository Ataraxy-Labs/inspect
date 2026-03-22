Now I have all the evidence needed. Let me reason through the bugs precisely.

**Bug 1 — Partial form state leading to template crash:**
- `WebAuthnConditionalUIAuthenticator.fillContextForm` sets `ENABLE_WEBAUTHN_CONDITIONAL_UI = TRUE` *before* calling `super.fillContextForm`
- `WebAuthnAuthenticator.fillContextForm` (the super) returns `null` early at line 104–105 when `authenticators.getAuthenticators().isEmpty()` — this path skips setting `IS_USER_IDENTIFIED` (`"isUserIdentified"`) and `USER_VERIFICATION` (`"userVerification"`)
- `passkeys.ftl` accesses `${isUserIdentified}` and `${userVerification}` as bare references (no `!` default or `??` null-safe check) inside the `<#if enableWebAuthnConditionalUI?has_content>` block — FreeMarker throws `InvalidReferenceException`
- Both callers (`authenticate()` and `challenge(context, error, field)`) discard the return value of `fillContextForm`, so neither detects the null and both proceed to render the form

**Bug 2 — Null-user passkeys check is unconditionally true:**
- `isConditionalPasskeysEnabled(null)` short-circuits to `true` whenever `webauthnAuth != null && webauthnAuth.isPasskeysEnabled()` — the per-user credential check is fully bypassed
- `challenge(MultivaluedMap)` (line 127–133) goes directly to `forms.createLoginUsernamePassword()` without calling `challenge(String, String)`, so the `authenticate()` path does not double-invoke `fillContextForm`; however the null-gate is still semantically wrong: it makes the method return true for ANY anonymous user on the login page whether or not any passkeys exist

```json
{
  "issues": [
    {
      "issue": "WebAuthnConditionalUIAuthenticator.fillContextForm sets ENABLE_WEBAUTHN_CONDITIONAL_UI=TRUE before calling super.fillContextForm, which returns null when the identified user has no WebAuthn credentials (authenticators.isEmpty()). Because IS_USER_IDENTIFIED ('isUserIdentified') and USER_VERIFICATION ('userVerification') are set only after the isEmpty() guard in the parent, they are never written to the form when the parent returns null. The passkeys.ftl template then unconditionally dereferences ${isUserIdentified} and ${userVerification} inside the <#if enableWebAuthnConditionalUI?has_content> block with no FreeMarker null-safe operator, causing an InvalidReferenceException at render time.",
      "evidence": "// WebAuthnConditionalUIAuthenticator.fillContextForm:\ncontext.form().setAttribute(WebAuthnConstants.ENABLE_WEBAUTHN_CONDITIONAL_UI, Boolean.TRUE); // set unconditionally\nreturn super.fillContextForm(context); // may return null\n\n// WebAuthnAuthenticator.fillContextForm (super):\nif (authenticators.getAuthenticators().isEmpty()) {\n    return null; // exits WITHOUT setting IS_USER_IDENTIFIED or USER_VERIFICATION\n}\n...\nform.setAttribute(WebAuthnConstants.IS_USER_IDENTIFIED, Boolean.toString(isUserIdentified)); // never reached\nform.setAttribute(WebAuthnConstants.USER_VERIFICATION, userVerificationRequirement);        // never reached\n\n// passkeys.ftl:\n<#if enableWebAuthnConditionalUI?has_content>  <!-- TRUE, so block is entered -->\n  isUserIdentified : ${isUserIdentified},   <!-- bare ref, not set → InvalidReferenceException -->\n  userVerification : '${userVerification}', <!-- bare ref, not set → InvalidReferenceException -->",
      "severity": "critical",
      "file": "services/src/main/java/org/keycloak/authentication/authenticators/browser/WebAuthnConditionalUIAuthenticator.java"
    },
    {
      "issue": "isConditionalPasskeysEnabled(null) unconditionally returns true (short-circuits the per-user credential check) whenever passkeys are enabled, causing fillContextForm to be invoked on every anonymous initial login page load even when no user has any passkeys registered. The method name and signature imply it gates on a specific user's credentials, but the null branch bypasses that gate entirely.",
      "evidence": "protected boolean isConditionalPasskeysEnabled(UserModel currentUser) {\n    return webauthnAuth != null && webauthnAuth.isPasskeysEnabled() &&\n            (currentUser == null   // ← short-circuit: skips isConfiguredFor check entirely\n             || currentUser.credentialManager().isConfiguredFor(webauthnAuth.getCredentialType()));\n}\n// In authenticate(), context.getUser() is null on initial page load:\nif (isConditionalPasskeysEnabled(context.getUser())) {  // → always true when passkeys enabled\n    webauthnAuth.fillContextForm(context);\n}",
      "severity": "medium",
      "file": "services/src/main/java/org/keycloak/authentication/authenticators/browser/UsernamePasswordForm.java"
    },
    {
      "issue": "Both authenticate() and challenge(context, error, field) ignore the return value of webauthnAuth.fillContextForm(context). When fillContextForm returns null (super detected no credentials), the callers have no way to know setup failed and both proceed unconditionally to render the form that now has ENABLE_WEBAUTHN_CONDITIONAL_UI=TRUE but missing isUserIdentified/userVerification attributes, compounding the template crash bug.",
      "evidence": "// In authenticate():\nif (isConditionalPasskeysEnabled(context.getUser())) {\n    webauthnAuth.fillContextForm(context);  // return value discarded\n}\nResponse challengeResponse = challenge(context, formData); // proceeds regardless\n\n// In challenge(context, error, field):\nif (isConditionalPasskeysEnabled(context.getUser())) {\n    webauthnAuth.fillContextForm(context);  // return value discarded\n}\nreturn super.challenge(context, error, field); // proceeds regardless, renders template",
      "severity": "high",
      "file": "services/src/main/java/org/keycloak/authentication/authenticators/browser/UsernamePasswordForm.java"
    }
  ]
}
```