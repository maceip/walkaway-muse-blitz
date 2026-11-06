# Herbarium: Pure-Rust Static Hermes + Blitz + Vello Browser Architecture

This document defines the target architecture for `hermes-blitz-browser`: a standalone, single-process native browser built exclusively on **Rust (`winit` + `wgpu` + `anyrender_vello` + `blitz-dom` / `blitz-html` / `blitz-paint` / `blitz-net`)** embedding **Static Hermes (`static_h`)**.

No C++ Dawn, Skia Graphite, React Native Fabric/Yoga, or Dear ImGui layers are used. Both the browser chrome (`chrome_doc`) and web page viewports (`content_doc` and `page_holder_doc`) are parsed, styled, laid out, hit-tested, and painted by the same Rust Blitz + Stylo + Taffy + Parley + Vello WebGPU compute pipeline.

---

## 1. Strict Layer Assignment

| Browser Layer | Technology / Crate | Responsibility |
| :--- | :--- | :--- |
| **Windowing, Input & Event Loop** | `winit` (`ApplicationHandler`) | Native OS window lifecycle, physical/logical HiDPI scaling, keyboard/mouse/scroll input routing, and async wakeup events (`EventLoopProxy<UserEvent>`). |
| **GPU Compute Vector Renderer** | `wgpu` + `anyrender_vello` + `blitz-paint` | Single shared `wgpu` device and swapchain surface. Renders `content_doc`, `page_holder_doc`, and `chrome_doc` layers via Vello GPU compute shaders and offscreen `VelloImageRenderer` PNG capture. |
| **HTML5, CSS3 & Layout Engine** | `blitz-html` + `blitz-dom` (`html5ever`, `Stylo`, `Taffy`, `Parley`) | Spec-compliant HTML5 parsing, Servo Stylo CSS selector cascade & media queries, Taffy CSS Flexbox/Grid layout, Parley font shaping, and sub-microsecond spatial hit-testing (`doc.hit(x, y)`). |
| **Browser Chrome & IE5 Mac Explorer Bar** | `src/chrome.rs` (`chrome_doc`) + `BrowserApp::handle_chrome_click` | Pure-Rust HTML/CSS overlay document rendered at `(0, 0)` with a transparent body over the page viewport. Hit-tested directly in Rust with zero JS runtime attached to `chrome_doc`. |
| **JS Engine & Dual-Mode DOM Pipeline** | `facebook/hermes` (`static_h`: `shermes` + `libhermesvm` + `libjsi`) | Ahead-of-time compiled TypeScript DOM runtime (`src/js/dom_runtime.ts` -> C via `shermes -typed -exported-unit=dom_runtime -emit-c`) calling Rust `#[no_mangle] pub extern "C"` DOM functions via `$SHBuiltin.extern_c`, paired with `facebook::jsi::Runtime` to evaluate page `<script>` tags. |
| **Networking & Subresource Loader** | `blitz-net` + `tokio` + `reqwest` | Multi-threaded async fetching for documents, `<head>` critical stylesheets, `<script src="...">`, images, and HTTP `GET`/`POST` form submissions. |

---

## 2. Project Layout

```text
hermes-blitz-browser/
├── Cargo.toml
├── build.rs                 # Invokes shermes -typed -emit-c and compiles hermes_shim.cpp
├── c_shim/
│   ├── blitz_bridge.h       # C ABI declarations shared between shermes C output and hermes_shim.cpp
│   └── hermes_shim.cpp      # C++20 boundary owning SHRuntime, ScopedCurrentApp, and JSI try/catch guards
└── src/
    ├── js/
    │   └── dom_runtime.ts   # AOT-compiled by shermes into native C ($SHBuiltin.extern_c DOM bindings)
    ├── browser.rs           # URL normalization, per-tab back/forward history, and async HTTP fetch
    ├── chrome.rs            # Top chrome + IE5 Mac Collapsible Toolbar + Left Explorer Bar rail & drawers
    ├── control.rs           # Supervised Host/Agent control protocol
    ├── dom_ffi.rs           # #[no_mangle] extern "C" Blitz DOM query, mutation, layout & serialization FFI
    ├── observe.rs           # Native DOM/layout observation & SHA-256 structural fingerprinting
    ├── page_endpoint.rs     # Page command channel
    ├── recall.rs            # Structural FormSignature & zero-LLM FormRecipe replay
    ├── scrapbook.rs         # Persistent offline DOM + inline style + data: URI image snapshot archive
    ├── vault.rs             # Blind credential vault & NIST P-256 ECDSA WebAuthn assertion signing
    └── main.rs              # Winit + Vello + Blitz + Static Hermes multi-tab host & E2E test suites
```

---

## 3. Deliverable 1: Static Hermes + Blitz DOM Pipeline (`$SHBuiltin.extern_c` + JSI)

### 3.1 Complete `$SHBuiltin.extern_c` DOM Bindings (`src/dom_ffi.rs` & `src/js/dom_runtime.ts`)
Because Static Hermes (`static_h`) NaN-boxes `SHLegacyValue`, raw 64-bit native pointers must not cross the JS/JSI boundary as IEEE-754 doubles. Instead, `c_shim/hermes_shim.cpp` binds the active tab's `BlitzHostContext*` in a scoped thread-local/static pointer (`ScopedCurrentApp`) for the duration of every FFI call, while JS holds integer `NodeId` handles (`uint64_t` / `c_size_t`).

The AOT-compiled `src/js/dom_runtime.ts` exposes the W3C DOM API on `globalThis.document` and `BlitzElement` backed by direct `$SHBuiltin.extern_c` calls into `src/dom_ffi.rs`:
- **Queries**:
  - `document.getElementById(id)` -> `blitz_find_element_by_id`
  - `document.querySelector(selector)` -> `blitz_query_selector` (supporting `#id`, `.class`, `tag`, `tag#id`, `tag.class`, `[attr]`, `[attr=value]`, and descendant combinators)
  - `document.querySelectorAll(selector)` -> `blitz_query_selector_all_count` + `blitz_query_selector_all_item`
- **Tree Construction & Mutation**:
  - `document.createElement(tag)` -> `blitz_create_element`
  - `element.appendChild(child)` -> `blitz_append_child`
  - `element.remove()` / `element.removeNode()` -> `blitz_remove_node`
  - `element.setAttribute(name, value)` / `element.getAttribute(name)` -> `blitz_set_attribute` / `blitz_get_attribute`
  - `element.style.setProperty(name, value)` -> `blitz_set_style_property`
  - `element.innerText` / `element.textContent` (getter & setter) -> `blitz_get_inner_text` / `blitz_set_inner_text`
  - `element.innerHTML` (getter & setter) -> `blitz_get_inner_html` / `blitz_set_inner_html` (via `DocumentMutator::set_inner_html` and `blitz_html::HtmlProvider`)
- **Events & Bubbling**:
  - `element.addEventListener(type, listener)` registers listeners keyed by `(nodeId, eventType)`.
  - When an event is dispatched via `hermes_app_dispatch_event(hermes, type, hit_node_id, x, y)`, `__dispatchBlitzEvent` walks up the ancestor chain using `blitz_parent_node(current)` from the hit target to the document root, invoking listeners on each ancestor until `event.stopPropagation()` is called.
- **Layout Geometry (`getBoundingClientRect`)**:
  - `element.getBoundingClientRect()` calls `blitz_get_bounding_rect_x`, `blitz_get_bounding_rect_y`, `blitz_get_bounding_rect_w`, and `blitz_get_bounding_rect_h` (which read Taffy's resolved `doc.get_client_bounding_rect(node_id)`), returning `{ x, y, width, height, top, left, right, bottom }`.

### 3.2 Automatic `<script>` Execution in Document Order
When a tab loads or navigates to a document (`Page::load`):
1. `HtmlDocument::from_html` parses the HTML (with `HtmlProvider` enabled so dynamic `innerHTML` mutations also parse full HTML fragments) and resolves initial styles/layout.
2. `Page::load` walks the DOM tree in document order for `<script>` elements whose `type` is empty, `text/javascript`, `application/javascript`, or `module`:
   - Inline `<script>` text content is evaluated directly in the tab's isolated `HermesAppRuntime` via `hermes_app_eval_script`.
   - External `<script src="...">` URLs are resolved against the document's base URL, fetched via `browser::fetch`, and evaluated in document order.
3. All evaluations run inside `try / catch (const facebook::jsi::JSIException&)` in `c_shim/hermes_shim.cpp`, draining microtasks (`rt.drainMicrotasks()`) after each script and re-resolving Stylo/Taffy layout (`doc.resolve(0.0)`) when mutations occur (`ctx.dirty == true`).

### 3.3 Multi-Tab Isolation & Viewport Compositing
- Each `Tab` owns an isolated `CanonicalTab` (`TabId`, `Browser` with `history`, `index`, `address`) and `PageInstance` (`Page` owning `Box<BlitzHostContext>` and `*mut HermesAppRuntime`).
- In `render()` / `render_to_png()` / `capture_png()`:
  1. The active tab's `content_doc` is painted at `(content_x, content_y)` with size `(content_w, content_h)` corresponding to the area right of the Explorer Bar and below the top chrome.
  2. When the Explorer Bar's **Page Holder** drawer is open in Rendered Page View, `page_holder_doc` is painted into its own clipped viewport region `(EXPLORER_RAIL_WIDTH, content_y + PH_HEADER_HEIGHT)` of width `EXPLORER_DRAWER_WIDTH`.
  3. `chrome_doc` is painted on top at `(0, 0)` across the full surface.

---

## 4. Deliverable 2: IE5 Mac Collapsible Explorer Bar (`src/chrome.rs`)

Inspired by Internet Explorer 5 Macintosh Edition:
- **Left Vertical Explorer Bar Rail (`EXPLORER_RAIL_WIDTH = 28px`)**:
  - Runs down the left edge of the window below the top chrome (or from `y = 0` when the toolbar is collapsed).
  - Contains the collapsible toolbar toggle button (`#btn-collapse-toolbar`) and five vertical Explorer Bar tab buttons:
    - `#exp-favorites` (*Favorites*)
    - `#exp-history` (*History*)
    - `#exp-search` (*Search*)
    - `#exp-scrapbook` (*Scrapbook*)
    - `#exp-pageholder` (*Page Holder*)
- **Collapsible Toolbar Mode (`toolbar_collapsed: bool`)**:
  - Clicking `#btn-collapse-toolbar` toggles `toolbar_collapsed`.
  - When `toolbar_collapsed == true`, the top tab strip and address/navigation bar are hidden (`content_y = 0`), expanding vertical viewing space to the full window height.
  - The four core navigation controls (`#btn-back`, `#btn-forward`, `#btn-stop`, `#btn-reload`) are rendered as compact icon buttons stacked at the top of the left vertical Explorer Bar rail above the five vertical tab buttons.
- **Explorer Bar Drawer (`EXPLORER_DRAWER_WIDTH = 260px`)**:
  - Clicking any of the five vertical tab buttons opens (or toggles closed if already active) a `260px`-wide drawer panel (`x = 28..288`).
  - Opening or closing the drawer shifts `content_x` between `28px` (rail only) and `288px` (`28 + 260`) and updates the active tab's `Viewport` width (`window_width - content_x`) so Taffy reflows the web page cleanly without horizontal clipping.

---

## 5. Deliverable 3: Page Holder (`#exp-pageholder`)

The Page Holder lets the user pin a reference page (such as a portal, directory, or search results page) in the left Explorer Bar drawer and click through its links to navigate the main browser viewport without losing the held page:
- **Header Controls in `chrome_doc`**:
  - `#ph-add` (**Add**): Copies/snapshots the active tab's current HTML and base URL into `BrowserApp::page_holder` (`page_holder_doc: HtmlDocument`, `base_url: Url`, `title: String`, `links: Vec<HeldLink>`).
  - `#ph-links` (**Links**): Toggles `page_holder_links_mode: bool` between **Rendered Page View** and **Links List View**.
- **Two View Modes**:
  1. **Rendered Page View (default)**: `page_holder_doc` is laid out at `Viewport::new(EXPLORER_DRAWER_WIDTH, drawer_body_height, scale, theme)` and painted directly inside the drawer via `blitz_paint::paint_scene`.
  2. **Links List View (`#ph-links` active)**: Walks `page_holder_doc` in Rust for all `<a href="...">` nodes, resolves their `href`s against `page_holder.base_url`, and renders clickable `#ph-link-0`, `#ph-link-1`, ... items in `chrome_doc`.
- **Link Retargeting**:
  - Clicking a link inside the Page Holder drawer—either by clicking a rendered `<a>` element inside `page_holder_doc` (hit-tested via `page_holder_doc.hit(local_x, local_y)`) or by clicking `#ph-link-N` in Links List View—navigates the **active main tab's `content_doc`** while keeping `page_holder_doc` pinned in the drawer.

---

## 6. Deliverable 4: Internet Scrapbook (`#exp-scrapbook`)

The Internet Scrapbook captures self-contained offline snapshots of live web pages—preserving DOM mutations, computed/inline styles, hypertext links, and embedded images—and persists them to disk:
- **Header Controls in `chrome_doc`**:
  - `#sb-add` (**Add**): Captures the active tab's live `content_doc` into a persistent `ScrapbookEntry`.
  - `#sb-delete` (**Delete**): Removes the selected Scrapbook entry and updates disk storage.
  - `#sb-organize` (**Organize**): Creates/toggles organizational folder dividers in the Scrapbook archive.
- **Complete Offline DOM + Image Serialization (`src/scrapbook.rs`)**:
  - Walks the live Blitz `BaseDocument` starting at `root_element()`, serializing every element's tag, current attributes (including `style` and `href` resolved against the page's base URL), and current text nodes (including any mutations made by Static Hermes or form fills).
  - For `<img src="...">` elements whose raster image data has been loaded into Blitz (`SpecialElementData::Image(ImageData::Raster(raster))`), encodes the RGBA buffer as a self-contained `data:image/png;base64,...` URI directly in the serialized `<img src="data:image/png;base64,...">`, so graphics render offline without network access.
  - Injects a `<base href="...">` tag and snapshot metadata header, and writes the archive to JSON disk storage (`HERMES_SCRAPBOOK_PATH` or `~/.local/share/hermes-blitz-browser/scrapbook.json`).
- **Instant Offline Restoration**:
  - Clicking any `#sb-entry-N` row in the Scrapbook drawer loads the archived HTML snapshot into the active tab's `content_doc` with its original URL context and hyperlinks intact.
