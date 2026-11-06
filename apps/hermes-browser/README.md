# hermes-blitz-browser

A browser assembled from existing engines, following the composition described in
`walkaway-hackathon/herbarium.md`: **Static Hermes** executes the page logic
(compiled ahead of time to C), **Blitz** (Stylo + Taffy) owns HTML/CSS and layout,
and **Vello on wgpu** rasterises. No browser engine was written from scratch.

It has **Netscape 6 Modern-inspired** navigation tools and compact menu controls,
with the original MIT-licensed Swift `CompactTabStrip` hosted in the existing
macOS window. Each tab carries its own address; the selected tab is the native
address/search editor, using the existing Safari-style `.address` component
mode. There is no separate Location row or second address widget. This replaces
the earlier partial Rust imitation and separate address-field integration.
The integrated Hermes JavaScript debugger runs explicitly loaded local scripts
in an isolated interpreter, with real breakpoints, stepping and frame evaluation.
Every control is wired to real state:
navigation fetches over the network, history is a real stack, tabs are real
documents, and disabled buttons are disabled because there is genuinely nowhere
to go.

```
winit ──input──> chrome hit-test ──> back / forward / reload / address bar
              └─> page hit-test ──> link? navigate : Hermes dispatch
                                        │
                                        │ $SHBuiltin.extern_c
                                        ▼
                                  Blitz DocumentMutator
                                        │ damage flags
                                        ▼
                             Stylo + Taffy re-resolve ──> Vello/wgpu ──> window
```

## Browser chrome

The browser keeps its existing Blitz-rendered chrome and native event routing.
The Netscape-inspired skin does not embed another browser or change the page
engine. On macOS, the actual Swift tab component owns retained native views,
Core Animation, drag reordering, pinning, close-width behavior, hover panels and
tear-off visuals. Rust callbacks move the real tab/document/runtime between
browser windows. The native row replaces the Rust row; both are never active
at once. Supervised tabs cannot detach or transfer while bound to an approval
session, so moving a tab cannot bypass host authority.

Original Swift source, revision and file hashes are in `native-tabs/VENDOR.json`;
licenses are in `native-tabs/LICENSE` and `THIRD_PARTY_NOTICES.txt`. The
non-macOS/offscreen fallback is explicitly not claimed to match Swift. Native
Glass is available on macOS 26+; older versions use the component's own fallback.
No smoothness parity is inferred from copied math: native integration needs its
own behavior, capture, lifecycle and timing checks.

| Control | Behaviour |
|---|---|
| Theme | Follows the system appearance (`window.theme()` + `ThemeChanged`); pages get the matching `prefers-color-scheme` |
|---|---|
| Tabs | One `Tab` per document: own history, own Blitz document, own Hermes runtime |
| `+` | Opens a real new tab on the start page |
| `×` on a tab | Closes it; closing the last tab replaces it with a fresh one |
| Tab title | From the document `<title>`, falling back to the host |
| Back / forward | Real history stack, truncated on new navigation; greyed when exhausted |
| Reload | Re-fetches the current URL and replaces the history entry |
| Address inside each tab | Original CompactAddressField: select a tab, click its address or `⌘L`; normal cursor/selection/clipboard/IME, Enter to navigate that tab, Esc to revert; internal start URL is hidden |
| Loading | Real indeterminate native spinner and Modern throbber/status; Reload becomes Stop during an in-flight navigation; no fabricated percentage |
| Tab identity | Original native tab identity/security state follows its own URL; no shared Location widget |
| Browser menu | Home, Reload, Back, Forward, JavaScript debugger, Close tab — each performs the real action |
| Debugger | Toolbar/menu, `F12` or `⌘⌥J` toggles the isolated local script debugger |
| URL handling | Absolute URLs, bare hosts (`example.com` → `https://`), anything else becomes a DuckDuckGo search |
| Links | Clicking an `<a href>` navigates, resolving relative URLs against the page |
| Scrolling | Mouse wheel scrolls the document |
| Shortcuts | `⌘L` focus, `⌘R` reload, `⌘T` new tab, `⌘W` close tab, `⌘[` / `⌘]` back / forward |

Navigation runs on a worker thread and reports back through a winit user event,
so the window stays responsive while a page loads. Subresources (stylesheets,
images, fonts) load through `blitz-net` on a Tokio runtime and repaint as they
arrive.

## Native tab/address and startup verification

- `--nativetabtest` exercises the original component in real AppKit windows:
  editor selection/clipboard commands, Enter/Escape/draft handling, actual
  spinner state, native cells, move/pin/close-width freeze, guarded transfer,
  capture, retained identity and teardown. It is a component test, not a claim
  that physical gestures or perceptual parity were independently measured.
- `--tabhosttest` verifies the browser moves the same real page/Hermes runtime
  and history between owners rather than cloning or reconstructing it.
- `--capture-window /absolute/path.png --exit-after 2` captures actual Vello
  content plus the native tab/address pixels after the first submitted frame.
  Separate transient hover/drag panels are not included in this steady-state
  screenshot API. `--render` alone remains the explicitly non-native fallback.
- Native tab animations run through the original AppKit/Core Animation code,
  not the Rust 16ms chrome-reparse loop. Identical bridge updates are no-ops.
  Debugger backend snapshots mutate retained DOM text/classes and preserve
  source/details scrolling and selection instead of replacing the document.
- Startup avoids provisional/duplicate chrome construction before actual
  window scale/theme/native initialization. `[startup] first_submitted_frame_ms`
  measures main-entry to GPU submission, **not visible presentation or cold
  filesystem-cache latency**. No cache purges or unverified Metal cache hack is
  used. Five observed fresh-process runs: 390.49, 476.80, 336.95, 273.01,
  277.03ms; no comparative speedup is claimed from these measurements.

The original per-tab field notifications keep the draft/query seam and a stable
cell anchor for future autocomplete; no suggestions provider or fake popover is
installed. The tab row starts at 74px and page content at 112px, reclaiming the
former 32px Location row.
The Browser dropdown temporarily hides native overlays so its items remain
clickable; showing them again preserves their retained views.

## JavaScript debugger

Open **JS Debugger** in the toolbar or menu, press `F12` or `⌘⌥J`, or launch
with `--debugger`. `--debug-script /absolute/path/script.js` loads a UTF-8 local
script without executing it. Click the source editor to type or paste (`⌘V`);
`⌘A` selects the whole input for replacement and `⌘C` copies it. The initial
example has a real line-2 breakpoint. Click gutter line numbers before Run to
set or remove breakpoints. Run, Pause, Resume, Step over/in/out and Stop are
connected to Hermes's debugger API. The pane shows copied stack frames, lexical
scopes, locals, console output, errors and expression results. Select an
expression and press Enter or Eval to evaluate it in the top paused frame.

This is a **local JavaScript scratchpad debugger, not webpage DevTools**.
Static Hermes compiles the trusted page adapter to native C; its AOT frames
cannot provide interactive bytecode breakpoints or stepping. The scratchpad
therefore owns a separate interpreter on a worker thread, with no Blitz DOM,
credential vault, approval channel, filesystem or network bindings. It never
automatically fetches or executes remote page scripts. Sources and displayed
values are bounded, and Stop/teardown interrupt runaway execution. Closing the
pane hides it; Stop ends the script. Debugger space is removed from the page's
viewport, and pane input never falls through to the page.

```sh
./target/release/hermes-blitz-browser --debugger
./target/release/hermes-blitz-browser --debug-script /path/to/example.js
./target/release/hermes-blitz-browser --debuggertest
```

## Status

| Check | Command | Result |
|---|---|---|
| Headless end-to-end | `--selftest` | PASS — Hermes mutation, history, URL normalization, toolbar hit-tests with menu closed and all dropdown actions with menu open |
| Local JS debugger | `--debuggertest` | PASS — 28 real Hermes assertions for breakpoint/stack/locals, all step modes, eval, isolation, output/errors, pause, restart and interrupt/teardown |
| Release tests | `cargo test --release` | PASS — 35 tests, including retained debugger DOM, compact address-in-tab layout/loading, real backend and native browser suites |
| Original native tabs/address | `--nativetabtest` | PASS — 72 real AppKit assertions |
| Real tab ownership | `--tabhosttest` | PASS — 10 checks including same runtime/history transfer, pinned grouping and stale-address rejection |
| Chrome interactions | `--uitest` | PASS — real hit-tests for tabs/navigation, stable identity, debugger opening/focus, viewport reservation, actual Hermes breakpoint and close |
| Real window + GPU | `--exit-after 4` | Runs, submits frames, exits cleanly |
| Offscreen render | `--render out.png [--click] [--menu] [--url <url>] [--dark] [--size WxH]` | Renders start page, menu, or a fetched page, in either theme, at any size |
| Real navigation | `--render out.png --url rust-lang.org` | Fetches, loads 58 subresources, lays out and paints the live site |
| Dark page plumbing | `--render out.png --url example.com --dark` | Page flips to its own `#222` via `color-scheme`, text stays readable |
| Docked width | `--render out.png --dark --menu --size 560x760` | Chrome fits the ~560px desktop panel without overflow |
| Static Hermes embed | `_sh_init` → unit → JSI call | Verified |

The click path was verified by rendering before/after through the real paint
pipeline: the button turns `#16A34A`, its label increments, and the paragraph
above it is rewritten by the AOT-compiled handler.

## Layout

```
Cargo.toml                 crate manifest (pinned dependency set, see below)
build.rs                   shermes -> C -> objects; links libhermesvm + libjsi
c_shim/blitz_bridge.h      C surface shared by dom_runtime.ts and the host
c_shim/hermes_shim.cpp     SHRuntime lifetime, JSI dispatch, context binding
src/js/dom_runtime.ts      page logic, AOT-compiled by shermes
src/dom_ffi.rs             Rust side of the document bridge (Blitz mutator)
src/chrome.rs              toolbar HTML/CSS, regenerated from state
src/browser.rs             URL parsing, history, off-thread fetch
src/main.rs                winit host, input routing, event loop, render modes
scripts/build-hermes.sh    builds Static Hermes (static_h)
scripts/run.sh             build-if-needed + run
examples/minwin.rs         bare winit window, for isolating display issues
examples/glyphs.rs         font-coverage probe for toolbar glyphs
```

## Build

Prerequisites: Rust (edition 2024), CMake ≥ 3.21, Ninja, a current Xcode SDK
with SwiftUI/AppKit Glass declarations, Swift compiler, and Python 3. The native
component deploys to macOS 13+ with runtime availability guards; packaged app
minimum remains declared in its Info.plist. No Swift package download or local
external checkout is needed: pinned sources are vendored and checked at build.

```sh
./scripts/build-hermes.sh     # ~35 s on 18 cores; clones static_h if absent
cargo build --release
```

`build.rs` reads `HERMES_SRC_DIR` / `HERMES_BUILD_DIR` and defaults to
`~/.cache/hermes-static_h/src` and
`~/.cache/hermes-static_h/build-walkaway-debugger`. The latter is a separate
**debugger-enabled** build cache; it leaves older debugger-OFF demo builds
untouched. Packaging uses this same dylib and validates the debugger in the
mounted DMG.

## Run

```sh
./target/release/hermes-blitz-browser            # window, follows system theme
./target/release/hermes-blitz-browser --force-dark     # force the dark palette
./target/release/hermes-blitz-browser --selftest # headless assertions
./target/release/hermes-blitz-browser --uitest   # chrome interaction assertions
./target/release/hermes-blitz-browser --render shot.png --click
./target/release/hermes-blitz-browser --render site.png --url rust-lang.org
./target/release/hermes-blitz-browser --render dock.png --dark --menu --size 560x760
./target/release/hermes-blitz-browser --exit-after 4   # run the loop, then quit
```

## Deviations from the document, and why

The document is a design sketch; several of its specifics no longer resolve
against the published crates or the current `static_h` API. Each change below was
forced by a compiler or a runtime failure, not preference.

### Dependency set

| Document | Actually used | Reason |
|---|---|---|
| `blitz-* = "0.2"` | `0.3.0-beta.4` | blitz 0.2.1 requires `anyrender ^0.6`, but no published `anyrender_vello` targets 0.6 (earliest is 0.10), so the document's set cannot resolve at all |
| `anyrender = "0.2"` | `0.14` | **0.2 was never published** (earliest is 0.10) |
| `anyrender_vello = "0.2"` | `0.15` | 0.15 requires `anyrender ^0.14`, `vello ^0.11`, `wgpu ^30` |
| `wgpu = "24"` | 30 (transitive) | Pinned by `anyrender_vello`; adding a direct 24 would compile two wgpu copies |
| `winit = "0.30"` | `0.30.13` | Unchanged |

### API drift

* `paint_scene` takes seven arguments now — `(painter, &mut BaseDocument, scale, w, h, x_offset, y_offset)`.
* `WindowRenderer::resume` takes `Arc<dyn WindowHandle>` plus an `on_ready`
  callback, and activation is completed by a separate `complete_resume()`.
* `_sh_unit_init_guarded` takes three arguments `(runtime, creator, out-param)`,
  not two.
* `shermes` emits the unit creator as `SHUnit *sh_export_dom_runtime(void)` via a
  `CREATE_THIS_UNIT` macro, so the symbol is not greppable in the generated C.

### Hard integration constraints discovered by running it

1. **A `c_ptr` cannot cross JSI.** `c_ptr` is extracted with
   `_sh_ljs_get_native_pointer`, which reinterprets the raw `SHLegacyValue`; a
   pointer arriving as a JSI `double` would be read from the double's bit pattern.
   The document's `__initBlitzContext(ctx)` design therefore cannot work. The
   context stays in the shim (`g_app->blitz_ctx`) and the JS-visible functions
   (`blitz_set_style_property` and friends, per the document's naming) take no
   context argument; the Rust implementations are `blitz_dom_*` and take it
   explicitly.
2. **`{declared: true}` does not emit a declaration.** The generated C calls the
   symbol with no prototype, which is an error in C99+. Declared FFI functions
   use `{include: "blitz_bridge.h"}` instead, and `build.rs` puts `c_shim/` on the
   include path.
3. **The generated C is not ISO C, and not debug-compatible.** It needs
   `-std=gnu11` (aarch64 `asm` statement expressions in `sh_tryfast_fp_cvt.h`)
   and `-DNDEBUG` (`_SH_MODEL()` expands to `_sh_model_s22_p8_rel` only in
   release builds; without it the link fails on the `_dbg` symbol).
4. **`set_node_text` only accepts text nodes.** On an element it returns early —
   upstream marks element `textContent` a TODO. `blitz_dom_set_text` therefore
   resolves the element's text child and creates one if missing, which is what
   `innerText` means.
5. **`set_style_property` does not rewrite the `style` attribute.** It mutates the
   parsed declaration block, so assertions must read the computed value
   (`resolved_style_value`), not `attr("style")`.
6. **Typed mode rejects setters and nullable narrowing.** Setters need
   `@Hermes.final`; a `T | null` binding cannot be cast to `T`. The runtime keeps
   node ids as `c_size_t` (0 = absent) and uses methods rather than properties.
7. **Teardown order matters.** `_sh_done` must run after every `jsi::Value` is
   dropped; otherwise destruction walks freed handles and segfaults at exit.

### Constraints found while building the chrome

8. **The toolbar cannot use icon glyphs.** A font-coverage probe
   (`examples/glyphs.rs`) rendered every candidate: `←` (U+2190), `→` (U+2192),
   `‹`, `›`, `×` resolve, but **every refresh glyph** — U+21BB, U+27F3, U+21BA,
   U+27F2 — renders as tofu in the Optimistic/Montserrat/Helvetica chain. The
   icons are therefore drawn from borders: back/forward are a 9px square with two
   borders rotated 45°/225°, and reload is a 15px ring with its top border
   transparent plus a border-triangle arrowhead.
9. **`blitz-net` requires a multi-threaded Tokio runtime, entered at parse
   time.** Its `NetProvider::fetch` calls `tokio::spawn`; a `current_thread`
   runtime or no runtime at all panics with "there is no reactor running". The
   runtime has two worker threads, and document construction runs inside
   `runtime().enter()`.
10. **`resolve()` can spawn fetches too.** Font and image loads are kicked off
    during layout resolution, so `Page::resolve` enters the runtime context as
    well. Without this, a page with a webfont panics on its second resolve.
11. **A dropdown cannot live inside a short document.** The chrome document was
    the height of the strip, so `overflow: hidden` clipped the menu to a few
    pixels. The chrome now spans the whole surface with a transparent body and
    is painted *after* the page, so the menu overlays content; clicks that fall
    through to the body are routed to the page.
12. **Blitz's UA stylesheet hard-codes `color: black`.** A page that declares
    `color-scheme: light dark` and leaves its text colour to the UA renders
    black text on its own dark background (example.com does exactly this). When
    the theme is dark, the document is rebuilt with an extra UA sheet setting
    `html, body { color: #F2F2F7 }` — but only if the page's computed
    `color-scheme` actually contains `dark`, so light pages with no explicit
    colour are untouched.

One deliberate difference from the reference: its screenshots show the active
tab and toolbar tinted to the *page's* colour (light, because the page in that
shot was a white JSON view). That reads well when the page is light and badly
when it is not, so the chrome here uses a fixed palette per theme and lets the
page supply its own colours.

### Why the Rust host, not the C++/Fabric one

The document offers two compositions. The C++ one (`Dawn + Skia Graphite + ImGui +
Fabric`) is not buildable as written:

* two of its own calls are commented out — `rnwgpu::installWebGPUBindings(...)`
  and `fabricMountManager.paintTree(domCanvas)`;
* it supplies a `SchedulerDelegate` but no Fabric `Scheduler`/`ShadowTree`
  bootstrap, no `ComponentDescriptorRegistry`, and no `TextLayoutManager`
  implementation to compile;
* the document itself lists the HTML/CSS parser and per-tab isolation as "what you
  still have to write";
* it depends on prebuilt `libwebgpu_dawn` / `libskia` Graphite artifacts taken
  from React Native release artifacts, which ship mobile binaries; a macOS
  desktop Dawn + Skia Graphite build would have to be produced from source with
  depot_tools (the document estimates 40+ minutes for that step alone).

The Rust composition is complete end to end and was built and executed here. The
C++ path remains the better route if Fabric's `ShadowView` mutation semantics are
required, but it is a larger project than this document's snippets imply.

## Agent handoff

This browser is the destination of a handoff from Muse's own browser. The
contract for that — control transfer, typed proposals, decision binding,
conformance tests — is specified in
`walkaway-hackathon/docs/HANDOFF_SPEC.md`. The browser-side implementation
companion is [`docs/supervision-interface.md`](docs/supervision-interface.md):
supervisor/engine IDL, Take control / Stop / Done bar contract, snapshot and
frame APIs, credential/passkey brokers, required custom browser APIs, and
acceptance tests. Phase 2 landed 2026-11-04: scoped per-tab Hermes
activation, stable tab IDs/document generations, a native element snapshot
(`--observe`, `--observetest`), an authenticated Unix-socket NDJSON control
API with separate host/agent tokens, the session/supervision state machine
with fingerprint-bound propose/decide/execute, an in-window supervision bar
(Approve/Reject/Take control/Done/Stop), screenshots, and audit records.
Run it with `--control-socket <path>` plus `--host-token-file` and
`--agent-token-file` (or the `HERMES_HOST_TOKEN`/`HERMES_AGENT_TOKEN`
environment variables); `--supervisortest` covers the protocol end to end.
Page text input, forms, credential brokers, and remote JavaScript remain
unimplemented and are reported as unsupported capabilities. An
unimplemented protocol in the tree is worse than a written one, and the spec
has open questions that need the shell team's answers before the shape is
fixed.

## What is deliberately not here

* **Remote JavaScript is not executed.** Pages render from the HTML and CSS the
  server sends; `<script>` is ignored. Server-rendered sites (example.com,
  rust-lang.org) render correctly; client-rendered sites (ai.meta.com/muse among
  them) come up empty. The Hermes runtime in this build executes only the
  AOT-compiled `dom_runtime.ts`, not page scripts — wiring page JS would need a
  DOM/`window` surface far larger than the six bridge functions.
* No bookmarks, no history UI beyond back/forward, no view-source, no downloads.
* Tabs have no drag-reorder, no tab overflow menu, and no session restore.
* Favicons are not decoded — tabs show a generated initial instead.
* No HTML parser beyond `blitz-html` (html5ever); no caching layer.
* No `<canvas>`, no SVG beyond what `blitz-paint` handles, no video.
* The address bar edits by append/backspace only — no caret placement, selection,
  or clipboard.
