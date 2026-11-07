# WebAuthn in the glasses browser runtime, and Facebook's passkey status

Research note. 2026-11-06. Answers the question: *is WebAuthn still disabled in
the glasses' web runtime, and has Facebook enabled it?*

**Short answer: WebAuthn is not disabled in the Quest browser, and Facebook has
enabled passkeys on the web.** The remaining unknowns are narrow and are listed
at the end. Nothing here is inferred from marketing copy; every claim below is
tied to a source or to a versioned data file.

## 1. The glasses browser runtime: WebAuthn is available

### The engine

Meta's Quest Browser is Blink/Chromium, and it has been continuously since
Quest Browser 5.0 (2018) on Blink 66. The current release, Quest Browser 42.0
(2026-01-26, status `current`), is on **Blink 142**.

- `browsers/oculus.json` in MDN's browser-compat-data:
  `"engine": "Blink"`, `"engine_version": "142"`, `"upstream": "chrome_android"`.
  Every release from 5.0 to 42.0 lists a Blink engine version.
  <https://raw.githubusercontent.com/mdn/browser-compat-data/main/browsers/oculus.json>

Chrome shipped `PublicKeyCredential` in **version 67**. Quest Browser has been
above that engine version since its first release, so the API has been present
in the Quest browser throughout.

### The API surface

MDN's browser-compat-data lists **`"oculus": "mirror"`** on every WebAuthn API
— `PublicKeyCredential`, `getClientCapabilities()`, `isConditionalMediationAvailable()`,
`isUserVerifyingPlatformAuthenticatorAvailable()`, `toJSON()`, `parseCreationOptionsFromJSON()`,
`parseRequestOptionsFromJSON()`, `signalAllAcceptedCredentials()`, and the
`signal*` family. `mirror` means the Quest browser tracks the upstream browser
it derives from.

- `api/PublicKeyCredential.json`:
  <https://raw.githubusercontent.com/mdn/browser-compat-data/main/api/PublicKeyCredential.json>

Mapping the Chrome version thresholds onto Quest Browser 42.0 (Blink 142):

| Capability | Chrome | Quest Browser 42.0 |
|---|---|---|
| `PublicKeyCredential` | 67 | available |
| `isUserVerifyingPlatformAuthenticatorAvailable()` | 67 | available |
| `isConditionalMediationAvailable()` | 108 | available |
| Related Origin Requests | 128 | available (engine 142) |
| `toJSON()`, `parse*OptionsFromJSON()` | 129 | available |
| `signal*()` family | 132 | available |
| `getClientCapabilities()` | 133 | available |

**Related Origin Requests matters here.** Facebook's passkey RP ID is
`accounts.meta.com` while the management page is served from
`accountscenter.facebook.com` — a cross-origin relationship. Related Origin
Requests (Chrome 128+) is how a browser validates that. Quest Browser 42.0's
Blink 142 is comfortably past 128.

### Meta's own position

Meta's developer docs for Quest Browser do **not** list WebAuthn as missing.
They cover WebXR and browsing modes, and repeatedly instruct developers to
*detect features at runtime* rather than trust the user-agent, saying "Browser
uses the Chromium rendering engine, but both products change over time."

- <https://developers.meta.com/horizon/llmstxt/documentation/web/browser-specs.md>
  (`last_updated: 2026-07-21`)

### Meta shipped a Quest-specific WebAuthn flow

On 2026-02-04 Meta's engineering blog published *"No Display? No Problem:
Cross-Device Passkey Authentication for XR Devices (Meta Quest)"*, describing
a WebAuthn passkey flow adapted for XR devices, using the hybrid (CTAP)
transport with the Meta Horizon companion app standing in for the QR code.
The post states the implementation "has been developed and is now broadly
available on Meta Quest devices powered by Meta Horizon OS."

- <https://engineering.fb.com/2026/02/04/security/cross-device-passkey-authentication-for-xr-devices-meta-quest/>

**This is the strongest single piece of evidence.** Meta did not just inherit
WebAuthn from Chromium; they built and shipped a passkey ceremony for Quest
specifically, because a headset's display is not usable for the normal
cross-device QR flow.

### Caveat on the runtime that is *not* Quest Browser

The breakage reported for passkeys in social apps is in **in-app WebViews**,
which are a different runtime from the Quest browser. `WKWebView` (iOS) and
`Android System WebView` (Android) frequently expose no WebAuthn bridge at
all, and conditional mediation is unsupported in WebView. Facebook's own
in-app browser is one of the commonly broken cases.

- Android WebView is marked `"None"` for WebAuthn in MDN's compat tables
  (e.g. `getClientCapabilities`, `clientDataJSON`, `attestationObject`), and
  `isConditionalMediationAvailable()` in WebView 131+ resolves `false` with the
  note "Conditional mediation (autofill integration) is not supported."
- <https://openpwa.net/reference/capabilities/webauthn-passkeys/>
- <https://www.corbado.com/blog/passkeys-in-app-browsers>

So "Facebook's passkey button does nothing in the Facebook app" and "passkeys
work in the Quest browser" are both true and not in conflict. They are
different runtimes. The Quest browser is a real browser; the Facebook app's
in-app browser is a WebView.

## 2. Facebook: passkeys are enabled on the web

Facebook's passkey support is live, and it is not mobile-only any more.

- Meta rolled out **passkey login for Facebook on iOS and Android mobile**
  globally. <https://fidoalliance.org/mobileidworld-meta-rolls-out-passkey-authentication-for-facebook-mobile-users-globally/>
- Facebook's own help pages list passkeys for **Facebook and Messenger**, with
  passkeys first rolled out on the mobile app.
- The passkey management page lives at
  `https://accountscenter.facebook.com/passkey/management`, and the relying
  party is **`accounts.meta.com`**. Users on desktop browsers have reported
  successfully creating Facebook passkeys in Firefox and Chromium.

Evidence that desktop web registration works, and the shape of it, comes from
the KeePassXC-Browser issue tracker, where users repeatedly registered Facebook
passkeys:

- <https://github.com/keepassxreboot/keepassxc-browser/issues/2808>

That issue is also a useful map of the real-world failure modes:

- **Chrome uses Related Origins** to bridge `accountscenter.facebook.com` →
  `accounts.meta.com`. **Firefox uses a Permissions Policy iframe** instead:
  `<iframe allow="publickey-credentials-create *" src="https://accounts.meta.com/passkey/iframe/register/?…">`.
  The two browsers pick different cross-origin mechanisms.
- `https://accounts.meta.com/.well-known/webauthn` returned **131 origins**,
  which broke clients that capped the array length (the spec limits the number
  of *registrable origin labels*, not the number of URL entries).
- Facebook's passkey UI is reported as flaky across browsers — the "Create
  passkey" button is sometimes disabled.

An important detail for us: the **RP ID is `accounts.meta.com`**, not
`facebook.com`. A browser that only implements same-origin RP validation will
fail against Facebook even with WebAuthn fully enabled.

## 3. What this means for Hermes

- The glasses browser can run a WebAuthn ceremony. The DOM/browser layer needs
  to expose `navigator.credentials` with `create()` and `get()`, and bind the
  ceremony to the document's real origin and RP ID.
- Facebook's registration and authentication both go through
  **`accounts.meta.com`**. The Hermes implementation must support **Related
  Origin Requests** — fetch `https://<rp-id>/.well-known/webauthn` and accept
  the RP ID when the document's origin is in the returned list — or Facebook
  simply will not work. This is the single highest-value missing feature.
- The RP-ID-vs-origin rule must permit the related-origin case. The current
  implementation validates the RP ID against the origin's host or a parent of
  it, which would **reject** `accounts.meta.com` from
  `accountscenter.facebook.com`.
- Facebook's passkey flow needs a **cross-origin iframe** with
  `allow="publickey-credentials-create *"` in Firefox's variant. Our runtime
  would need to support the Permissions-Policy delegation path too, or at
  minimum not break when the page uses one.
- **Conditional mediation** (`mediation: "conditional"`, passkey autofill) is
  a separate feature from plain WebAuthn and is not supported in Android
  WebView. It is available in Quest Browser 42.0, but the Hermes runtime would
  need to implement the mediation plumbing for it to mean anything.
- Facebook requires HTTPS. The fixture and the e2e harness serve plain HTTP on
  `127.0.0.1`, which is a secure context by exception in browsers, but any
  real Facebook test must be over HTTPS.

## 4. What is still unknown, and how to settle it

These are the only open questions. Each is answerable by a direct measurement
on a device, not by more searching.

1. **Does the Quest browser expose a platform authenticator?** The API is
   present, but `isUserVerifyingPlatformAuthenticatorAvailable()` may return
   `false` on a headset — there is no biometric sensor to enrol. Meta's own
   blog addresses the *hybrid* path (the phone authenticates), which suggests
   the headset itself is not the platform authenticator. **Measure it.**
2. **Does `getClientCapabilities()` on Quest report `relatedOrigins: true`?**
   The engine version says it should. **Measure it.**
3. **Does the Quest browser's `/.well-known/webauthn` fetch work for
   `accounts.meta.com` from `accountscenter.facebook.com`?** Related Origins
   requires a network fetch by the browser. **Measure it against Facebook.**
4. **Is conditional mediation actually functional on Quest?** The API is
   there; the autofill integration may not be. **Measure it.**

The measurement is a single HTML page served over HTTPS that reports
capabilities and attempts a Facebook-shaped ceremony. Meta's own browser docs
say to detect at runtime; that advice applies to us too.
