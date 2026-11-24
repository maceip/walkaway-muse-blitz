# Real-system test registry

A test belongs here only when the exact command, real components, assertion,
and last observed result are known. Passing a unit test never proves a demo
hop works across devices.

## Verified

### T-HERMES-SUITES

- Command: `cargo test --release --manifest-path apps/hermes-browser/Cargo.toml`
  (runs all 103 Rust unit tests plus `--selftest`, `--runtimetest`, `--uitest`,
  `--observetest`, `--supervisortest`, and `--hurdletest` on the Hermes release
  binary).
- Real components: Hermes browser, Blitz rendering, Static Hermes AOT page
  (`src/js/dom_runtime.ts` + `src/dom_ffi.rs` + `c_shim/*`), RFC 6265 cookie
  jar + 10-hop HTTP redirect following, `window.fetch` / `FormData` /
  `URLSearchParams` / `localStorage` / `sessionStorage` / `window.location` /
  `history.pushState` / `<script src>` / JS Console, native observe,
  supervision state machine, background session tabs, supervision bar.
- Asserts: rendering/input foundations, tab/runtime isolation, indexed
  snapshots, proposal/decision execution, takeover/release/stop, audit and
  secret exclusion, Part 4 multi-route auth/cookie/fetch/storage/location flow,
  and Part 5 modern SPA/form interactivity (`DOMContentLoaded` + `load`,
  `classList`, `removeAttribute`/`hasAttribute`, scoped `querySelectorAll`,
  `btoa`/`atob`, `crypto.getRandomValues`, `fill_form` dispatching `input` +
  `change` to enable a disabled submit button, and `submit` honoring
  `e.preventDefault()` + `new FormData(form)` + `fetch()` on a background tab
  without stealing foreground focus). The supervisor suite's passkey checks
  drive the **real broker** (`src/credentials.rs`): a registration ceremony is
  held until a human approves it, then completes as a real ECDSA P-256 WebAuthn
  registration; an assertion ceremony is held until a human approves it, then
  completes as a real ECDSA P-256 WebAuthn assertion.
- Last observed: **PASS** 2026-11-11 — `cargo test --release` 103/103 PASS in
  `9.29s` (`--selftest`, `--runtimetest`, `--uitest`, `--observetest`,
  `--supervisortest`, and `--hurdletest` all PASS).

### T-HERMES-PASSKEY-E2E

- Command: `./target/release/hermes-blitz-browser --passkeytest` from
  `apps/hermes-browser/`. The harness starts the real demo fixture
  (`services/demo-fixtures/signup_fixture.py`) as a subprocess on an ephemeral
  port and tears it down afterwards; it never skips.
- Real components: the Rust WebAuthn authenticator
  (`apps/hermes-browser/src/webauthn/`), the macOS Keychain
  (`apps/hermes-browser/src/keychain.rs`), the passkey broker
  (`apps/hermes-browser/src/credentials.rs`), and the demo fixture's
  independent server-side verifier, which checks the signature with the
  `openssl` CLI against the registered public key.
- Asserts: the relying party issues a challenge; a registration ceremony
  produces nothing until a human approves it; the completed registration is
  accepted by the relying party and the passkey is stored in the Keychain; an
  assertion ceremony produces nothing until a human approves it; the
  relying party's verifier accepts the assertion; the verifier rejects a
  replayed challenge, an unknown credential, and a mismatched challenge; the
  broker's prompt redacts the account name; the harness refuses to run if
  the macOS Keychain is unreachable, and it removes the passkey it created
  when it finishes.
- Last observed: **PASS** 2026-11-06 10:5x PT — 12/12 checks. The suite
  exercises a real `openssl dgst -sha256 -verify` in the fixture, so the
  assertion is checked by an implementation independent of the one under test.
- Note: the harness switches macOS keychain user interaction off before it
  touches the Keychain, so this suite cannot put a login-keychain password
  prompt on the user's screen. An access that the operating system would have
  turned into a prompt fails with an error instead, and the run reports that
  failure rather than skipping the check.
- Note: the fixture subprocess is owned by a `FixtureGuard` that kills it when
  it goes out of scope. `std::process::Child` does not kill its child when
  dropped, so without the guard an interrupted or failed run would leave the
  fixture holding its port and database. The guard is covered by
  `cargo test --release fixture_guard`.
- Note: on a locally built, unsigned binary the data protection keychain
  refuses the write (`errSecMissingEntitlement`), and the module falls back to
  the file-based login keychain, where macOS ignores `kSecAttrAccessible`. The
  fallback is reported on stderr and the active backend is exposed by
  `keychain::backend()`. A signed build with a keychain access group uses the
  data protection keychain, where the accessibility attribute applies.

### T-HERMES-KEYCHAIN-ROUNDTRIP

- Command: `cargo test --release keychain` from `apps/hermes-browser/`.
- Real components: the macOS login keychain through `security-framework`, and
  the live `SecKeychainGetUserInteractionAllowed` platform call.
- Asserts: a generic item round-trips; a write replaces an existing item;
  items enumerate by service with their account and label; an update replaces
  the stored data; a missing item reports "not deleted"; binary payloads
  survive intact; the item carries
  `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`; macOS keychain user
  interaction is switched off, so an access that would have asked the user
  for the login keychain password fails with an error instead; the run removes
  the Keychain items that earlier test runs left behind and reports how many
  it could not remove.
- Last observed: **PASS** 2026-11-06 10:5x PT — 11/11 checks against the real
  login keychain.
- Note: the no-prompt check is a real regression test, not a restatement of
  intent. `SecKeychain::disable_user_interaction()` returns a guard whose
  `Drop` re-enables interaction; the guard is held for the life of the
  process. The test was confirmed to fail (`left: true`, meaning a prompt was
  still possible) when the guard is dropped early, and to pass when it is
  retained.

### T-HERMES-KEYCHAIN-NO-PROMPT

- Command: `./target/release/hermes-blitz-browser --keychain-cleanup` from
  `apps/hermes-browser/`.
- Real components: the macOS login keychain through `security-framework`.
- Asserts: the process reports that keychain user interaction is disabled
  before it touches any item, and it removes the generic-password items left
  under the `ai.zmax.hermes.browser.` services, naming each item it removed
  and naming each one it could not remove.
- Last observed: **PASS** 2026-11-06 10:5x PT — reported
  `keychain user interaction: disabled (no password prompt possible)` and
  `keychain cleanup: 0 removed, 0 left in place` against a keychain with no
  leftover Hermes items (`security dump-keychain` shows no `ai.zmax.hermes.*`
  service).
- Note: macOS keychain ACLs trust the binary that created an item. An item
  left by a different build is therefore not deletable by this binary; the
  cleanup reports it as "left in place" rather than claiming a removal that
  did not happen. Removing such an item needs a one-time approval in Keychain
  Access.

### T-HERMES-PAGE-WEBAUTHN-BROKER

- Command: `./target/release/hermes-blitz-browser --runtimetest` from
  `apps/hermes-browser/` (the WebAuthn checks in that suite), plus
  `cargo test --release credentials::tests` for the broker-level tests.
- Real components: the AOT-compiled Static Hermes runtime
  (`apps/hermes-browser/src/js/dom_runtime.ts`), the C bridge
  (`c_shim/hermes_shim.cpp` + `c_shim/blitz_bridge.h`), the Rust host bindings
  (`apps/hermes-browser/src/dom_ffi.rs`), the passkey broker
  (`apps/hermes-browser/src/credentials.rs`), the WebAuthn authenticator
  (`apps/hermes-browser/src/webauthn/authenticator.rs`), and the macOS Keychain
  (`apps/hermes-browser/src/keychain.rs`). No mock and no stub stands in for
  any of them.
- Asserts: a page calling `navigator.credentials.create()` runs a real P-256
  registration ceremony against the Keychain-backed broker and gets back a
  credential with real `id`/`rawId`/`authenticatorData`/`clientDataJSON`/
  `signature` buffers; a page calling `navigator.credentials.get()` with an
  `allowCredentials` list gets an assertion for the credential it named; a page
  calling `navigator.credentials.get()` with no list discovers the relying
  party's resident credential, which is what autofill and conditional mediation
  need; and a page naming a credential outside the list has its ceremony
  **rejected**, so the authenticator cannot silently substitute a different
  credential of the same relying party.
- Last observed: **PASS** 2026-11-06 23:0x PT — `--runtimetest` reports all
  three page-path checks ok and `cargo test --release` is 74/74.
- Note: the negative check is a real regression test, not a restatement of
  intent. With the authenticator's allow-list condition reduced to the
  relying-party match alone, the check fails with "the page saw the ceremony
  resolve"; it passes only when the credential must be one the relying party
  named.
- Note: the broker-level test `an_allow_list_selects_exactly_the_named_credential`
  registers **two** credentials for one relying party, because with only one
  credential "used the named one" and "used some credential of this relying
  party" are indistinguishable and a broken implementation would pass by
  accident.
- Note: the runtime's binary buffers have sharp edges that the implementation
  has to respect. An array *literal* in this AOT Hermes is sealed — it has no
  `push` and takes no new index properties — so code that builds a list must use
  the `Array` constructor. A bare `ArrayBuffer` is not indexable, so copying
  its bytes one index at a time yields zeros; it must be wrapped in a
  `Uint8Array` view first.

### T-HERMES-WEATHER-APP

- Command: `./target/release/hermes-blitz-browser --weathertest` from
  `apps/hermes-browser/`. The harness starts the real demo fixture as a
  subprocess on an ephemeral port and tears it down afterwards.
- Real components: the demo fixture (`services/demo-fixtures/signup_fixture.py`)
  and the weather module (`services/demo-fixtures/weather_app.py`), the real
  key-less **Open-Meteo** upstream services (`geocoding-api.open-meteo.com`
  and `api.open-meteo.com`), and the Hermes/Blitz engine's own DOM
  (`Page::load` + `Document`). Nothing is mocked: there is no canned
  forecast anywhere in the module, and a run with no network fails loudly
  rather than showing invented weather.
- Asserts: the weather API answers 200 for a real place and reports a
  place name resolved by the real geocoder, a current temperature in a
  plausible range, and a multi-day forecast; the served page already
  contains that real temperature, so **no page-side network call is needed**
  (note: `window.fetch` was later added to `dom_runtime.ts` in `d1d9ee8` on
  2026-11-11, while `/weather` remains server-rendered by design); the engine's
  own DOM, after loading that page, holds the
  rendered temperature, the condition, the place, one row per forecast day,
  and the inline SVG weather icon; the page carries inline SVG for the icon
  rather than an emoji glyph, because the engine's font stack has no emoji
  font and an emoji renders as a tofu box; an unknown place is a 404 naming
  the real reason, and its page shows that reason with no temperature.
- Last observed: **PASS** 2026-11-11 — 16/16 checks, all ok.
- Note: the DOM assertions were mutation-tested. Rendering the temperature
  100° off makes the DOM check fail, and replacing the inline SVG with a
  text glyph makes both SVG checks fail. A test that cannot fail proves
  nothing, so both mutations were applied and observed to fail the suite
  before the real implementation was restored.
- Note: this is the first thing the browser is pointed at that is not the
  demo fixture's own campaign machinery. It exists to prove the browser
  renders an ordinary, real web app — real upstream data, real HTTP, real
  DOM, real vector icons — and not only the fixture's own pages.

### T-HERMES-PASSKEY-INTERSTITIAL

- Command: `./target/release/hermes-blitz-browser --passkeyinterstitialtest`
  from `apps/hermes-browser/`.
- Real components: the AOT-compiled Static Hermes runtime
  (`apps/hermes-browser/src/js/dom_runtime.ts`), the C bridge
  (`c_shim/hermes_shim.cpp` + `c_shim/blitz_bridge.h`), the Rust host bindings
  (`apps/hermes-browser/src/dom_ffi.rs`), the passkey broker
  (`apps/hermes-browser/src/credentials.rs`), the WebAuthn authenticator
  (`apps/hermes-browser/src/webauthn/authenticator.rs`), the macOS Keychain
  (`apps/hermes-browser/src/keychain.rs`), and the browser chrome
  (`apps/hermes-browser/src/chrome.rs`). Nothing is mocked or stubbed.
- Asserts: a page-initiated ceremony raises the interstitial and the page
  sees no credential while the prompt is up; the interstitial names the site
  and renders Allow and Deny; a click outside the dialog does not dismiss it
  and does not sign anything; every key press is swallowed while it is up, so
  Escape cannot dismiss it and a stray Enter cannot approve it; **Allow**
  registers the credential and then signs a real assertion with a real P-256
  signature that the page receives; **Deny** makes the page's promise reject
  and signs and registers nothing; an unanswered ceremony expires after its
  deadline, fails cleanly, and signs nothing; the ceremony is announced to
  the glasses as a `proposal.pending` with `requires_glasses: true`, and a
  `decision` frame from the glasses settles the ceremony.
- Last observed: **PASS** 2026-11-06 23:5x PT — 26 checks, all ok.
- Note: the check that a page cannot get a credential without a human is a
  real regression test, not a restatement of intent. Restoring the
  auto-approval that used to sit in `dom_ffi.rs` makes the Allow path fail
  with "the ceremony already completed", because the page then receives an
  already-finished ceremony instead of a signed assertion.
- Note: the registration and the assertion are *separate* ceremonies, and
  each needs its own answer. The test drives both, because a page must not
  be able to slip a registration past the gate and then reuse it.

### T-HERMES-PAGE-PASSKEY-GLASSES-E2E

- Command: `python3 services/hermes-host/test_decision_bridge_e2e.py
  PagePasskeyGlassesE2ETest` from the repo root.
- Real components: the release Hermes binary (`--control-socket`), a live
  HTTP server serving a page that really calls
  `navigator.credentials.create()`, `services/hermes-host/message_bus.py`,
  and `services/hermes-host/decision_bridge.py`. The glasses tap is
  represented exactly as the Android bridge emits it: a bus ack with
  `source="glasses_dat"`.
- Asserts: a page-initiated ceremony is announced as a `proposal.pending`
  with a `page-passkey-` proposal id, the bridge turns it into a bus card of
  kind `passkey` (both in `--card-mode immediate` and automatically in
  `--card-mode held` across both foreground and `background=True` supervised
  tabs without needing `POST /cards`), **nothing is decided before the glasses
  answer** (the browser does not approve a page's own ceremony by itself), the
  glasses tap decides it once with `decision: approve` and `surface: glasses`,
  full WebAuthn registration + ECDSA P-256 assertion against `signup_fixture.py`
  (`openssl` verifier) succeeds, and a replayed tap decides nothing a second
  time.
- Last observed: **PASS** 2026-11-11 — `python3 services/hermes-host/test_decision_bridge_e2e.py`
  7/7 tests PASS in `17.915s` (3 tests in `PagePasskeyGlassesE2ETest` + 4
  bridge tests).
- Note: this is the proof that the glasses DAT card really is the primary
  surface for a page's passkey prompt. The chain is the whole way round:
  the page calls the API, the browser announces the ceremony, the bridge
  cards it, the Android bridge sends a two-button DAT card to the glasses,
  the physical tap comes back as a bus ack, and the bridge turns that into
  the `decision` frame the browser is waiting for.

### T-HERMES-RELATED-ORIGINS

- Command: `cargo test --release webauthn::origin` from
  `apps/hermes-browser/` (28 tests), and `cargo test --release` for the
  whole suite.
- Real components: the Rust WebAuthn origin validator
  (`src/webauthn/origin.rs`), the related-origins procedure
  (`src/webauthn/related_origins.rs`), the passkey broker
  (`src/credentials.rs`), and the authenticator
  (`src/webauthn/authenticator.rs`). The tests run against a **real local
  HTTPS server** (`src/webauthn/test_server.rs`): a real socket, a real TLS
  handshake over a self-signed certificate, and real HTTP responses. Nothing
  is mocked — the fetch goes over the network stack to a live server.
- Asserts: the related origins validation procedure follows WebAuthn
  Level 3 §5.11.1 step for step — the well-known fetch must be `https:`
  without credentials or a referrer, the status after redirects must be 200,
  the content type must be `application/json`, the body must be a JSON
  object, and `origins` must be an array of strings, or the result is a
  `SecurityError`; the caller's origin must be same-origin with one of the
  listed origins, compared on scheme, host and port; the registrable-origin
  label cap is honoured; and a non-suffix RP ID is deferred to the procedure
  rather than refused outright.
- Asserts: a suffix RP ID is decided with **no fetch at all** — the test
  asserts the server's request log is empty.
- Asserts: the broker refuses an origin that did not pass the related
  origins check, so the flag that permits a cross-origin RP ID can only be
  set by a caller that actually ran the procedure.
- Last observed: **PASS** 2026-11-07 — `cargo test --release` 98/98, of
  which 28 are in the origin module (was 78/78 in total, 17 in the module).
- Note: the security properties are mutation-tested. Replacing the same-origin
  comparison with an unconditional accept fails three tests, including
  `refuses_an_origin_the_relying_party_does_not_list`. Removing the deferral
  entirely — which restores the original defect, where a cross-origin RP ID
  was refused outright — fails four, including
  `the_broker_refuses_an_origin_without_the_related_origins_check` and
  `the_whole_chain_lets_a_related_origin_through`.
- Note: the registrable origin label uses the last two labels of the host,
  which is exact only when the public suffix is a single label. It matches
  the default labeler in the go-webauthn reference implementation. The
  approximation can only make the label cap bite sooner; it never widens
  what a relying party's document may claim.

### T-HERMES-SOCKET-E2E
- Command: `python3 /tmp/socket_e2e.py` against the real Hermes GUI process.
- Real components: Unix socket, host/agent token roles, session, observe,
  propose, host approve, execute, stop.
- Asserts: the real counter increments once after host approval; the agent
  cannot approve its own proposal.
- Last observed: pass.
- Gap: the script lives outside the monorepo. Move or replace the script only
  under a claimed test task; do not create a second incompatible E2E.

### T-ANDROID-BUS-001

- Command: start `services/hermes-host/message_bus.py`, connect the installed
  Android bridge, POST one message to `/messages`, then inspect `/acks` and
  Android Notification Manager state.
- Real components: laptop bus, ADB reverse transport, installed Android app,
  Android Notification Manager.
- Asserts so far: message #1 was received by Android; Android returned a
  `received` ack; Notification Manager recorded a high-importance
  notification with Approve and Reject actions.
- Last observed: partial pass.
- Not yet asserted: the notification appeared on the glasses, and a physical
  Approve action returned to the laptop. The user had the glasses off during
  message #1.
- Current blocker: port `8899` collides with a pre-existing unrelated HTTP
  server. Walkaway moved to `19099`; message test 2 was received and acked by Android at 2026-11-04T19:58:49Z, with a high-importance notification recorded. Physical glasses appearance/action is still unconfirmed.


### T-GLASSES-DAT-REFERENCE-001

- Command: install the existing Ambient Link debug APK, connect Meta AI until
  green, then broadcast
  `com.lowkey.ambientlink.DEBUG_HUD_YANK` to package
  `com.lowkey.ambientlink`.
- Real components: Ambient Android app, Meta Wearables DAT SDK registration,
  Meta AI connection, paired Ray-Ban Display glasses.
- Asserts: DAT session and display reach `STARTED`; display request is sent;
  display content acknowledgement is received; the user physically sees the
  card on the glasses.
- Last observed: pass on 2026-11-04. Logs showed `createSession`,
  `addDisplay SUCCESS`, `display.state -> STARTED`,
  `Display request sent successfully`, and `Display content ack received`;
  the user confirmed “Saw the card.”
- Boundary: this proves the registered Ambient app's Android-to-glasses DAT
  hop. It does not prove Walkaway Bridge can push cards, because Walkaway has
  different package/credentials and no DAT integration yet. No Ambient sync,
  relay, or mesh code was copied.

### T-CONNECTOR-TOOLS-LIST

- Command: authenticated MCP `tools/list` POST to
  `https://muse.secure.build/mcp`.
- Real components: Caddy, hosted connector service, connector bearer auth.
- Asserts: HTTP 200 and a tools result.
- Last observed: pass. The user then saw six tools listed in Meta's form.


### T-MARKETING-SURFACE-001

- Command: `/tmp/fontenv/bin/python scripts/verify-marketing-site.py` from the
  repo root (fontTools 4.68.0 + brotli 1.2.1 required for the metric checks;
  plain `python3` runs the asset checks only and warns).
- Real components: `apps/marketing-site/index.html`, `assets/css/site.css`,
  the four shipped `assets/fonts/*.woff2` faces, and all 16 generated
  creatives in `apps/marketing-site/assets/posters/` and
  `apps/marketing-site/assets/banners/`.
- Asserts: every local `src`/`href` in `index.html` exists on disk; every
  `@font-face` URL resolves; each creative's `viewBox` matches the intrinsic
  size advertised for it in `index.html`; no `<text>` node overflows its
  canvas horizontally or vertically, measured with the real advance widths of
  the shipped faces and per-glyph fallback through the declared font stack;
  every non-ASCII character on the page and in the creatives is covered by at
  least one shipped font.
- Last observed: pass — 61 local references resolved, 4 font URLs resolved, 16
  creative viewBoxes matched, 89 creative text nodes fitted, 2 non-ASCII
  classes covered. Negative control: inflating one display line's `font-size`
  by 1.6x makes the script exit 1 naming the offending label
  (`'A LOCAL' (width 1888 > available 1800)`).
- Scope: static-asset integrity for the marketing surface only. It is not part
  of the demo path and proves nothing about a device hop. The browser-side
  check is a local render at `http://127.0.0.1:8099/` (preview server,
  `python3 -m http.server 8099 --bind 127.0.0.1` rooted at
  `apps/marketing-site/`): in-browser console query returned 0 entries and the
  in-browser network query returned 44/44 requests at HTTP 200 with 0
  failed/4xx/5xx, and the paid-placement filter was verified to show only the
  Square creatives when the Square tab is active.
- Responsive overflow sweep: the page is loaded in a same-origin iframe of
  fixed width (360, 390, 480, 768, 1024, 1280, 1600 CSS px) and every
  element's `scrollWidth`/`clientWidth` and bounding right edge are compared
  against the frame width. Last observed: zero overflowing elements and
  `documentElement.scrollWidth == clientWidth` at all seven widths (re-verified
  after the copy, artwork and mobile pass, and again after the mascot family and
  the baked hero square were added). Display
  type is sized by container-query units (`cqw`) from the headline's measured
  Druk Wide advance width, so it scales to its container instead of being
  guessed; measured hero size at a 390 px viewport is 33.02 px
  (0.94 × 346 px ÷ 9.851 em).


- Browser-measured creative fit: all 16 creatives are loaded into Chrome and
  every `<text>` node is measured with `getComputedTextLength()` and
  `getBBox()`. Last observed: 89 text nodes measured, 0 nodes with ink outside
  the artboard canvas in either axis (the largest deviations are 3-5 px of
  ascender / side-bearing ink past the declared `data-box`, which is ink
  overhang, not overflow). This is a real browser engine check, not a model of
  the font metrics.
- Hero artwork build: `/tmp/fontenv/bin/python scripts/build-hero-art.py` from
  the repo root (Pillow required). Real components:
  `apps/marketing-site/assets/download/hero-winged-chips.png` (source of the
  real Hermes browser crop), `muse-mascot.png`, `muse-chips.png`,
  `mark-walkaway.png`. Asserts: the script writes `hero-muse.png` (1920x1080),
  `hero-muse-flight.png` (1920x1080), `og-walkaway.png` (1200x630) and
  `square-muse.png` (1080x1080) into `assets/download/`, and every hero PNG
  re-rendered in Chrome shows the real Hermes window with the mascot and chips
  and no overlap on the baked green supervision square. Last observed: pass,
  6 files written, all four heroes re-rendered in Chrome without overlap.


### T-AI-RED-TEAM-WEB-001

- Command: `python3 scripts/verify-ai-red-team-web.py` from the repo root
  (Pillow and numpy required, because the check re-runs both media
  generators; ffmpeg/ffprobe required by the film generator).
- Real components: the built app in `apps/ai-red-team-web/dist/` (the Vite
  output that the server actually serves), `src/styles.css`,
  `src/lib/schedule.ts` (the exercise clock), `src/components/App.tsx` (the
  brand block), `src/lib/useScene.ts` (the scene switch), the generated
  manifest `src/media/scenes.ts`, the three self-hosted
  `public/fonts/*.woff2` faces (Michroma plus IBM Plex Mono 400/500), the
  shipped scenes in `public/media/scenes/`, the drawn sky in
  `public/media/space/deep-field.webp`, the films in `public/video/`, the
  media pipeline `scripts/build-redteam-media.py` together with its config
  `videos/scenes.json` and its source media in `videos/`, the sky generator
  `scripts/build-space-backdrop.py`, and the production server
  `server/index.mjs`.
- Asserts:
  1. every local reference in the **built** document resolves to a real,
     non-empty file;
  2. **the page makes no external request** — every subresource in the
     document and in the stylesheet is same-origin (a `rel="canonical"` link
     declares an address rather than fetching one, so it is allowed to be
     absolute);
  3. every `@font-face` URL resolves to a real, non-empty woff2;
  4. the brand block is exactly what it must be — an anchor whose `href` is
     `mailto:contact@model.tools`, whose text is `AI RED TEAM`, whose face is
     Michroma, with `San Francisco, CA` beneath it;
  5. the schedule in `schedule.ts` is one drill every **240 s**, the drill and
     the blue settle fit inside one period, the warning window is shorter than
     a period, and the warning is the window immediately before the next
     drill;
  6. **the scene pipeline holds together** — `videos/scenes.json` (14 scenes), the
     generated `src/media/scenes.ts` and the files on disk agree; every scene
     the manifest names exists and is non-empty; **the set contains both a
     still scene and a film scene**, so the page can actually switch between
     them; **every scene is the same size** — 1024×1024, the canonical stage —
     which is what makes the globe the same size and in the same place in every
     scene; and the document preloads the default scene's picture, so the
     preload cannot drift away from the default;
  7. **the globe is not carrying weather.** For every scene whose delivered
     source carried the storm — the five films and the one still marked
     `"storm": true` — the shipped picture must differ from the delivered one
     over the cloud, and the cloud's own light must be *lower* in the shipped
     picture than it was in the delivered one. The scenes delivered without a
     storm are not checked this way: their brightness is the sky, and a
     daylight sky is white in the same places a storm is;
  8. the shipped scenes are byte-for-byte the ones
     `scripts/build-redteam-media.py` produces from `videos/`, and the films
     still hash to the recorded value from the same source;
  9. the shipped sky is byte-for-byte the plate
     `scripts/build-space-backdrop.py` draws;
  10. the production server keeps resolved paths inside its dist directory,
      exposes `/healthz`, supports HTTP Range requests for the video, and
      answers a missing asset with a 404 rather than with the page — a
      missing picture must be able to say it is missing.
- Last observed: **PASS** 2026-11-24 15:0x — 5 local references resolved, 0
  external requests, 3 woff2 faces resolved, brand block exact (4 checks),
  schedule exact (6 checks), scene pipeline exact (59 checks: 14 scenes, both
  kinds present, every still and every film present, every scene at 1024×1024,
  preload matching the default), **the storm is off (12 checks)**, scenes
  matched the generator, sky matched the generator, server guard present
  (4 checks).
- Mutation-tested, because a test that cannot fail proves nothing. Four
  separate mutations were applied to the 2026-11-23 build and the restored
  files re-checked:
  - renaming a scene in the generated manifest to `city-spiral-RENAMED` made
    the suite exit 1 naming both the manifest check and the generator check;
  - tampering with the last 40 bytes of `deep-field.webp` made the suite exit
    1 with `stale deep-field.webp`;
  - tampering with the last 40 bytes of `city-globe-grey.jpg` made the suite
    exit 1 with `stale .../city-globe-grey.jpg`;
  - the mailto with `mailto:wrong@example.com` makes the suite exit 1 naming
    the expected address.
  All four files were restored and the suite passes again.
- **The storm check was mutation-tested separately**, because the storm is the
  one thing the owner asked to be gone and a statistic that cannot tell a
  storm from a daylight sky proves nothing:
  - shipping the *delivered* `city-spiral.jpg` instead of the repaired one
    made the suite exit 1 with `the scene city-spiral ships the storm
    unchanged (mean change 0.0)`, and the generator drift check caught it as
    well;
  - shipping the unrepaired first frame of `city-dive.mp4` as the film's
    picture made the suite exit 1 with `the scene city-dive ships the storm
    unchanged (mean change 0.0)`.
  Both files were restored and the suite passes again.
- **The storm was measured before and after.** The delivered sources carry the
  cloud over 20-26% of the top band of the frame (the grey plate 25.4%, the
  spiral 25.9%, the films 20.1-20.5%); after the repair the shipped pictures
  carry 0.3-3.7% of the frame over the same band.
- **The lightning is told from the city's light by shape, not by colour.**
  The flash is a bright violet blob and the city's own magenta light is in
  long thin streaks, and by saturation and hue the two are close (the flash
  at 79, the streaks at 65). What separates them is the shape: the flash
  fills 0.35 of its own bounding box and the streaks 0.17-0.18. The
  detector uses that, so the flash comes off and the city's light stays.
- **The drawn sky.** The page used to carry two NASA survey photographs of
  Andromeda — 580×498 and 550×373 px — stretched to `168vmax`, which is a
  five- or six-fold upscale of a thumbnail and is why the sky read as a pasted
  photograph. `scripts/build-space-backdrop.py` now draws the sky at
  4096×2304 instead: ~46,000 stars on a power-law magnitude distribution with
  a crowded galactic band, colour by spectral class, bloom and four-point
  diffraction on the brightest few, the Andromeda galaxy at an inclination
  with logarithmic arms and a dust lane, nebulae in the page's own green and
  magenta, and a dither pass so an 8-bit gradient of that size does not band.
  The render is deterministic (fixed seed), so the check re-draws it and
  compares bytes. Last observed: **4096×2304, 373 KB as WebP**, and the
  browser scales it *down* on every screen — no upscaling anywhere.
- **One canonical stage.** Every scene — still or film — is built at
  **1024×1024** (`CANVAS` in the pipeline): the delivery border is cropped and
  the frame Lanczos-resized onto it, and the stills are then masked to the
  circle inscribed in that square, so what the page carries is the globe and
  nothing around it. The delivered frames all have the globe filling them,
  centred, with the same framing, so staging them all onto one square is what
  makes the globe the same size and in the same place in every scene — and
  therefore what makes the crossing from one scene to the next a change of
  colour and camera rather than a jump. The old alpha-extraction generator
  (`scripts/build-orb-assets.py`) was retired: the orb is clipped to a circle
  in CSS, so the circle is the silhouette of the globe and a second,
  competing silhouette in the picture was a liability.
- **The crossing is a fade, and it was measured as one.** The page cycles the
  scenes on its own (`src/lib/useScene.ts`): a still is held for 7 s and a
  film for its own length, and two layers sit inside the orb while the page
  crosses — the one being left underneath and the one being arrived at over
  the top. Measured in Chromium against the real build at 1280×720, the two
  layers' opacities during a crossing run 0.995/0.005 → 0.894/0.106 → 0.753/
  0.247 → 0.540/0.460 → 0.364/0.636 → 0.164/0.836 → 0.019/0.981 — they sum
  to one the whole way. The crossing now runs over **2200 ms** and the
  arriving scene settles as it comes in while the one being left drifts away
  as it goes, so the crossing is a movement and not a dissolve between two
  still pictures. The order of the scenes is drawn at random, so the page
  does not come round the same way twice.
  **This is a real regression test, not a restatement of intent:** before the
  layer asked the browser for its size on mount, a picture that was ready the
  instant it appeared had no *before* to fade out of, and the crossing was a
  cut — the incoming layer jumped to 1 while the outgoing was still at 0.99.
  The crossing also waits for the arriving picture to be on the page before
  it starts, so a slow picture never leaves a hole in the globe.
- **The scene set is fourteen scenes**: four stills (`globe-day`, `globe-dusk`,
  `city-globe-night`, `city-spiral`), five camera moves (`city-globe-spin`,
  `city-dive`, `city-edge`, `city-zoom`, `city-sealevel`), the storm rolling
  over the city (`city-storm`), the red alert state (`city-alert`), and the
  owner's three new clips (`city-scan`, `city-globe-scan`, `city-redteam`).
  The two cloudy plates that are not scenes any more are not in it.
- **The films are a third of the speed they were delivered at**, and the
  mechanism is checked rather than assumed: the frames are interpolated up to
  the rate the slowed film needs — a camera that judders is a camera that has
  been dropped frames, not one that has been slowed — and their timestamps
  are stretched to match, or the film would be the same length and simply
  dropping frames. Built lengths, measured on the shipped files: the spin
  15.17 s (delivered 5.04 s), the storm 15.13 s (5.04 s), the camera moves
  27.13-38.25 s (9.04-12.75 s). **The owner's three new clips are slowed a
  half speed instead** (`city-scan` 20.000 s from 10.000 s, `city-redteam`
  20.000 s from 10.000 s, `city-globe-scan` 19.958 s from 9.983 s): they are
  already smooth and slow, and at a third of their speed the dark beat in the
  middle of the scan would sit on the page for three seconds, which reads as a
  fault rather than as a cut.
- **One delivered film does not fill its frame**, and the pipeline can crop a
  source in before it is staged (`crop` in the scene config, a fraction of
  the frame that is kept, centred). The clean globe scan is a glass sphere
  sitting inside a blurred city rather than filling the frame; measured on
  the source, the sphere runs from x=25 to x=695 of 720, so it is cropped in
  and staged like every other scene, which is what puts its edge on the orb's
  circle instead of showing the bokeh behind it. The owner then sent the
  same wave on a black background at 1080x1080 and 60 fps, and that is the
  version that ships: the blurred city was cropped away anyway, so the two
  were the same beat with different surroundings, and the black one puts the
  sphere's edge exactly on the orb's circle.
- **The frame rate of a delivered clip is read, not assumed.** The pipeline
  used to declare every clip to be 24 fps when it piped the frames to the
  encoder, so a clip delivered at 60 fps would have been played two and a
  half times too slow and had four frames in five thrown away by the
  interpolation. The rate now comes from the clip itself, the interpolation
  runs at that rate times the slow-down, and every film is delivered at
  24 fps whatever it arrived at. Checked on the shipped files: a 599-frame
  60 fps source slowed a half speed comes out as **479 frames at 24 fps
  (19.958 s)**, and a 600-frame 60 fps source as **480 frames at 24 fps
  (20.000 s)** — exactly twice the delivered length, which is what the
  slow-down means.
- **Two of the three clips were re-delivered at 1080x1080 and 60 fps, and
  they replaced the 720x720 24 fps versions rather than joining them.** They
  were measured against the ones already in the rotation before anything was
  changed: the red scan correlates **0.83-0.92** with the shipped
  `city-scan` and the RED TEAM HUD **0.82-0.96** with the shipped
  `city-redteam`, and their light curves are the same arc — the scan's
  blackout falls in the same 5.67-6.75 s window in both (27 frames at 24 fps
  against 55 frames at 60 fps, the same 1.1 s), so they are the same beats
  re-rendered and not new ones. Shipping them side by side would have put
  the same beat in the cycle twice. The superseded 720x720 sources are kept
  in the inbox as `city-scan-720.mp4` and `city-redteam-720.mp4`, the way
  `city-globe-scan-bokeh.mp4` is kept for the globe scan. The new render is
  framed tighter (the globe runs 96% of the frame across the shot, against
  80-100% in the old one), which the pipeline reproduces exactly rather than
  cropping: the delivered frame and the shipped frame show the same corner
  readouts cut by the orb's circle.
- **The globe is still the same size in the same place in every scene.** The
  disc was measured on the shipped media of all fourteen scenes — the stills
  from their alpha, the films from their first frame — and it is centred at
  (0.498, 0.498) of the frame with a diameter of 0.945-1.000 across the
  whole set. That is what makes a crossing change the colour and the camera
  and nothing else.
- **The storm is one deliberate beat, not the constant look it was.** The
  pipeline is told per scene which of its delivered sources carried a storm
  to take off (`repair_storm`), and the storm film is the one source that is
  not repaired — it is the storm. The check proves the other six are clean
  and says nothing about the storm film, which is supposed to have one.
- **The code red beat costs nothing to ship.** `city-alert` is the city under
  the red alert light with the laser sweeping the grid — the same visuals the
  four-minute drill uses — and it has no media of its own: it borrows another
  scene's picture, so the orb can go red without a second copy of anything.
  Checked in Chromium: opening it puts `is-alert` on the orb, the alert grade
  reads at 0.6 opacity, and the sweep is running.
- **The cycle is shuffled, and it never repeats itself back to back.**
  Checked in Chromium against the live site: over 95 s the page moved
  `city-globe-spin → city-storm → city-edge → city-dive`, which is not the
  order the scenes are declared in, and no scene was shown twice in a row.
- **Two behaviours of the cycle, checked in the browser against the live
  site.** With `prefers-reduced-motion: reduce` the cycle uses **only the
  stills and the alert** — observed order `globe-day → globe-dusk → city-globe-night →
  city-spiral` and round again, with **no film in it at all** — because the
  camera moves are films of the camera going into the city. And a scene
  opened directly with `?scene=city-spiral` is **pinned**: over twelve
  seconds the picture stayed `city-spiral` and the orb held a single layer,
  with no crossing.
- **The film loops on itself.** The delivered clip turns one full spin in 121
  frames and stops one frame short of where it started, so a film made by
  slowing it down ends a whole rotation out of step with its own first frame
  and the globe would jump forward nine degrees in a single frame at the
  loop. The pipeline appends a copy of the opening frames and trims the
  slowed film to end on it. Measured on the shipped `city-globe-spin.mp4`:
  364 frames, per-frame step median 3.775 and max 5.252 with no frozen tail,
  and the last frame against the first frame **2.111** mean absolute
  difference — against 8.906 for the naive build.
- Scene check after the 2026-11-23 rebuild, driven in Chromium against the
  real build: **no wireframe** (`document.querySelectorAll('svg ellipse,
  .orb__wire').length === 0`), **one drawn sky layer** and **no galaxy
  plates**, **two planets**, the globe at 209 px at 360 CSS px, 278 px at 480,
  430 px at 768 and 302 px at 1280×720; **zero horizontal overflow** at 360,
  480 and 768 (`scrollWidth === innerWidth`), no vertical overflow, the
  wordmark never clipped, the film playing (`currentTime` advancing,
  `paused === false`, `readyState === 4`), and the still served at its native
  1584×1584.
- Scene switching check: `?scene=city-globe-night` renders the still scene
  (no `<video>` in the orb) and `?scene=city-globe-spin` renders the film
  scene, both from the same manifest. An unknown `?scene=` falls back to the
  default rather than rendering nothing.
- Browser-side phase check: the built app is served by the real production
  server and opened in Chromium at 1280×720, which is where the phase behaviour
  was observed rather than modelled. The schedule takes a `?t=<sec>` offset,
  so every phase can be opened directly. Last observed: `nominal` at t=0
  (countdown `T-04:00`), `nominal` at t=225 (`T-00:15`), `warning` at t=230
  (`T-00:10`) and t=239 (`T-00:01`), `exercise` (the scanning class on, the
  countdown reading `Running`, the city reading `Laser scan`) at t=240 and
  t=263, `settle` (blue glow, `Complete`) at t=270,
  `nominal` again at t=280 (`T-03:20`), and the whole sequence repeating 240 s
  later — `warning` at t=470, `exercise` at t=480 — which is the drill firing
  exactly four minutes apart. **0 console errors and 0 external requests** in
  the same run.
- A real bug was found by the phase check and fixed: the countdown rendered
  `T-4:00` instead of `T-04:00`, because the minutes were only zero-padded when
  the remaining time was under ten seconds. The padding now applies to the
  minutes and the seconds independently, and the phase table above is the
  post-fix observation.
- Live-host check: **both** `https://redteam.model.tools/` and
  `https://red.model.tools/` answer HTTP/2 200 with the same title and the
  same headers (`strict-transport-security`, `x-frame-options: DENY`,
  `x-content-type-options: nosniff`, `no-cache` on the document); each host
  serves its own Let's Encrypt certificate; the video answers `206` with a
  correct `content-range` through Caddy on both hosts; hashed assets carry
  `immutable` caching; `/healthz` answers `ok` on both.
  Browser-verified against the live host in a **cache-free browser context**:
  each hostname stays on its own URL (no redirect) and renders the orb, the
  turning wireframe, the HUD, the wordmark, the 2K backdrop video and the
  `?t=240` drill, with 0 console errors. The cache-free context matters: the
  first deployment redirected `red.` to `redteam.`, and a browser that had
  already cached that 301 kept following it after the redirect was removed —
  a cached redirect is not evidence that the server still redirects.
  Re-run against the 2026-11-23 release `20261123-192803`: both hostnames
  answer 200 over HTTP/2 with the new hashed assets `index-Bms17xRB.js` and
  `index-vwvilzYV.css`, the drawn sky, the scenes and the film (206 with a
  correct `content-range`), and `/healthz` answers `ok`. Browser-verified on
  **both** hostnames in a fresh context: the default scene plays the film
  (`currentTime` advancing, `paused === false`, 624x624), `?scene=
  city-globe-original` swaps to the still (no `<video>` in the orb), and the
  page load makes **8 requests, all to its own origin, 0 external** and
  **0 console errors**.
- The Caddy stanza for this site lives in the repo at
  `apps/ai-red-team-web/deploy/redteam.caddy` and is byte-identical to the
  live `/etc/caddy/redteam.caddy` (sha256
  `5dc057c769ed5cfa72efa0712326c53cb64793f12c4f696cbe87ccd8f8db6dd1`), so
  the host configuration is reviewable and restorable from the repository.
- Scope: static-asset integrity, a real browser render and a live deployment
  check for the model.tools AI Red Team page. It is not part of the Hermes demo
  path and proves nothing about a device hop.


### T-WALKAWAY-DAT-001

- Command: build/install Walkaway Bridge, start the laptop bus on `19099`,
  connect Android, POST a message to `/messages`, then inspect `/acks`,
  Walkaway DAT logs, and the physical glasses.
- Real components: laptop bus, Walkaway Android app, Walkaway Meta DAT
  integration, Meta AI registration, paired Ray-Ban Display glasses.
- Earlier observed (glasses not worn): partial. Display-session creation
  reached `STARTING`, then returned `SESSION_ENDED_BY_DEVICE` and
  `NO_ELIGIBLE_DEVICE` before `STARTED`; Meta companion device state reported
  `peerConnectedState=Disconnected` and `isMounted=false`.
- Last observed: **PASS (P1 Hard Gate 1)** on 2026-11-04 with glasses worn and
  neural band connected. Bus message #7 (`P1 Gate: Approve`): Android
  `received` 00:40:46Z, `dat_sent` 00:40:49Z, logcat `session.state=STARTED` +
  `display.state=STARTED` + `DAT card sent`, physical glasses ack
  `{"message_id": 7, "action": "approve", "source": "glasses_dat"}` at
  00:40:55Z. Bus message #8 (`P1 Gate: Reject`): physical glasses ack
  `{"message_id": 8, "action": "reject", "source": "glasses_dat"}` at
  00:41:26Z. Prerequisite found during the run: the bus must not renumber
  message ids below the Android persisted `last_message_id` cursor (was 6);
  `message_bus.py` now persists NEXT_ID (`WALKAWAY_BUS_STATE`).

### T-GLASSES-WEB-REDIRECT-001

- Command:
  `apps/glasses-web/install-debug.sh --name Walkaway --url https://walkaway.secure.build/app/`.
- Note 2026-11-05: `apps/glasses-web/` deleted as vapor stub; this entry is historical (script recoverable from git history).
- Real components: ADB, Pixel, Meta AI (`com.facebook.stella`).
- Last observed: deep link launched successfully and Meta AI became the
  focused app. In-app confirmation and glasses-side installation were not
  physically asserted.

## Required before any demo claim

### T-GLASSES-DELIVERY-001

- Real components: laptop bus on the recorded Walkaway port, installed
  Android bridge, Pixel notification system, paired Ray-Ban Display glasses.
- Asserts: a new laptop message causes a Walkaway DAT card on the glasses; a physical Approve or Reject action reaches the laptop `/acks` log.
- Status: **PASS** on 2026-11-04 (see T-WALKAWAY-DAT-001: #7 Approve + #8
  Reject, both `source="glasses_dat"`). Ordinary Android notification posting
  remains a non-accepted delivery path; DAT is the accepted path.

### T-DEMO-VERTICAL-001 (Slice 1 — One-Shot Account Creation & Blind Vault Commit)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_slice1_signup.py`
- Real components: `services/demo-fixtures/signup_fixture.py` (localhost HTTP + SQLite backend), `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser` (`--control-socket`, `src/vault.rs`, `src/page_endpoint.rs`, `src/observe.rs`), real host and agent socket channels.
- Asserts:
  1. Initial RED failure recorded before Slice 1 implementation (`AssertionError: 'unsupported' != 'supported'` on `capabilities.text_input` / `forms`).
  2. Staging `fill_form` (including `action: "fill_secret"` from `vault.rs`) does not submit the form (`0` rows in SQLite before `submit` approval).
  3. Host approval of `submit` creates exactly `1` account row in SQLite with the exact speaker/CFP fields (`Ryan MacArthur`, `ryan@zunftmax.secure.build`, `Containment-First Sandboxing: What Firecracker Leaves as Homework`, `sandboxing`, `live_demo="yes"`, `confirmation_ref="yoink=926849de55"`).
  4. The SHA-256 of the generated 24-char secret in `HERMES_VAULT_PATH` matches `password_sha256` stored by the server, and the plaintext password appears nowhere in `observe.result`, proposals, events, `HERMES_AUDIT_PATH`, stdout, or stderr.
- Last observed: **PASS** (2 consecutive runs: `1.210s` and `1.426s` on 2026-11-04).

### T-DEMO-VERTICAL-002 (Slice 2 — Interactive Form Navigation & Self-Correcting Validation via Real Local Qwen3-4B)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_slice2_validation_and_llm.py`
- Real components: `services/demo-fixtures/signup_fixture.py` (`/cfp-socon` + SQLite `cfp_submissions` table), `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`, `services/agent-driver/hermes_agent_driver.py`, real local `mlx-community/Qwen3-4B-Instruct-2507-4bit` running via MLX on Apple Silicon GPU.
- Asserts:
  1. Initial RED failure recorded before Slice 2 implementation (`ModuleNotFoundError: No module named 'hermes_agent_driver'`).
  2. Submitting an incomplete CFP form (missing required `tshirt_size` and with `< 40`-char `abstract_summary`) commits `0` rows to `cfp_submissions` and renders `#validation-error` in the live Blitz DOM.
  3. `hermes_agent_driver.plan_validation_repair_with_local_llm` observes the validation error in Hermes and executes real local `mlx-community/Qwen3-4B-Instruct-2507-4bit` inference (`llm_calls == 1`) to synthesize the corrective `fill_form` operation (`tshirt_size="L"`, `len(abstract_summary) >= 40`).
  4. Host approval of the repaired `fill_form` + `submit` commits `1` verified row in `cfp_submissions` (`confirmation_ref="socon2027=cfp-accepted"`).
- Last observed: **PASS** (2 consecutive runs: `3.846s` and `3.455s` on 2026-11-04).

### T-DEMO-VERTICAL-003 (Slice 3 — Zero-LLM FormRecipe Recall & Instant Sign-In / Replay)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_slice3_recall_and_signin.py`
- Real components: `services/demo-fixtures/signup_fixture.py` (`/signup` + `/signin` + SQLite `accounts` & `logins` tables), `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser` (`src/recall.rs` + `src/vault.rs`), `services/agent-driver/hermes_agent_driver.py`.
- Asserts:
  1. Initial RED failure recorded before Slice 3 implementation (`AssertionError: None != 'supported' : Recall capability missing`).
  2. After initial supervised sign-in on `/signin`, `HERMES_RECALL_PATH` persists a `FormRecipe` keyed by `(origin, form_signature)` containing zero plaintext secrets.
  3. On repeat visit to `/signin`, `observe.result` exposes `form_signature` and `matched_recipe_id`; `hermes_agent_driver.plan_recall_or_llm` selects `{"kind": "replay_recipe"}` with `llm_calls == 0` in `< 5ms`.
  4. Host approval of `replay_recipe` + `submit` stages `#email` + blind-injects the vault password into `#password` and records a second authenticated login row in SQLite (`len(logins) == 2`), with zero secret leaks across `HERMES_RECALL_PATH`, `HERMES_AUDIT_PATH`, socket frames, stdout, and stderr.
- Last observed: **PASS** (2 consecutive runs: `2.006s` and `2.047s` on 2026-11-04).

### T-DEMO-VERTICAL-FULL (All 3 Acts Continuous Session + `message_bus.py` Approval Routing)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_full_storyline_vertical.py`
- Real components: `services/demo-fixtures/signup_fixture.py`, `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`, `services/agent-driver/hermes_agent_driver.py`, real local `mlx-community/Qwen3-4B-Instruct-2507-4bit`, and `services/hermes-host/message_bus.py`.
- Asserts:
  1. Executes Act 1 (`/signup` account creation + blind vault commit), Act 2 (`/cfp-socon` validation error + real local `Qwen3-4B-Instruct-2507-4bit` self-correction), and Act 3 (`/signin` initial login + `<5ms` zero-LLM `FormRecipe` recall replay) in one continuous Hermes session, completing with `task.done` (`state == "done"`).
  2. Routes all 13 proposal approvals through `services/hermes-host/message_bus.py` (`POST /messages` -> `GET /messages/next` -> `POST /ack` -> host `decision: "approve"`), asserting `len(acks) == 13` and SQLite counts `accounts == 1`, `cfp_submissions == 1`, `logins == 2`.
- Last observed: **PASS** (2 consecutive runs: `4.319s` and `4.485s` on 2026-11-04).


### T-BRIDGE-E2E-001 (P2 Decision Bridge — Socket-Level Real-System E2E)

- Command: `python3 /Users/mac/walkaway-monorepo/services/hermes-host/test_decision_bridge_e2e.py`
- Real components: release Hermes binary (`--control-socket`), SQLite demo
  fixture, `message_bus.py`, `decision_bridge.py`. Bus acks with
  `source="glasses_dat"` stand in for the physical tap exactly as the Android
  bridge emits the tap; the physical tap itself is T-WALKAWAY-DAT-001.
- Asserts:
  1. `requires_glasses == false` proposal is policy-approved by the bridge
     (`decided_by=policy`, `surface=commit_gate`) with no bus card created.
  2. `received` / `dat_sent` acks and forged `approve` acks from
     `android_app` and `e2e-bus-bridge` sources produce no Hermes execution.
  3. Glasses `reject` yields `proposal.invalidated` (`reason=rejected`) and
     `accounts == 0`; a re-proposed submit approved from `glasses_dat`
     commits exactly once (`accounts == 1`); a replayed glasses ack decides
     nothing twice.
  4. Held mode with 2 Hermes processes: a tampered batch manifest returns
     409; one explicit 2-proposal manifest card + one glasses approve fans
     out to two bound decisions on two sockets (`accounts == 2`).
- Last observed: **PASS** (2 consecutive runs: 8.205s and 8.291s on
  2026-11-04). Regression: `test_slice1_signup.py` still PASS (1.424s).

### T-BRIDGE-PHYSICAL-001 (P2 Decision Bridge — Full Physical Chain)

- Command: start bus on 19099, Android bridge connected, then
  `python3 /tmp/p2_physical.py` (real Hermes + bridge immediate mode +
  fixture `/signup`); wearer approves both DAT cards on the glasses.
- Real components: everything in T-BRIDGE-E2E-001 plus the physical Android
  DAT hop and worn Ray-Ban Display glasses.
- Asserts: `executed` receipts for `p2-fill` and `p2-submit` in bridge
  `GET /decisions`, and one account row in SQLite.
- Earlier observed: partial. Bridge bound card #9 to the real `p2-fill`
  proposal; bus recorded `received` 00:46:33Z and `dat_sent` 00:46:36Z; the
  run then timed out waiting for the glasses tap (150s window), so no
  `executed` receipt and `accounts == 0`.
- Last observed: **PASS** on 2026-11-04 (rerun with 600s windows, wearer
  tapping both cards). Card #10 (`p2-fill`): `dat_sent` 01:17:11Z, physical
  ack `approve`/`glasses_dat` 01:21:57Z. Card #11 (`p2-submit`): `dat_sent`
  01:22:01Z, physical ack `approve`/`glasses_dat` 01:22:10Z. Bridge logged
  `receipt executed` for both proposals; Hermes audit records both as
  `result=executed, decided_by=user, surface=glasses` (01:21:57.695Z and
  01:22:10.881Z); SQLite `accounts == 1` (`Ryan MacArthur`,
  `ryan@zunftmax.secure.build`). Script result: `P2 PHYSICAL CHAIN PASS`.

### T-ZONE-X-SUPERPOWERS (Zone X — Headed Commit-Only Gating, Semantic X-Ray HUD, and P-256 WebAuthn Vault)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_zone_x_engine_superpowers.py`
- Real components: `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser` (`--control-socket`, `--supervise-commits-only`, `--xray`, `src/vault.rs`, `src/chrome.rs`), `services/demo-fixtures/signup_fixture.py`.
- Asserts:
  1. Under `--supervise-commits-only`, routine non-secret `fill_form` auto-executes via `commit_gate` (`requires_glasses=False`) without blocking on a host decision.
  2. `fill_secret` and `submit` emit `proposal.pending` with `requires_glasses=True` and hard-freeze before commit (`0` rows in SQLite before approval).
  3. Rejecting `fill_secret` destroys the proposal and creates no vault credentials or WebAuthn signatures.
  4. Approving `fill_secret` generates a 24-char secret plus NIST P-256 keypair in `HERMES_VAULT_PATH`, and the secret is mirrored into the macOS Keychain. The DOM fill posts **no** WebAuthn assertion: spec §12 says "WebAuthn is not a DOM fill operation", so the fill fills the password and nothing else. A passkey assertion comes from the broker through the page API, never from a form field.
  5. Live-window screenshot (`include_screenshot=True`) renders the Semantic X-Ray overlay and telemetry bar (`#xray-telemetry-bar`, `#xray-vault-shield`, `#xray-freeze-shield`); zero secrets or private keys leak anywhere.
- Last observed: **PASS** (2026-11-06 12:0x PT; `--supervisortest` PASS with the broker-driven passkey checks, `--hurdletest` PASS including "the DOM fill posts no WebAuthn assertion; a passkey is not a DOM fill operation").

### T-ZONE-Y-10PORTALS-DISCO (Zone Y — 10 Conference Portals, Disco 10-Viewport Workbench, WebAuthn RP, and Reset)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_zone_y_10portals_and_disco.py`
- Real components: `services/demo-fixtures/signup_fixture.py`, `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`.
- Asserts:
  1. All 10 conference portals (`/cfp/1`..`/cfp/10`) render inside Hermes with identical `FormSignature` (`sig_c31d6574c15e07f6`); Portal #1 enforces the `< 40`-char abstract + `tshirt_size` curveball (`#validation-error`) while Portals #2–#10 accept replayed `FormRecipe` submissions.
  2. `/disco-workbench` renders all 10 live tiles (`#tile-1`..`#tile-10`) inside Blitz with non-zero bounding rects and live-measured timings/telemetry.
  3. `/api/webauthn/{challenge,register,verify}` verifies real OpenSSL ECDSA P-256 signatures and rejects wrong RP, wrong challenge, and replayed assertions.
  4. `/campaign-receipt` and `POST /api/reset` verify 10/10 submissions and `<1s` deterministic state reset.
- Last observed: **PASS** (`4.871s` on 2026-11-04).

### T-ZONE-Z-10PORTAL-CAMPAIGN (Zone Z — Full 10-Portal Disco Campaign, Resident Qwen3-4B, 3-Tap Glasses Coordinator & Reset Gate)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_10portal_disco_campaign.py`
- Real components: `services/agent-driver/parallel_campaign_driver.py`, `services/agent-driver/reset_demo.py`, `services/demo-fixtures/signup_fixture.py`, `services/hermes-host/message_bus.py`, `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`, real local `mlx-community/Qwen3-4B-Instruct-2507-4bit`.
- Asserts:
  1. Executes the full 10-portal campaign (`SoCon 2026` through `QCon SF`) with resident `Qwen3-4B-Instruct-2507-4bit` (`total_llm_calls == 1` across all 10 portals; Portals #2–#10 replayed via `FormRecipe` with `0` LLM calls).
  2. Emits exactly 3 glasses cards to `message_bus.py` (`Card #1` Passkey/Vault Auth, `Card #2` Portal #1 Repaired CFP Commit, `Card #3` Batch Commit 9 Conference CFPs carrying the explicit 9-portal `batch_proposals` manifest).
  3. Ignores non-glasses acks (`received`, `dat_sent`, `dat_failed`) and duplicate acks; rejecting Card #3 leaves Portals #2–#10 uncommitted (`total_submissions == 1`), while approving Card #3 commits all 10 portals (`total_submissions == 10`).
  4. `reset_demo.py --reset-demo` resets SQLite, workbench state, vault, recall, and audit logs in `~3ms` (`< 2000ms` gate), with zero secret or private-key leaks.
- Last observed: **PASS** (3 tests in `16.695s` on 2026-11-04; full 9-test discovery suite `python3 -m unittest discover -s tests/e2e -p "test_*.py" -v` PASS in `37.271s`).


### T-TRANSPORT-FREEZE-001 (P3 Frozen USB Transport + Recovery)

- Command: `adb reverse --remove tcp:19099`, then
  `services/hermes-host/start-transport.sh`, then POST one bus message.
- Real components: laptop bus on 19099, USB `adb reverse`, installed
  Walkaway Bridge on Pixel `67080DLDV009AD`.
- Asserts: script restores `tcp:19099 -> tcp:19099`, reports the single
  device and installed app, and the next message is `received` by Android.
- Last observed: **PASS** 2026-11-04. Message #12 `received` ~0.5s after
  posting; `dat_failed` followed with `SESSION_ENDED_BY_DEVICE` because the
  glasses were not worn (eligibility signal, not a transport failure).
- Frozen path: USB + adb reverse only. LAN procedure is documented in
  `services/hermes-host/README.md` as UNVERIFIED and requires a P1 re-run
  on LAN before any demo use.

### T-ANDROID-P4-001 (P4 4-State Demo-Safe UI + Dedupe)

- Commands: `./gradlew :app:testDebugUnitTest` and
  `./gradlew :app:connectedDebugAndroidTest` in
  `apps/android-notification-bridge/`, plus install/launch on the Pixel.
- Real components: `BridgeState.kt` (phase derivation + `IdDedupe`),
  `BridgeStatusPanel.kt`, rewired `BridgeScreen.kt`, laptop bus on 19099.
- Asserts:
  1. Unit: all 4 `BridgePhase` mappings; dedupe marks each id exactly once
     across instances over the same persisted set; eligibility hint mapping
     (`SESSION_ENDED_BY_DEVICE` / `NO_ELIGIBLE_DEVICE`).
  2. Instrumented: all 4 phase labels render; no `Approve`/`Reject` nodes
     exist in the panel composition.
  3. On-device: real app shows `DISCONNECTED`, then `CONNECTED — WAITING`
     after Connect; `Latest card #13` renders with zero phone decision
     buttons; after the test-run data wipe, bus backlog #9-#13 was carded
     exactly once each (one `dat_failed` per id in the new install).
- Last observed: **PASS** 2026-11-04 (unit + instrumented BUILD SUCCESSFUL;
  device states read via UIAutomator). `CARD_ON_GLASSES` and `DECIDED` on
  the real screen are NOT yet observed: DAT re-registration (wiped by the
  instrumented run) plus a worn-glasses tap are required; the phase logic
  and labels for those states are covered by the unit + instrumented tests.
- Cleanup in the same change: the unreachable template stack
  (`Navigation.kt`, `NavigationKeys.kt`, `ui/main/*`, `data/*` + template
  tests) was deleted; `MainActivity` renders `BridgeScreen` directly and
  nothing referenced the template stack except its own tests.


### T-LIVE-DEMO-001 (Full 10-Portal Physical Campaign)

- Command: `docs/DEMO_RUN.md` sequence (bus 19099 + fixture 8795 + Hermes
  `--control-socket` + `parallel_campaign_driver.run_10portal_campaign`),
  wearer approving glasses DAT cards. Final completing run used Hermes
  socket `muse_blitz.sock` with DB `live_demo.sqlite3` in
  `/var/folders/8j/5mwkcdz55xbcg1pg_54tlcf00000gn/T/walkaway-live-demo-psa_o5bz/`.
- Real components: release Hermes (primary + 8 workers), SQLite fixture
  (10 portals + disco workbench), resident Qwen3-4B, message bus 19099,
  Walkaway Android DAT bridge, worn Ray-Ban Display glasses.
- Last observed: **PASS** 2026-11-04 19:15 PT. `campaign_meta=COMMITTED`,
  `llm_calls=1`, `accounts == 1`, `cfp_submissions == 10`, 11 executed
  receipts with `decision_surface=glasses_dat`, bus `approve/glasses_dat`
  acks #27-#32. Earlier Lane P attempts in the same hour failed honestly:
  30s CLI tap-window timeout after a dismissed Card 2, and a
  `ConnectionResetError` when the bus restarted mid-run.


### T-SALVAGE-WS1WS2 (Salvage WS1 Dynamic Attach & Unified Campaign Driver + WS2 Bus/Stack Single-Instance)

- Commands:
  - `python3 services/hermes-host/test_decision_bridge_e2e.py` (3 cases incl. dynamic-attach case)
  - `python3 services/hermes-host/test_message_bus_lock.py`
  - `python3 tests/e2e/test_10portal_disco_campaign.py` (3 cases routing 100% of host supervision through `decision_bridge.py` `19101` held mode)
- Asserts:
  1. Bridge boots with zero sockets; `POST /sockets` attaches real Hermes host channels (primary + 8 parallel workers, HTTP 201 + live `session_id`), duplicate attach is 409, intermediate `stage_only: true` fills/probes are auto-approved by bridge policy (`requires_glasses=false`), and the 11 commit proposals pause in `GET /pending`, emit 3 glasses cards via `POST /cards`, and return verbatim `executed` / `proposal.invalidated` receipts (`webauthn_*`, `before/after_fingerprint`) via `GET /decisions`.
  2. Bus: a second instance on the same port via `0.0.0.0` (the 19:07 dual-bind shape) exits non-zero on the per-port flock (`/tmp/walkaway-bus-<port>.lock`); first bus untouched.
  3. Campaign driver + `HighBacklogThreadingHTTPServer` (`request_queue_size=128`): Card #3 fanout across 9 concurrent worker browsers commits all 10 SQLite CFP rows (`cfp_submissions == 10`), while rejecting Card #3 leaves Portals #2–#10 uncommitted (`cfp_submissions == 1`).
- Last observed: **PASS** 2026-11-04 21:06 PT (`test_decision_bridge_e2e.py` 3/3 in `9.061s`; `test_message_bus_lock.py` 1/1 in `0.172s`; `test_10portal_disco_campaign.py` 3/3 in `14.466s`; full 9-test `tests/e2e` discovery suite 9/9 in `29.783s`; `--supervisortest` 54/54 PASS).

### T-SALVAGE-WS3 (Android Hardening — Device Verification)

- Commands: `./gradlew :app:testDebugUnitTest :app:assembleDebug
  :app:installDebug :app:connectedDebugAndroidTest` in
  `apps/android-notification-bridge/` + ADB-driven checks on Pixel
  `67080DLDV009AD` against the live 19099 bus.
- Last observed: **PASS** 2026-11-05 07:23-07:26 PT.
  1. Install/launch: `installDebug` (upgrade in place, DAT registration
     preserved), auto-connect shows `CONNECTED — WAITING`.
  2. Foreground service: `dumpsys activity services` shows
     `BridgeKeepAliveService isForeground=true foregroundId=4101`
     (channel `walkaway_bridge_service`).
  3. Background polling: with the launcher in front, bus message #45 was
     `received` ~0.2s after posting; process state `cur=FGS cached=false
     isFrozen=false` (Android 15 freezer defeated while connected).
  4. Resend: #45 `dat_failed` (glasses not eligible) surfaced
     `Resend card #45`; tapping Resend produced a second `dat_failed`
     attempt for #45 (dedupe `forget` works; Resend re-offered).
  5. Instrumented: `:app:connectedDebugAndroidTest` BUILD SUCCESSFUL
     (panel: 4 phase labels, zero Approve/Reject nodes). Unit:
     `BridgeStateTest` 8/8.
- Aftermath (expected): the instrumented run uninstalls the app, so DAT
  registration read `AVAILABLE devices=0` after reinstall. One user
  re-registration via Connect Meta AI is owed before any glasses card.
- Not exercised (needs worn glasses): live DAT session REUSE path
  (both-STARTED predicate is unit-covered) and dismissal-after-sent
  `onCardGone` (send-failure Resend path above IS device-proven).

### T-ALIGNMENT-KILL-001 (Dead Controls + Vapor Folders Removal)

- Commands: `./gradlew :app:testDebugUnitTest :app:installDebug` in
  `apps/android-notification-bridge/` + ADB verification on Pixel
  `67080DLDV009AD` against live bus 19099.
- Last observed: **PASS** 2026-11-05. Notification for bus #46 carries zero
  actions (`dumpsys notification`); receiver + `notification_action` ack
  path deleted (no source references remain). Fresh-install blank bus
  token fails visibly (`HTTP 401` in `walkaway.poll` log + status);
  seeding `shared_prefs/walkaway_bus.xml` restored polling (#46
  `received` + `dat_failed`, DAT then `AVAILABLE`). Vapor folders
  `apps/glasses-web`, `apps/phone-approval-web`,
  `apps/desktop-supervision`, `packages/*` deleted with zero code
  references; STATUS/source-map updated same change. Diagnostic poll
  logging that found the 401 was reduced to effect-start + failure lines.

### T-TRACKB-DUAL-001 (Track B Dual-Surface Approval: Glasses OR Phone)

- Commands: `HERMES_HIDDEN_WINDOW=1 python3 services/hermes-host/test_decision_bridge_e2e.py`
  (4 cases) and `HERMES_HIDDEN_WINDOW=1 python3 /tmp/trackb_phone_e2e.py`
  (real Pixel UI taps) plus
  `apps/android-notification-bridge/scripts/run-device-tests-preserve-data.sh`.
- Last observed: **PASS** 2026-11-05.
  - Bridge suite 4/4 (12.738s): glasses-only mode still rejects
    `android_app`; dual mode commits from `android_app` and from
    `glasses_dat`, first tap wins (loser ack ignored), domain-first card
    title <=80 / body <=160, Hermes audit surfaces asserted per proposal.
  - Phone E2E: `TRACK B PHONE SURFACE PASS` — cards #47/#48 decided by
    tapping the real **Approve on Phone** Compose button via ADB input
    with glasses unused (DAT unavailable); `accounts == 1`; audit +
    bridge surfaces both `android_app`.
  - Preserve-data runner: instrumentation `OK (1 test)`,
    `firstInstallTime` unchanged, `dat_registration.xml` intact — device
    tests no longer wipe Meta DAT registration. Identity pin:
    `scripts/verify-app-identity.sh` PASS (package + cert SHA-256
    `abdf2c3a...`, keystore created 2026-05-02, unchanged).
- Not exercised: phone-dismisses-live-DAT-card sync with glasses worn
  (`dismissActiveCard` shipped; needs a worn-glasses pass).

### T-TRACKB-DUAL-002 (Track B Phone Approve + Reject, separate live cards)

- Command: `HERMES_HIDDEN_WINDOW=1 python3 /tmp/trackb_phone_reject_e2e.py`
  (throwaway harness over the maintained bridge/Hermes/Android surfaces;
  real Compose buttons tapped through ADB input on Pixel `67080DLDV009AD`).
- Last observed: **PASS** 2026-11-05. Card #52 fill approved on phone;
  card #53 rejected on phone (`proposal.invalidated`, bridge decision
  `reject`/`android_app`, audit `result=rejected surface=android_app`,
  accounts stayed 0); card #54 re-proposed and approved on phone
  (`executed`, accounts became 1). Per-tap responsiveness was sampled over
  successive post-tap UI frames: input dispatch 146-152 ms, with the second
  button-absent frame observed at 2,598-2,649 ms under the 250 ms dump cadence;
  screenshots in `/tmp/trackb_reject/` show both live phone buttons and the
  final `Card #54 decided via android_app: approve` state. No product code
  changed during the verification rerun; one over-broad harness assertion was
  narrowed from all audit rows to the audit row whose result is `rejected`.

### T-MUSE-MCP-REAL-BROWSER (Track A — Muse MCP -> Real Hermes Browser + Live HTTPS Webpage + Glasses/Phone Dual Approval)

- Command: `HERMES_HIDDEN_WINDOW=1 python3 /Users/mac/walkaway-monorepo/tests/e2e/test_muse_mcp_real_browser.py`
- Real components: `services/muse-connector/mcp_server.py`, `services/agent-driver/hermes_task_relay.py` (`:8790`), `services/agent-driver/hermes_agent_driver.py` (`plan_general_web_form_with_local_llm` + local `mlx-community/Qwen3-4B-Instruct-2507-4bit`), `services/hermes-host/decision_bridge.py` (`--allow-android-approval`), `services/hermes-host/message_bus.py`, and release `apps/hermes-browser` (`--control-socket`, hidden window).
- Asserts:
  1. `test_muse_mcp_drives_real_hermes_browser_and_commits_on_glasses_or_android_approval`: Invokes `mcp_server.py` JSON-RPC `tools/call` (`walkaway_start_task` with `url` + `goal`, `walkaway_get_state`), verifies real Hermes navigates, stages form fields via local `Qwen3-4B` (`llm_calls == 1`), pauses at `waiting_for_human` with a domain-first card (`<=80` title, `<=160` body), and commits only after either a `glasses_dat` (`surface="glasses"`) or `android_app` (`surface="android_app"`) approval ack (`0` rows before approval, `1` row after approval).
  2. `test_live_internet_https_httpbin_form_via_muse_mcp`: Drives live external `https://httpbin.org/forms/post` over HTTPS through `mcp_server.py` (`walkaway_start_task`), fills customer/pizza fields via local `Qwen3-4B`, emits `COMMIT httpbin.org: ...`, approves via `source="android_app"`, and verifies the real HTTPS POST response from `https://httpbin.org/post` inside Hermes (`"custname": "Ryan MacArthur"`).
- Last observed: **PASS** 2026-11-11 (`2/2` tests as part of `HERMES_HIDDEN_WINDOW=1 python3 -m unittest discover -s tests/e2e -p "test_*.py" -v` — 11/11 OK in `43.338s`, 100% headless with zero visible desktop windows).

### T-FGS-CRASH-001 (BridgeKeepAliveService dataSync Crash Loop — Fix)

- Observed failure 2026-11-05 22:29 PT: `ForegroundServiceDidNotStopInTimeException`
  (Android 15 dataSync quota timeout, service never stopped itself), then
  `START_STICKY` restart into `ForegroundServiceStartNotAllowedException:
  Time limit already exhausted for foreground service type dataSync` at
  `BridgeKeepAliveService.kt:40` — app crash-loop, process dead.
- Fix: type `dataSync` -> `connectedDevice` (manifest permission + service
  type + runtime `FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE`),
  `START_STICKY` -> `START_NOT_STICKY`, `onTimeout()` stops cleanly,
  `startForeground` and companion `start()` are failure-tolerant
  (degraded polling, never crash), `onDestroy` removes foreground state.
- Last observed: **PASS** 2026-11-05 22:36 PT. Preserve-data device tests
  green; dumpsys shows `isForeground=true types=0x00000010` on pid 13565;
  crash buffer clean after reinstall + relaunch + restart attempts;
  process stable. Not exercised: full Disconnect->Connect UI cycle in
  this round (user held phone foreground; service start/stop calls are
  unchanged from the previously proven path + new onDestroy guard).

### T-CASHPAINT-001 (Cash App Paint — Host Green, Device Render Pending)

- Change: Cash palette (signature green `#00D54B`, black/white surfaces,
  flat dark mode), `WalkawayShapes` (12-36dp radii), bold tight type scale,
  flat green launcher background. Files: `ui/theme/{Color,Theme,Type,Shape}.kt`,
  `res/drawable/ic_launcher_background.xml`.
- Last observed: host **PASS** (`:app:testDebugUnitTest :app:assembleDebug`
  BUILD SUCCESSFUL, fresh APK). On-device install + screenshot comparison
  **PENDING**: Pixel left USB mid-verification (`adb devices` empty).
  Owed on reconnect: preserve-data install, screenshot of BridgeScreen in
  light (and dark if toggled), confirm buttons green/white text and panel
  radius match theme — visual result is NOT claimed yet.

### T-SALVAGE-641-POLLER (6.4.1 — Bus Poller Moved into BridgeKeepAliveService, Activity-Destroyed Ack)

- Change: polling loop moved out of the `BridgeScreen` Activity composition
  into `BridgePoller` (process-wide scope) started/stopped by
  `BridgeKeepAliveService` (`BridgePoller.kt`, `BridgeKeepAliveService.kt`,
  `BridgeScreen.kt`). The screen is a pure `StateFlow` observer + control
  surface. FGS promotion refusal now leaves polling degraded on the poller
  scope instead of dying with the Activity.
- Commands: `./gradlew :app:testDebugUnitTest :app:assembleDebug
  :app:installDebug` in `apps/android-notification-bridge/` (upgrade in
  place; DAT registration preserved) + ADB-driven proof on Pixel
  `67080DLDV009AD` against live bus `127.0.0.1:19099`
  (`WALKAWAY_BUS_TOKEN=walkaway-dev-bus-token`, `adb reverse tcp:19099`):
  launch app -> `cmd activity stack remove <taskId>` (Activity destroyed)
  -> `am kill com.example.walkawaybridge` -> `POST /messages` ->
  `GET /acks`.
- Last observed: **PASS** 2026-11-06 18:36 PT (2026-11-05 07:23 PT era
  evidence for the service half remains in `T-SALVAGE-WS3`).
  1. Host: `BridgeStateTest` 8/8 PASS; `assembleDebug` BUILD SUCCESSFUL.
  2. FGS: `dumpsys activity services` `isForeground=true foregroundId=4101
     types=0x00000010` (connectedDevice) while polling.
  3. Activity destroyed: `cmd activity stack remove 4692` ->
     `walkawaybridge/.MainActivity` count in `dumpsys activity activities`
     is **0**; `am kill` does NOT kill the process (pid 10417 alive —
     FGS-protected).
  4. **6.4.1 evidence**: `POST /messages` (id 60, 18:36:26.364Z) with zero
     Activity alive -> `GET /acks` shows
     `{message_id: 60, action: "received", source: "android_app"}` at
     18:36:26.811Z (~0.45s).
- Note: bus state was restarted with `next_id: 60` because the phone's
  persisted `last_message_id` cursor (54) outranked the fresh bus ids —
  setup artifact only, not product behavior.
- Not exercised (needs worn glasses): DAT card delivery for #60
  (expected `dat_failed` without glasses), live session-reuse fast path,
  dismissal-after-sent `onCardGone`.

### T-SALVAGE-643-LAUNCHER (alignment 6.4.3 — One Lifecycle Owner / Single Launcher)

- Change: deleted `scripts/salvaged-demo-live/` (`spawn_demo.py`,
  `run_demo.py`, `run_campaign3.py` — the salvaged `/tmp/demo-live` copies;
  recoverable from git `c48b782`). Exactly one launcher remains:
  `services/agent-driver/run_live_demo.py` (stack flock + `--detach` +
  `glasses_timeout_s=900`). `docs/DEMO_RUN.md` launch section rewritten to
  that launcher; `README.md` quickstart step 1 re-pointed to
  `apps/hermes-browser/target/release/hermes-blitz-browser` (the removed
  standalone `/Users/mac/hermes-blitz-browser` tree was a stale path).
- Commands (from README only, repo root): `--supervisortest`;
  `python3 services/hermes-host/test_message_bus_lock.py`;
  `python3 services/hermes-host/test_decision_bridge_e2e.py`;
  `python3 -m unittest discover -s tests/e2e -p "test_*.py" -v`; plus a
  stack-lock fail-fast check (hold `/tmp/walkaway-demo-stack.lock`, run
  `run_live_demo.py`, expect non-zero).
- Last observed: **PASS** 2026-11-06 ~19:0x PT. `SUPERVISOR TEST PASS`;
  bus lock 1/1 OK; bridge e2e 4/4 OK (`14.481s`); full e2e discovery
  **11/11 OK** (`60.585s`); second launcher exits 1 with
  `STACK LOCK BUSY: ... Stop the existing stack before starting a second one.`
- Not claimed: a literal fresh-machine boot of README step 4
  (`run_live_demo.py --detach` with physical glasses) — that remains the
  physical glasses runbook (`LIVE DEMO RUN` row in `docs/STATUS.md`); the
  launcher path, lock, and suites above are the host-side evidence for
  "exactly one launcher".

### T-CARD-PIPELINE-001 (Page-Capture Preview + Card Fields + Muse Result)

- Protocol: `observe.request {include_screenshot:true}` -> `observe.result
  {screenshot_png_b64}` -> relay downscales to max 320px wide PNG/JPEG
  (budget `PREVIEW_MAX_B64_CHARS=48000`) -> `GET /api/state
  {preview_b64, result}` -> Muse `public(state) {page_preview_b64, result}`;
  bus `POST /messages` carries optional `image_url`/`price`/`detail`/
  `image_b64` (b64 capped 48k, POST body limit 128KB) -> phone shows the
  preview crop on the card row. Glasses card text stays the proven flat
  DAT template (shaped-chrome work parked per Ryan 2026-11-06).
- Commands: inline downscale budget check (synthetic 1280x720 PNG ->
  320x180, b64 612 chars); `test_message_bus_lock.py`; `test_decision_bridge_e2e.py`;
  `HERMES_HIDDEN_WINDOW=1 tests/e2e/test_muse_mcp_real_browser.py`; Android
  `:app:testDebugUnitTest :app:assembleDebug`.
- Last observed: **PASS** 2026-11-06 ~22:2x PT. Downscale helper OK;
  bus lock OK; bridge e2e OK; muse MCP e2e 2/2 OK (`13.695s`); Android
  build + unit tests green (`WindowChrome.kt` compiles — shaped chrome
  asset, not yet wired into `sendContent`).

### T-MOCK-MUSE-PURCHASE-001 (Mock Muse Purchase Card Demo)

- `scripts/mock_muse_purchase.py` — purchase card (title/price/detail +
  mock 320px PNG within the 48k budget) posted to the live bus, waits for a
  real human decision, prints the Muse `result` object. Mock payload is
  covered by the verbatim RYAN EXPLICIT MOCK/FALLBACK AUTHORIZATION comment
  in the file (2026-11-06 "just a fake" demo directive).
- Last observed: **PARTIAL PASS** 2026-11-06 ~22:5x PT. Bus accepted the
  card (`POST /messages` 201, id 70, image_b64 stored); no human decision
  within the 8s bounded run (expected — no user present).
  **Owed on device reconnect:** phone render of the preview crop +
  `received`/`dat_*` acks for a mock card; **owed with Ryan:** one real
  Approve/Deny tap to close the loop and print the Muse result.
