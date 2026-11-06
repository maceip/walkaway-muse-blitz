# Polish Browser Chrome to Match IE5 Macintosh Edition Aqua Explorer Bar, Scrapbook, and Page Holder

Written against: `f91d457` (`/Users/mac/walkaway-monorepo`)

## Evidence chain

- Surface: `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/chrome.rs` (`chrome::render`, rendered into `BrowserApp::chrome_doc` and hit-tested via `BrowserApp::handle_chrome_click` in `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/main.rs`)
- Problem:
  1. **Explorer Bar vertical tab labels are wrong and horizontal instead of sideways (90° rotated)**: `src/chrome.rs:342-346` renders truncated 4-character strings (`Fav`, `Hist`, `Srch`, `Scrp`, `Hold`) horizontally inside `24px × 46px` pills instead of the full vertical sideways labels (`Favorites`, `History`, `Search`, `Scrapbook`, `Page Holder`) rotated 90° clockwise down the left Explorer Bar rail as shown in PDF Pages 2 and 3.
  2. **Collapse toggle and collapsed rail controls do not match IE5 Mac iconography**: `src/chrome.rs:301-331` uses plain text `<`, `>`, `×`, `↻`, `▴`, `▾` instead of the IE5 Mac `◀|||` / `|||▶` collapse bar button and color-coded Aqua compact icons (green Back arrow, green Forward arrow, red X Stop, blue circular Refresh) shown in PDF Page 2 (left vs. right comparison).
  3. **Top toolbar lacks the IE5 Mac Aqua multi-row structure and icon+label buttons**: `src/chrome.rs:267-298` places unlabelled circular chevrons on the same row as a rounded pill address bar, omitting the Aqua pinstripe button bar (`Back`, `Forward`, `Stop`, `Refresh`, `AutoFill`, `Print`, `Sherlock` with icons above text labels), the dedicated `Address` bar row with its blue-tinted `Address` badge and `@` page icon, the horizontal Favorites strip (`@ Web Gallery`, etc.), and the bottom `Internet zone` status bar shown in PDF Pages 1–3.
  4. **Internet Scrapbook (`#exp-scrapbook`) and Page Holder (`#exp-pageholder`) drawers do not match PDF Figure 6 and Page 2**:
     - `src/chrome.rs:448-456` renders a 1-line `Scrapbook` title, orders buttons `Add`, `Organize`, `Delete` (with red `#D70015` text on `Delete`), and renders multi-line card boxes instead of Figure 6's two-line blue-teal `Internet` / `Scrapbook` header, beveled Mac buttons ordered `Add`, `Delete`, `Organize ▸`, and compact single-line rows with a snapshot icon.
     - `src/chrome.rs:358-377` renders Page Holder links (`#ph-link-N`) as multi-line cards instead of the compact `▸ <underlined blue link>` list shown at the bottom of PDF Page 2 (right screenshot).
- Design evidence:
  - `/Users/mac/.gemini/antigravity/brain/fe8b99f7-f96f-4bb3-b19e-b16e765f2dbb/.user_uploaded/media_1793981185434_0d9bff7a.pdf` (Page 1 Figure 6: Internet Scrapbook; Page 2: Page Holder and Collapsible Toolbar before/after; Page 3: IE5 Macintosh Edition Aqua toolbar and left Explorer Bar rail).
  - `/Users/mac/walkaway-monorepo/apps/hermes-browser/herbarium.md` (Sections 4, 5, and 6).
- Owner: `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/chrome.rs`
- Scope and affected surfaces:
  - `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/chrome.rs`
  - `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/main.rs` (only if `CHROME_HEIGHT` or `#btn-autofill` click routing is adjusted; all existing test IDs must remain intact)
- Uncertainty: None. Both `blitz-dom` (`src/node/node.rs:425`, `resolve_2d_transform`) and `blitz-paint` (`src/render.rs:863-898`, `stroke_text` with `self.transform`, and `src/render.rs:565`, `draw_svg` via `anyrender_svg`/`usvg`) natively support 2D CSS `transform: rotate(90deg)` on text nodes and inline `<svg>` icons, and `dom_ffi::bubble_target` (`src/dom_ffi.rs:803-817`) walks up from any hit child node (`<span>`, `<svg>`, `<path>`) to the nearest ancestor carrying an `id` attribute.

## Design decision

Restyle `chrome::render` in `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/chrome.rs` to faithfully reproduce the IE5 Macintosh Edition Aqua interface shown in the reference PDF while preserving all existing element `id` contracts (`#btn-collapse-toolbar`, `#exp-favorites`, `#exp-history`, `#exp-search`, `#exp-scrapbook`, `#exp-pageholder`, `#ph-add`, `#ph-links`, `#ph-link-{i}`, `#ph-canvas`, `#sb-add`, `#sb-delete`, `#sb-organize`, `#sb-entry-{i}`, `#btn-back`, `#btn-forward`, `#btn-stop`, `#btn-reload`, `#address`, `#tab-{i}`, `#tab-close-{i}`, `#btn-newtab`) and viewport geometry constants (`CHROME_HEIGHT = 88.0`, `EXPLORER_RAIL_WIDTH = 28.0`, `EXPLORER_DRAWER_WIDTH = 260.0`, `PH_HEADER_HEIGHT = 38.0`).

## Reuse

- `dom_ffi::bubble_target` (`/Users/mac/walkaway-monorepo/apps/hermes-browser/src/dom_ffi.rs:803-817`): Automatically bubbles hits on inner rotated `<span>` or `<svg>` elements inside `.exp-tab` or `.btn` up to the outer element bearing `id="..."`, so child elements must never carry their own `id` attributes.
- `CHROME_HEIGHT`, `EXPLORER_RAIL_WIDTH`, `EXPLORER_DRAWER_WIDTH`, `PH_HEADER_HEIGHT` (`/Users/mac/walkaway-monorepo/apps/hermes-browser/src/chrome.rs:204-210`): Keep these four public geometry constants unchanged so all layout/compositing math and E2E coordinate assertions in `src/main.rs` continue to pass without coordinate drift.
- Exemplar: `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/chrome.rs:212-1081`

## Changes

1. **Left Vertical Explorer Bar Rail (`#exp-rail`) — Sideways 90° Rotated Full Tab Labels & Collapse Bar (`src/chrome.rs:300-349`, `780-842`)**
   - Change:
     - Replace the abbreviated horizontal labels (`Fav`, `Hist`, `Srch`, `Scrp`, `Hold`) with the exact full labels from PDF Pages 2 & 3 wrapped in a rotated inner `<span class="exp-tab-label">` (with **no** `id` on the inner `<span>` so `bubble_target` resolves to `#exp-*`):
       ```html
       <div class="{fav_cls}" id="exp-favorites"><span class="exp-tab-label">Favorites</span></div>
       <div class="{hist_cls}" id="exp-history"><span class="exp-tab-label">History</span></div>
       <div class="{srch_cls}" id="exp-search"><span class="exp-tab-label">Search</span></div>
       <div class="{scrp_cls}" id="exp-scrapbook"><span class="exp-tab-label">Scrapbook</span></div>
       <div class="{ph_cls}" id="exp-pageholder"><span class="exp-tab-label">Page Holder</span></div>
       ```
     - Style `.exp-tab` as a tall IE5 Mac beveled vertical folder tab (`width: 24px; height: 78px; margin-left: 2px; position: relative; display: flex; align-items: center; justify-content: center; overflow: visible; background-color: #DCDCDC; border-top: 1px solid #FFFFFF; border-left: 1px solid #FFFFFF; border-right: 1px solid #7F7F7F; border-bottom: 1px solid #7F7F7F; border-radius: 0 4px 4px 0;`) and `.exp-tab-active` with bright Aqua highlight (`background-color: #F4F7FB; border-right: 2px solid #2B6CB0;`).
     - Style `.exp-tab-label` so Blitz rotates the text 90° clockwise (reading top-to-bottom down the left rail, matching PDF Pages 2 & 3):
       ```css
       .exp-tab-label {
         display: block;
         width: 72px;
         height: 14px;
         line-height: 14px;
         font-size: 10px;
         font-weight: 700;
         color: #111111;
         text-align: center;
         white-space: nowrap;
         transform: rotate(90deg);
       }
       ```
     - Style `#btn-collapse-toolbar` (`.rail-collapse`) at the top of `#exp-rail` (`width: 24px; height: 18px;`) to display the IE5 Mac triangle-and-bars icon: `&#9664;&#124;&#124;` (`◀||`) when expanded and `&#124;&#124;&#9654;` (`||▶`) when `toolbar_collapsed == true`.
     - When `toolbar_collapsed == true`, style the four compact rail buttons (`#btn-back`, `#btn-forward`, `#btn-stop`, `#btn-reload`) at the top of `#exp-rail` with the PDF Page 2 (right) Aqua color-coded glyphs:
       - `#btn-back`: green left arrow (`color: #1E824C;` when enabled, `#9E9E9E` when disabled)
       - `#btn-forward`: green right arrow (`color: #1E824C;` when enabled, `#9E9E9E` when disabled)
       - `#btn-stop`: red `✕` (`color: #C0392B;` when loading, `#9E9E9E` when idle)
       - `#btn-reload`: Aqua blue circular refresh arrow (`color: #1B6EC2;`)
   - Preserve: `#exp-rail`, `#btn-collapse-toolbar`, `#exp-favorites`, `#exp-history`, `#exp-search`, `#exp-scrapbook`, `#exp-pageholder`, and `EXPLORER_RAIL_WIDTH = 28.0`.
   - Verify: Opening the browser or capturing a PNG shows `Favorites`, `History`, `Search`, `Scrapbook`, and `Page Holder` rendered vertically (rotated 90° clockwise) down the left rail, and clicking each tab still toggles its drawer in `--uitest`.

2. **Top Aqua Toolbars: Icon+Label Button Row, Dedicated `Address` Bar Row, and Favorites Strip (`src/chrome.rs:267-298`, `669-779`)**
   - Change:
     - Within the `88px` top chrome height (`CHROME_HEIGHT = 88.0`), structure `top_bars` into the classic IE5 Mac Aqua rows over a subtle Aqua pinstripe surface (`background-color: #E8E8E8;`):
       1. **Row 1 (`22px` height): Window/Document Tab Strip (`.tabstrip`)** with classic beveled Aqua tabs (`#tab-{i}`, `#tab-close-{i}`, `#btn-newtab`).
       2. **Row 2 (`38px` height): IE5 Mac Custom Button Bar (`.nav-buttons`)** containing icon-over-label Aqua toolbar buttons:
          - `#btn-back`: green left arrow icon on top + `Back` label (`font-size: 9px`) underneath
          - `#btn-forward`: green right arrow icon on top + `Forward` label underneath
          - `#btn-stop`: red `✕` icon on top + `Stop` label underneath
          - `#btn-reload`: blue circular arrows icon on top + `Refresh` label underneath
          - `#btn-autofill`: form/pencil icon on top + `AutoFill` label underneath
          - `#btn-print`: printer icon on top + `Print` label underneath
          - `#btn-menu`: globe/magnifying glass icon on top + `Sherlock` label underneath
       3. **Row 3 (`28px` height): Dedicated `Address` & Favorites Row (`.address-row`)**:
          - Left-hand recessed Aqua blue-teal `Address` bevel badge (`background-color: #6B94B5; color: #FFFFFF; font-size: 11px; font-weight: 700; padding: 2px 8px; border: 1px solid #3A5F7D;`) matching PDF Pages 1–3.
          - Sunken white rectangular address input box (`#address`, `background-color: #FFFFFF; border-top: 1px solid #666666; border-left: 1px solid #666666; border-right: 1px solid #CCCCCC; border-bottom: 1px solid #CCCCCC; height: 22px;`) containing an `@` globe badge, padlock/security indicator, URL text (`.url`), and focus caret.
          - Inline quick-favorites pills on the right of the row (`@ Web Gallery`, `@ Office for Macintosh`, or the first 2–3 entries of `state.favorites` mapped to `#fav-bar-0`, `#fav-bar-1`).
   - Preserve: `CHROME_HEIGHT = 88.0`, `#tab-{i}`, `#tab-close-{i}`, `#btn-newtab`, `#btn-back`, `#btn-forward`, `#btn-stop`, `#btn-reload`, `#address`, `#btn-menu`, and `#sup-bar`.
   - Verify: Top chrome height remains `88.0` CSS px (`0.0` when `toolbar_collapsed == true`), and all toolbar buttons show both their Aqua icon and text label (`Back`, `Forward`, `Stop`, `Refresh`, `AutoFill`, `Print`, `Sherlock`) above the `Address` bar.

3. **Internet Scrapbook Drawer (`#exp-scrapbook`) — Match PDF Figure 6 (`src/chrome.rs:404-458`, `843-936`)**
   - Change:
     - In `Some(ExplorerTab::Scrapbook)`, render the header (`#sb-header`) matching PDF Page 1 Figure 6:
       - Left title (`.sb-brand`): two stacked lines `Internet` / `Scrapbook` in bold blue-teal (`color: #1F5F8B; font-size: 11px; line-height: 11px; font-weight: 700;`).
       - Three classic beveled rectangular Mac OS buttons in the exact order shown in Figure 6:
         1. `<div class="mac-btn" id="sb-add">Add</div>`
         2. `<div class="{delete_btn_cls}" id="sb-delete">Delete</div>` (where `delete_btn_cls` uses muted `.mac-btn .mac-btn-disabled` styling when no item in `state.scrapbook_items` has `selected == true`, and standard `.mac-btn` when an item is selected—never red text)
         3. `<div class="mac-btn" id="sb-organize">Organize &#9656;</div>`
     - Below a subtle grey sub-divider bar, render `#sb-list` with a clean white background (`background-color: #FFFFFF;`) and compact single-line rows (`height: 22px; border-bottom: 1px solid #EAEAEA; display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 0 6px;`) where each `#sb-entry-{i}` displays a small snapshot page/camera badge (`[📷]` or folder icon for dividers) followed by the single-line entry title (`font-size: 11px; color: #111111; white-space: nowrap; overflow: hidden;`).
   - Preserve: `#sb-header`, `#sb-add`, `#sb-organize`, `#sb-delete`, `#sb-list`, `#sb-empty`, `#sb-entry-{i}`, and `data-sb-id`.
   - Verify: Opening the Scrapbook drawer (`#exp-scrapbook`) matches Figure 6 (`Internet Scrapbook` stacked title, `Add` | `Delete` | `Organize ▸` beveled buttons, and compact icon+title rows).

4. **Page Holder Drawer (`#exp-pageholder`) — Match PDF Page 2 (`src/chrome.rs:352-403`)**
   - Change:
     - In `Some(ExplorerTab::PageHolder)`, style `#ph-header` with stacked `Page` / `Holder` blue-teal title (`color: #1F5F8B; font-size: 11px; font-weight: 700;`) and classic beveled `.mac-btn` controls for `Add` (`#ph-add`) and `Links` (`#ph-links`).
     - When `state.page_holder.links_mode` is active (`#ph-links-list`), render each `#ph-link-{i}` as a compact IE5 Mac link row (`padding: 3px 6px; display: flex; flex-direction: row; align-items: center; gap: 4px; background-color: #FFFFFF; border-bottom: 1px solid #F0F0F0;`) with a blue right-pointing triangle (`&#9656;`, `color: #1F5F8B;`) and underlined blue hyperlink text (`color: #0000EE; text-decoration: underline; font-size: 11px;`) matching the bottom-left list in PDF Page 2 (right screenshot).
   - Preserve: `#ph-header`, `#ph-add`, `#ph-links`, `#ph-links-list`, `#ph-link-{i}`, `#ph-canvas`, `#ph-empty`, `#ph-empty-links`, and `PH_HEADER_HEIGHT = 38.0`.
   - Verify: Toggling `#ph-links` displays the compact `▸ link` list, and clicking any `#ph-link-{i}` or rendered link in `#ph-canvas` retargets the main tab viewport.

## Scope

- Inherit: `BrowserApp` window rendering, `capture_png` offscreen Vello screenshots, and all Explorer Bar / Page Holder / Scrapbook modes in `/Users/mac/walkaway-monorepo/apps/hermes-browser`.
- Verify:
  - `/Users/mac/walkaway-monorepo/apps/hermes-browser/src/main.rs` (`run_uitest`, `run_selftest`, `run_supervisortest`, `handle_chrome_click`)
- Exclude:
  - Static Hermes FFI (`src/dom_ffi.rs`, `src/js/dom_runtime.ts`, `c_shim/hermes_shim.cpp`)
  - WebAuthn / Keychain (`src/webauthn/`, `src/credentials.rs`, `src/keychain.rs`)
  - Offline Scrapbook serializer (`src/scrapbook.rs`)

## Validation

- Product: Build and launch `hermes-blitz-browser`, inspect the top Aqua button bar, `Address` row, sideways-rotated Explorer Bar tab labels (`Favorites`, `History`, `Search`, `Scrapbook`, `Page Holder`), collapsed rail mode, Page Holder drawer, and Internet Scrapbook drawer against `media_1793981185434_0d9bff7a.pdf`.
- Interface:
  - Expanded vs. collapsed toolbar (`#btn-collapse-toolbar`)
  - Closed drawer vs. open `Scrapbook` (`#exp-scrapbook`) and open `Page Holder` (`#exp-pageholder` in both rendered and `Links` view modes)
  - Light and Dark theme palettes
- System: Confirm no inner child element inside `#exp-*`, `#btn-*`, `#sb-*`, or `#ph-*` defines an `id` attribute that would intercept `dom_ffi::bubble_target`.
- Repository:
  - `HERMES_HIDDEN_WINDOW=1 cargo test --release --manifest-path /Users/mac/walkaway-monorepo/apps/hermes-browser/Cargo.toml` → all unit and integration tests pass (`--selftest`, `--runtimetest`, `--uitest`, `--observetest`, `--supervisortest`, `--passkeytest`).

## Stop conditions

- Stop if any change alters `CHROME_HEIGHT` (`88.0`), `EXPLORER_RAIL_WIDTH` (`28.0`), `EXPLORER_DRAWER_WIDTH` (`260.0`), or `PH_HEADER_HEIGHT` (`38.0`), as downstream viewport clipping and hit-test coordinates in `src/main.rs` depend on those exact values.
- Stop if any child element inside a clickable chrome control is given an `id` attribute, which would break `dom_ffi::bubble_target` hit resolution.

## Design documentation

- After acceptance and validation: Update Section 4 of `/Users/mac/walkaway-monorepo/apps/hermes-browser/herbarium.md` to note the 90° clockwise rotated Explorer Bar tab labels (`.exp-tab-label { transform: rotate(90deg); }`) and the IE5 Mac Aqua toolbar rows.
