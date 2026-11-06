//! The browser chrome, rendered by the same HTML/CSS engine as the page.
//!
//! Faithfully implements the Internet Explorer 5 Macintosh Edition Aqua chrome
//! and Collapsible Explorer Bar shown in the reference specification:
//! - Multi-row Aqua top chrome (`CHROME_HEIGHT = 88.0`):
//!   1. Document tab strip (`#tab-N`, `#tab-close-N`, `#btn-newtab`)
//!   2. Custom Aqua icon-over-label button bar (`Back`, `Forward`, `Stop`,
//!      `Refresh`, `AutoFill`, `Print`, `Sherlock`)
//!   3. Dedicated `Address` bar row with blue-slate `Address` badge, `@` page
//!      swirl icon, and sunken white URL field (`#address`)
//!   4. Horizontal Favorites bar (`@ Web Gallery`, `@ Office for Macintosh`, ...)
//! - Left vertical Explorer Bar rail (`EXPLORER_RAIL_WIDTH = 28.0`) with
//!   `#btn-collapse-toolbar` (`◀|||` / `|||▶`) and five vertical folder tabs
//!   with 90°-rotated sideways labels (`Favorites`, `History`, `Search`,
//!   `Scrapbook`, `Page Holder`).
//! - Collapsible Toolbar Mode (`toolbar_collapsed`): hides the top toolbars so
//!   the web viewport expands to `y = 0`, placing compact color-coded Aqua
//!   `#btn-back`, `#btn-forward`, `#btn-stop`, and `#btn-reload` icon buttons at
//!   the top of the vertical Explorer Bar rail.
//! - Explorer Bar Drawer (`EXPLORER_DRAWER_WIDTH = 260.0`) supporting Internet
//!   Scrapbook (`#sb-add`, `#sb-delete`, `#sb-organize`, `#sb-entry-N`) and
//!   Page Holder (`#ph-add`, `#ph-links`, Rendered Page View & Links List View).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn is_dark(self) -> bool {
        matches!(self, Theme::Dark)
    }

    pub fn color_scheme(self) -> blitz_traits::shell::ColorScheme {
        match self {
            Theme::Light => blitz_traits::shell::ColorScheme::Light,
            Theme::Dark => blitz_traits::shell::ColorScheme::Dark,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExplorerTab {
    Favorites,
    History,
    Search,
    Scrapbook,
    PageHolder,
}

pub struct HeldLinkInfo<'a> {
    pub label: &'a str,
    pub url: &'a str,
}

pub struct PageHolderInfo<'a> {
    pub has_doc: bool,
    pub title: &'a str,
    pub url: &'a str,
    pub links_mode: bool,
    pub links: &'a [HeldLinkInfo<'a>],
}

pub struct ScrapbookItemInfo<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub url: &'a str,
    pub timestamp: &'a str,
    pub is_folder: bool,
    pub selected: bool,
}

pub struct NavEntryInfo<'a> {
    pub title: &'a str,
    pub url: &'a str,
}

struct Palette {
    strip: &'static str,
    tab_active: &'static str,
    nav: &'static str,
    hairline: &'static str,
    bevel_light: &'static str,
    bevel_dark: &'static str,
    pill: &'static str,
    pill_focus: &'static str,
    ink: &'static str,
    muted: &'static str,
    hover: &'static str,
    menu_bg: &'static str,
    separator: &'static str,
    favicon_bg: &'static str,
    favicon_ink: &'static str,
    accent: &'static str,
    aqua_badge: &'static str,
    aqua_badge_border: &'static str,
    rail_bg: &'static str,
    rail_tab_bg: &'static str,
    drawer_bg: &'static str,
    drawer_header_bg: &'static str,
    brand_teal: &'static str,
    link_blue: &'static str,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            strip: "#D6D6D6",
            tab_active: "#F2F2F2",
            nav: "#E6E6E6",
            hairline: "#8E8E8E",
            bevel_light: "#FFFFFF",
            bevel_dark: "#7A7A7A",
            pill: "#FFFFFF",
            pill_focus: "#FFFFFF",
            ink: "#111111",
            muted: "#8C8C8C",
            hover: "#DCDCDC",
            menu_bg: "#F4F4F4",
            separator: "#BEBEBE",
            favicon_bg: "#6B8FA8",
            favicon_ink: "#30485C",
            accent: "#2B6CB0",
            aqua_badge: "#6990B2",
            aqua_badge_border: "#345470",
            rail_bg: "#D4D4D4",
            rail_tab_bg: "#DFDFDF",
            drawer_bg: "#FFFFFF",
            drawer_header_bg: "#DDDDDD",
            brand_teal: "#1B5E85",
            link_blue: "#0000EE",
        },
        Theme::Dark => Palette {
            strip: "#1E1E20",
            tab_active: "#2E2E32",
            nav: "#262629",
            hairline: "#48484C",
            bevel_light: "#3E3E42",
            bevel_dark: "#141416",
            pill: "#1B1B1D",
            pill_focus: "#141416",
            ink: "#F2F2F7",
            muted: "#6E6E73",
            hover: "#36363A",
            menu_bg: "#2C2C2E",
            separator: "#3A3A3C",
            favicon_bg: "#3B5978",
            favicon_ink: "#C7D5E5",
            accent: "#0A84FF",
            aqua_badge: "#3B6285",
            aqua_badge_border: "#5882A8",
            rail_bg: "#222225",
            rail_tab_bg: "#2C2C30",
            drawer_bg: "#1C1C1E",
            drawer_header_bg: "#28282B",
            brand_teal: "#64B5F6",
            link_blue: "#5AC8FA",
        },
    }
}

pub struct TabInfo<'a> {
    pub title: &'a str,
    pub active: bool,
}

/// Supervision bar content, supplied from the same session state the control
/// protocol reports. Rendered only on the session's own tab.
pub struct SupervisionInfo<'a> {
    pub title: &'a str,
    pub state: &'a str,
    pub detail: &'a str,
    pub can_decide: bool,
    pub can_take: bool,
    pub can_release: bool,
    pub can_stop: bool,
}

/// One DOM bounding-box overlay rendered in the Semantic X-Ray layer.
#[derive(Clone, Debug)]
pub struct XRayBox {
    pub id: String,
    pub role_tag: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_vault_secret: bool,
    pub is_frozen_target: bool,
    pub freeze_fp_short: Option<String>,
}

/// Live-measured engine telemetry and X-Ray bounding boxes for the active tab.
#[derive(Clone, Debug)]
pub struct EngineTelemetry {
    pub layout_observe_ms: f64,
    pub rss_mb: f64,
    pub form_signature: Option<String>,
    pub matched_recipe_id: Option<String>,
    pub recalled_count: u64,
    pub overlays: Vec<XRayBox>,
}

pub struct ChromeState<'a> {
    pub url: &'a str,
    pub focused: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub loading: bool,
    pub tabs: &'a [TabInfo<'a>],
    pub menu_open: bool,
    /// True when the current URL is https — drives the padlock.
    pub secure: bool,
    /// Height of the whole surface in CSS pixels.
    pub surface_height: f64,
    pub theme: Theme,
    pub supervision: Option<SupervisionInfo<'a>>,
    pub telemetry: Option<EngineTelemetry>,
    /// IE5 Mac Collapsible Toolbar state.
    pub toolbar_collapsed: bool,
    /// Active IE5 Mac Explorer Bar drawer tab, if open.
    pub explorer_tab: Option<ExplorerTab>,
    /// Page Holder drawer state.
    pub page_holder: PageHolderInfo<'a>,
    /// Saved Internet Scrapbook entries.
    pub scrapbook_items: &'a [ScrapbookItemInfo<'a>],
    /// Saved Favorites list.
    pub favorites: &'a [NavEntryInfo<'a>],
    /// Active tab navigation history list.
    pub history_entries: &'a [NavEntryInfo<'a>],
}

/// Total height of the IE5 Mac top chrome:
/// Tab/Title Strip (18) + Aqua Button Bar (34) + Address Bar (18) + Favorites Strip (18) = 88.0.
pub const CHROME_HEIGHT: f64 = 88.0;
/// Width of the left vertical IE5 Mac Explorer Bar rail.
pub const EXPLORER_RAIL_WIDTH: f64 = 28.0;
/// Width of the collapsible Explorer Bar drawer panel when open.
pub const EXPLORER_DRAWER_WIDTH: f64 = 260.0;
/// Height of the Page Holder drawer's top control header bar.
pub const PH_HEADER_HEIGHT: f64 = 38.0;

pub fn render(state: &ChromeState<'_>) -> String {
    let p = palette(state.theme);
    let top_chrome_h = if state.toolbar_collapsed {
        0.0
    } else {
        CHROME_HEIGHT
    };
    let content_x = EXPLORER_RAIL_WIDTH
        + if state.explorer_tab.is_some() {
            EXPLORER_DRAWER_WIDTH
        } else {
            0.0
        };
    let back_class = button_class(state.can_back);
    let forward_class = button_class(state.can_forward);
    let stop_class = button_class(state.loading);
    let bar_class = if state.focused {
        "bar bar-focused"
    } else {
        "bar"
    };
    let caret = if state.focused {
        r#"<div class="caret"></div>"#
    } else {
        ""
    };
    let progress = if state.loading {
        r#"<div class="progress"></div>"#
    } else {
        ""
    };

    let mut tabs = String::new();
    for (index, tab) in state.tabs.iter().enumerate() {
        let class = if tab.active { "tab tab-active" } else { "tab" };
        let letter = first_letter(tab.title);
        tabs.push_str(&format!(
            r#"<div class="{class}" id="tab-{index}">
                 <div class="favicon">{letter}</div>
                 <div class="tab-title">{title}</div>
                 <div class="tab-close" id="tab-close-{index}">&#215;</div>
               </div>"#,
            class = class,
            index = index,
            letter = letter,
            title = escape_html(tab.title),
        ));
    }

    // Horizontal Favorites Bar items rendered directly from `state.favorites`
    let mut fav_strip_items = String::new();
    for (i, fav) in state.favorites.iter().take(5).enumerate() {
        fav_strip_items.push_str(&format!(
            r#"<div class="fav-pill" id="fav-bar-{i}" data-url="{url}"><span class="at-badge">@</span><span class="fav-pill-text">{title}</span></div>"#,
            i = i,
            url = escape_html(fav.url),
            title = escape_html(fav.title),
        ));
    }

    let back_svg_color = if state.can_back { "#3B8686" } else { "#9E9E9E" };
    let fwd_svg_color = if state.can_forward { "#3B8686" } else { "#9E9E9E" };
    let stop_svg_color = if state.loading { "#B83B2A" } else { "#9E9E9E" };
    let secure_attr = if state.secure { "https" } else { "http" };

    let top_bars = if state.toolbar_collapsed {
        String::new()
    } else {
        format!(
            r##"<div class="tabstrip">
                 <div class="win-box"></div>
                 {tabs}
                 <div class="newtab" id="btn-newtab">+</div>
               </div>
               <div class="nav">
                 <div class="{back_class}" id="btn-back">
                   <svg width="16" height="15" viewBox="0 0 16 15"><polygon points="10,2 3,7.5 10,13" fill="{back_svg_color}"/><rect x="9" y="5.5" width="5" height="4" fill="{back_svg_color}"/></svg>
                   <div class="btn-lbl">Back</div>
                 </div>
                 <div class="{forward_class}" id="btn-forward">
                   <svg width="16" height="15" viewBox="0 0 16 15"><polygon points="6,2 13,7.5 6,13" fill="{fwd_svg_color}"/><rect x="2" y="5.5" width="5" height="4" fill="{fwd_svg_color}"/></svg>
                   <div class="btn-lbl">Forward</div>
                 </div>
                 <div class="{stop_class}" id="btn-stop">
                   <svg width="16" height="15" viewBox="0 0 16 15"><path d="M3,3 L13,12 M13,3 L3,12" stroke="{stop_svg_color}" stroke-width="2.6" stroke-linecap="round"/></svg>
                   <div class="btn-lbl">Stop</div>
                 </div>
                 <div class="btn" id="btn-reload">
                   <svg width="16" height="15" viewBox="0 0 16 15"><path d="M4,7 A4.5,4.5 0 1,1 11.5,10.5" fill="none" stroke="#286090" stroke-width="2"/><polygon points="12,3 14.5,7 9.5,6.5" fill="#286090"/></svg>
                   <div class="btn-lbl">Refresh</div>
                 </div>
                 <div class="nav-sep"></div>
                 <div class="btn" id="btn-autofill">
                   <svg width="16" height="15" viewBox="0 0 16 15"><rect x="2" y="3" width="10" height="9" rx="1" fill="#EBF3F9" stroke="#286090" stroke-width="1.3"/><line x1="4" y1="6" x2="9" y2="6" stroke="#286090" stroke-width="1.2"/><line x1="4" y1="9" x2="8" y2="9" stroke="#286090" stroke-width="1.2"/><line x1="10" y1="11" x2="14" y2="2" stroke="#1F4E78" stroke-width="2"/></svg>
                   <div class="btn-lbl">AutoFill</div>
                 </div>
                 <div class="btn" id="btn-print">
                   <svg width="16" height="15" viewBox="0 0 16 15"><rect x="4" y="1.5" width="8" height="4" fill="#FFFFFF" stroke="#4A6075" stroke-width="1"/><rect x="2" y="5.5" width="12" height="6" rx="1" fill="#6990B2" stroke="#2E4E68" stroke-width="1.2"/><rect x="4" y="9.5" width="8" height="4" fill="#FFFFFF" stroke="#4A6075" stroke-width="1"/></svg>
                   <div class="btn-lbl">Print</div>
                 </div>
                 <div class="btn" id="btn-menu">
                   <svg width="16" height="15" viewBox="0 0 16 15"><circle cx="6.5" cy="6.5" r="4.2" fill="#D6E8F5" stroke="#286090" stroke-width="1.6"/><line x1="9.8" y1="9.8" x2="14" y2="13.8" stroke="#286090" stroke-width="2.2" stroke-linecap="round"/></svg>
                   <div class="btn-lbl">Sherlock</div>
                 </div>
               </div>
               <div class="address-row">
                 <div class="address-badge">Address</div>
                 <div class="{bar_class}" id="address" data-scheme="{secure_attr}">
                   <div class="at-badge">@</div>
                   <div class="url">{url}</div>
                   {caret}
                 </div>
               </div>
               <div class="favbar">
                 {fav_strip_items}
               </div>"##,
            tabs = tabs,
            back_class = back_class,
            forward_class = forward_class,
            stop_class = stop_class,
            back_svg_color = back_svg_color,
            fwd_svg_color = fwd_svg_color,
            stop_svg_color = stop_svg_color,
            bar_class = bar_class,
            secure_attr = secure_attr,
            url = escape_html(state.url),
            caret = caret,
            fav_strip_items = fav_strip_items,
        )
    };

    // Compact navigation buttons rendered inside the left Explorer Bar rail when the top toolbar is collapsed (PDF Page 2 right).
    let rail_compact_nav = if state.toolbar_collapsed {
        let b_cls = if state.can_back {
            "rail-icon"
        } else {
            "rail-icon rail-icon-off"
        };
        let f_cls = if state.can_forward {
            "rail-icon"
        } else {
            "rail-icon rail-icon-off"
        };
        let s_cls = if state.loading {
            "rail-icon"
        } else {
            "rail-icon rail-icon-off"
        };
        let b_col = if state.can_back { "#2E8B57" } else { "#999999" };
        let f_col = if state.can_forward { "#2E8B57" } else { "#999999" };
        let s_col = if state.loading { "#C0392B" } else { "#B85D54" };
        format!(
            r##"<div class="{b_cls}" id="btn-back">
                 <svg width="14" height="14" viewBox="0 0 14 14"><polygon points="9,2 3,7 9,12" fill="{b_col}"/><rect x="8" y="5" width="4" height="4" fill="{b_col}"/></svg>
               </div>
               <div class="{f_cls}" id="btn-forward">
                 <svg width="14" height="14" viewBox="0 0 14 14"><polygon points="5,2 11,7 5,12" fill="{f_col}"/><rect x="2" y="5" width="4" height="4" fill="{f_col}"/></svg>
               </div>
               <div class="{s_cls}" id="btn-stop">
                 <svg width="14" height="14" viewBox="0 0 14 14"><path d="M3,3 L11,11 M11,3 L3,11" stroke="{s_col}" stroke-width="2.2" stroke-linecap="round"/></svg>
               </div>
               <div class="rail-icon" id="btn-reload">
                 <svg width="14" height="14" viewBox="0 0 14 14"><path d="M3.5,6.5 A4,4 0 1,1 10,9.5" fill="none" stroke="#286090" stroke-width="1.8"/><polygon points="10.5,2.5 13,6.5 8.5,6" fill="#286090"/></svg>
               </div>"##,
        )
    } else {
        String::new()
    };

    let collapse_svg = if state.toolbar_collapsed {
        r##"<svg width="18" height="12" viewBox="0 0 18 12"><rect x="2" y="2" width="1.5" height="8" fill="#345470"/><rect x="5" y="2" width="1.5" height="8" fill="#345470"/><rect x="8" y="2" width="1.5" height="8" fill="#345470"/><polygon points="11,2 16,6 11,10" fill="#345470"/></svg>"##
    } else {
        r##"<svg width="18" height="12" viewBox="0 0 18 12"><polygon points="7,2 2,6 7,10" fill="#345470"/><rect x="9" y="2" width="1.5" height="8" fill="#345470"/><rect x="12" y="2" width="1.5" height="8" fill="#345470"/><rect x="15" y="2" width="1.5" height="8" fill="#345470"/></svg>"##
    };

    let fav_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Favorites));
    let hist_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::History));
    let srch_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Search));
    let scrp_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Scrapbook));
    let ph_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::PageHolder));

    let tab_svg_inactive = r##"<svg class="exp-tab-bg" width="22" height="76" viewBox="0 0 22 76"><polygon points="0,0 21,5 21,71 0,76" fill="#DFDFDF" stroke="#6E6E6E" stroke-width="1"/><polyline points="1,1 20,5.5" fill="none" stroke="#FFFFFF" stroke-width="1.2"/></svg>"##;
    let tab_svg_active = r##"<svg class="exp-tab-bg" width="22" height="76" viewBox="0 0 22 76"><polygon points="0,0 21,5 21,71 0,76" fill="#F7FAFC" stroke="#2B6CB0" stroke-width="1.4"/><polyline points="1,1 20,5.5" fill="none" stroke="#FFFFFF" stroke-width="1.4"/></svg>"##;
    let fav_bg = if state.explorer_tab == Some(ExplorerTab::Favorites) { tab_svg_active } else { tab_svg_inactive };
    let hist_bg = if state.explorer_tab == Some(ExplorerTab::History) { tab_svg_active } else { tab_svg_inactive };
    let srch_bg = if state.explorer_tab == Some(ExplorerTab::Search) { tab_svg_active } else { tab_svg_inactive };
    let scrp_bg = if state.explorer_tab == Some(ExplorerTab::Scrapbook) { tab_svg_active } else { tab_svg_inactive };
    let ph_bg = if state.explorer_tab == Some(ExplorerTab::PageHolder) { tab_svg_active } else { tab_svg_inactive };

    let explorer_rail = format!(
        r##"<div class="exp-rail" id="exp-rail" style="top:{top_chrome_h:.1}px;">
             <div class="rail-collapse" id="btn-collapse-toolbar">{collapse_svg}</div>
             {rail_compact_nav}
             <div class="{fav_cls}" id="exp-favorites">{fav_bg}<div class="exp-tab-label">Favorites</div></div>
             <div class="{hist_cls}" id="exp-history">{hist_bg}<div class="exp-tab-label">History</div></div>
             <div class="{srch_cls}" id="exp-search">{srch_bg}<div class="exp-tab-label">Search</div></div>
             <div class="{scrp_cls}" id="exp-scrapbook">{scrp_bg}<div class="exp-tab-label">Scrapbook</div></div>
             <div class="{ph_cls}" id="exp-pageholder">{ph_bg}<div class="exp-tab-label">Page Holder</div></div>
             <div class="rail-globe">
               <svg width="13" height="13" viewBox="0 0 14 14"><circle cx="7" cy="7" r="5.5" fill="#4A89DC" stroke="#1E4D8C" stroke-width="1"/><ellipse cx="7" cy="7" rx="2.8" ry="5.5" fill="none" stroke="#B5D3F7" stroke-width="0.9"/><line x1="1.5" y1="7" x2="12.5" y2="7" stroke="#B5D3F7" stroke-width="0.9"/></svg>
             </div>
           </div>"##,
    );

    // Camera-and-page icon used for Scrapbook entries (PDF Page 1 Figure 6).
    let sb_doc_icon = r##"<svg width="16" height="13" viewBox="0 0 16 13"><rect x="1" y="2" width="10" height="9" rx="1.5" fill="#7A8B99" stroke="#2F3E4C" stroke-width="1"/><circle cx="6" cy="6.5" r="2.3" fill="#D9E6F2" stroke="#2F3E4C" stroke-width="0.9"/><rect x="9" y="1" width="6" height="11" fill="#FFFFFF" stroke="#4A5568" stroke-width="0.9"/><line x1="10.5" y1="4" x2="13.5" y2="4" stroke="#718096" stroke-width="0.9"/><line x1="10.5" y1="6.5" x2="13.5" y2="6.5" stroke="#718096" stroke-width="0.9"/></svg>"##;

    let explorer_drawer = match state.explorer_tab {
        None => String::new(),
        Some(ExplorerTab::PageHolder) => {
            let links_btn_cls = if state.page_holder.links_mode {
                "mac-btn mac-btn-active"
            } else {
                "mac-btn"
            };
            let body_html = if state.page_holder.links_mode {
                let mut rows = String::new();
                if state.page_holder.links.is_empty() {
                    rows.push_str(
                        r#"<div class="drawer-empty" id="ph-empty-links">No links in held page. Click Add to pin a page with hyperlinks.</div>"#,
                    );
                } else {
                    for (i, link) in state.page_holder.links.iter().enumerate() {
                        rows.push_str(&format!(
                            r#"<div class="ph-link-row" id="ph-link-{i}" data-href="{url}">
                                 <span class="ph-tri">&#9656;</span>
                                 <span class="ph-link-text">{label}</span>
                               </div>"#,
                            i = i,
                            url = escape_html(link.url),
                            label = escape_html(link.label),
                        ));
                    }
                }
                format!(r#"<div class="drawer-body" id="ph-links-list">{rows}</div>"#)
            } else if state.page_holder.has_doc {
                // Transparent body so the live `page_holder_doc` painted in its clipped Vello layer underneath shows through.
                r#"<div class="drawer-body-transparent" id="ph-canvas"></div>"#.to_string()
            } else {
                r#"<div class="drawer-body"><div class="drawer-empty" id="ph-empty">Click "Add" to place a copy of the current page in the Page Holder for reference while browsing.</div></div>"#.to_string()
            };

            let drawer_bg_cls = if !state.page_holder.links_mode && state.page_holder.has_doc {
                "exp-drawer exp-drawer-transparent"
            } else {
                "exp-drawer"
            };

            let ph_url_attr = escape_html(state.page_holder.url);
            let ph_title_attr = escape_html(state.page_holder.title);
            format!(
                r#"<div class="{drawer_bg_cls}" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="ph-header" data-held-url="{ph_url_attr}" data-held-title="{ph_title_attr}">
                       <div class="drawer-brand"><div>Page</div><div>Holder</div></div>
                       <div class="mac-btn" id="ph-add">Add</div>
                       <div class="{links_btn_cls}" id="ph-links">Links</div>
                     </div>
                     <div class="drawer-subbar"></div>
                     {body_html}
                   </div>"#,
            )
        }
        Some(ExplorerTab::Scrapbook) => {
            let any_selected = state.scrapbook_items.iter().any(|item| item.selected);
            let delete_btn_cls = if any_selected {
                "mac-btn"
            } else {
                "mac-btn mac-btn-disabled"
            };
            let mut rows = String::new();
            if state.scrapbook_items.is_empty() {
                rows.push_str(
                    r#"<div class="drawer-empty" id="sb-empty">Click "Add" to save a complete offline snapshot of the current web page.</div>"#,
                );
            } else {
                for (i, item) in state.scrapbook_items.iter().enumerate() {
                    let cls = match (item.is_folder, item.selected) {
                        (true, true) => "sb-row sb-folder sb-row-selected",
                        (true, false) => "sb-row sb-folder",
                        (false, true) => "sb-row sb-row-selected",
                        (false, false) => "sb-row",
                    };
                    if item.is_folder {
                        rows.push_str(&format!(
                            r#"<div class="{cls}" id="sb-entry-{i}" data-sb-id="{id}" data-ts="{ts}">
                                 <span class="ph-tri">&#9662;</span>
                                 <span class="sb-row-title">{title}</span>
                               </div>"#,
                            cls = cls,
                            i = i,
                            id = escape_html(item.id),
                            ts = escape_html(item.timestamp),
                            title = escape_html(item.title),
                        ));
                    } else {
                        rows.push_str(&format!(
                            r#"<div class="{cls}" id="sb-entry-{i}" data-sb-id="{id}" data-url="{url}" data-ts="{ts}">
                                 {sb_doc_icon}
                                 <span class="sb-row-title">{title}</span>
                               </div>"#,
                            cls = cls,
                            i = i,
                            id = escape_html(item.id),
                            url = escape_html(item.url),
                            ts = escape_html(item.timestamp),
                            sb_doc_icon = sb_doc_icon,
                            title = escape_html(item.title),
                        ));
                    }
                }
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="sb-header">
                       <div class="drawer-brand"><div>Internet</div><div>Scrapbook</div></div>
                       <div class="mac-btn" id="sb-add">Add</div>
                       <div class="{delete_btn_cls}" id="sb-delete">Delete</div>
                       <div class="mac-btn" id="sb-organize">Organize &#9656;</div>
                     </div>
                     <div class="drawer-subbar"></div>
                     <div class="drawer-body" id="sb-list">{rows}</div>
                   </div>"#,
            )
        }
        Some(ExplorerTab::Favorites) => {
            let mut rows = String::new();
            for (i, fav) in state.favorites.iter().enumerate() {
                rows.push_str(&format!(
                    r#"<div class="sb-row" id="fav-item-{i}" data-url="{url}">
                         <span class="at-badge">@</span>
                         <span class="sb-row-title">{title}</span>
                       </div>"#,
                    i = i,
                    url = escape_html(fav.url),
                    title = escape_html(fav.title),
                ));
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="fav-header">
                       <div class="drawer-brand"><div>Internet</div><div>Favorites</div></div>
                       <div class="mac-btn" id="fav-add">Add</div>
                     </div>
                     <div class="drawer-subbar"></div>
                     <div class="drawer-body" id="fav-list">{rows}</div>
                   </div>"#,
            )
        }
        Some(ExplorerTab::History) => {
            let mut rows = String::new();
            if state.history_entries.is_empty() {
                rows.push_str(
                    r#"<div class="drawer-empty" id="hist-empty">No pages in tab history yet.</div>"#,
                );
            } else {
                for (i, entry) in state.history_entries.iter().enumerate() {
                    rows.push_str(&format!(
                        r#"<div class="sb-row" id="hist-item-{i}" data-url="{url}">
                             <span class="at-badge">@</span>
                             <span class="sb-row-title">{title}</span>
                           </div>"#,
                        i = i,
                        url = escape_html(entry.url),
                        title = escape_html(entry.title),
                    ));
                }
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="hist-header">
                       <div class="drawer-brand"><div>Browsing</div><div>History</div></div>
                     </div>
                     <div class="drawer-subbar"></div>
                     <div class="drawer-body" id="hist-list">{rows}</div>
                   </div>"#,
            )
        }
        Some(ExplorerTab::Search) => {
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="search-header">
                       <div class="drawer-brand"><div>Search</div><div>Assistant</div></div>
                       <div class="mac-btn" id="exp-search-focus">Address</div>
                     </div>
                     <div class="drawer-subbar"></div>
                     <div class="drawer-body" id="search-list">
                       <div class="ph-link-row" id="search-item-0">
                         <span class="ph-tri">&#9656;</span>
                         <span class="ph-link-text">DuckDuckGo Lite</span>
                       </div>
                       <div class="ph-link-row" id="search-item-1">
                         <span class="ph-tri">&#9656;</span>
                         <span class="ph-link-text">Wikipedia Encyclopedia</span>
                       </div>
                       <div class="ph-link-row" id="search-item-2">
                         <span class="ph-tri">&#9656;</span>
                         <span class="ph-link-text">AOL Portal</span>
                       </div>
                     </div>
                   </div>"#,
            )
        }
    };

    let supbar = match &state.supervision {
        Some(sup) => {
            let mut buttons = String::new();
            if sup.can_decide {
                buttons
                    .push_str(r#"<div class="sup-btn sup-approve" id="sup-approve">Approve</div>"#);
                buttons.push_str(r#"<div class="sup-btn" id="sup-reject">Reject</div>"#);
            }
            if sup.can_take {
                buttons.push_str(r#"<div class="sup-btn" id="sup-take">Take control</div>"#);
            }
            if sup.can_release {
                buttons.push_str(r#"<div class="sup-btn sup-approve" id="sup-done">Done</div>"#);
            }
            if sup.can_stop {
                buttons.push_str(r#"<div class="sup-btn sup-stop" id="sup-stop">Stop</div>"#);
            }
            format!(
                r#"<div class="supbar" id="sup-bar"><div class="sup-title">{}</div><div class="sup-state">{}</div><div class="sup-detail">{}</div>{}</div>"#,
                escape_html(sup.title),
                escape_html(sup.state),
                escape_html(sup.detail),
                buttons,
            )
        }
        None => String::new(),
    };

    let xray_layer = match &state.telemetry {
        Some(tel) => {
            let mut items = String::new();
            let min_y = top_chrome_h;
            let max_y = (state.surface_height - 32.0).max(min_y + 20.0);
            for b in &tel.overlays {
                let page_x = b.x.max(0.0) + content_x;
                let page_y = b.y + top_chrome_h;
                if page_y < min_y || page_y > max_y || b.width <= 1.0 || b.height <= 1.0 {
                    continue;
                }
                let w = b.width.max(24.0);
                let h = b.height.max(18.0);
                if b.is_frozen_target {
                    let fp = b.freeze_fp_short.as_deref().unwrap_or("bound");
                    items.push_str(&format!(
                        r#"<div class="xray-box xray-freeze" id="xray-freeze-shield" style="left:{page_x:.1}px;top:{page_y:.1}px;width:{w:.1}px;height:{h:.1}px;">
                             <div class="xray-pill xray-pill-freeze" id="xray-tag-{id}">[{id}: ENGINE COMMIT FREEZE &#183; fp:{fp}]</div>
                           </div>"#,
                        page_x = page_x,
                        page_y = page_y,
                        w = w,
                        h = h,
                        id = escape_html(&b.id),
                        fp = escape_html(fp),
                    ));
                } else if b.is_vault_secret {
                    items.push_str(&format!(
                        r#"<div class="xray-box xray-vault" id="xray-vault-shield" style="left:{page_x:.1}px;top:{page_y:.1}px;width:{w:.1}px;height:{h:.1}px;">
                             <div class="xray-pill xray-pill-vault" id="xray-tag-{id}">[{id}: BLIND VAULT &#8212; Hidden from LLM/JS]</div>
                           </div>"#,
                        page_x = page_x,
                        page_y = page_y,
                        w = w,
                        h = h,
                        id = escape_html(&b.id),
                    ));
                } else {
                    items.push_str(&format!(
                        r#"<div class="xray-box" id="xray-box-{id}" style="left:{page_x:.1}px;top:{page_y:.1}px;width:{w:.1}px;height:{h:.1}px;">
                             <div class="xray-pill" id="xray-tag-{id}">[{id}: {role}]</div>
                           </div>"#,
                        page_x = page_x,
                        page_y = page_y,
                        w = w,
                        h = h,
                        id = escape_html(&b.id),
                        role = escape_html(&b.role_tag),
                    ));
                }
            }
            let sig_text = tel.form_signature.as_deref().unwrap_or("none");
            let recall_text = match &tel.matched_recipe_id {
                Some(rid) => format!(
                    "MATCHED {} (Replayed: {} | 0 LLM calls)",
                    rid, tel.recalled_count
                ),
                None => format!("Replayed: {} (0 LLM calls)", tel.recalled_count),
            };
            items.push_str(&format!(
                r#"<div class="xray-hud" id="xray-telemetry-bar" style="left:{content_x:.1}px;">
                     <div class="xray-hud-brand">STATIC HERMES + BLITZ ENGINE</div>
                     <div class="xray-hud-metric" id="xray-metric-timing">Layout+Observe: {ms:.2}ms</div>
                     <div class="xray-hud-metric" id="xray-metric-rss">RAM/RSS: {rss:.1}MB</div>
                     <div class="xray-hud-metric" id="xray-metric-sig">FormSignature: {sig}</div>
                     <div class="xray-hud-metric xray-hud-recall" id="xray-metric-recall">Recall: {recall}</div>
                   </div>"#,
                content_x = content_x,
                ms = tel.layout_observe_ms,
                rss = tel.rss_mb,
                sig = escape_html(sig_text),
                recall = escape_html(&recall_text),
            ));
            items
        }
        None => String::new(),
    };

    let menu = if state.menu_open {
        r#"<div class="menu" id="menu">
             <div class="menu-item" id="menu-home">Home</div>
             <div class="menu-item" id="menu-reload">Reload</div>
             <div class="menu-sep"></div>
             <div class="menu-item" id="menu-back">Back</div>
             <div class="menu-item" id="menu-forward">Forward</div>
             <div class="menu-sep"></div>
             <div class="menu-item" id="menu-close-tab">Close tab</div>
           </div>"#
    } else {
        ""
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  html, body {{ height: {surface_height}px; overflow: hidden; }}
  body {{
    background-color: transparent;
    font-family: "Lucida Grande", "Geneva", "Helvetica Neue", Helvetica, Arial, sans-serif;
    position: relative;
  }}

  /* ---- Row 1: IE5 Mac Window / Document Tab Strip (18px) ---- */
  .tabstrip {{
    height: 18px;
    background-color: {strip};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 6px;
    gap: 4px;
  }}
  .win-box {{
    width: 10px;
    height: 10px;
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    margin-right: 4px;
  }}
  .tab {{
    height: 15px;
    max-width: 210px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 5px;
    padding: 0 7px;
    background-color: {rail_tab_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-radius: 3px 3px 0 0;
  }}
  .tab-active {{
    background-color: {tab_active};
    border-bottom: 1px solid {tab_active};
  }}
  .favicon {{
    width: 11px; height: 11px;
    border-radius: 2px;
    background-color: {favicon_bg};
    color: #FFFFFF;
    font-size: 8px;
    line-height: 11px;
    text-align: center;
    font-weight: 700;
  }}
  .tab-title {{
    font-size: 10px;
    font-weight: 700;
    color: {ink};
    white-space: nowrap;
    overflow: hidden;
  }}
  .tab-close {{
    font-size: 11px;
    color: {muted};
    padding: 0 2px;
  }}
  .newtab {{
    width: 16px; height: 14px;
    font-size: 12px;
    line-height: 13px;
    font-weight: 700;
    color: {ink};
    background-color: {rail_tab_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    text-align: center;
  }}

  /* ---- Row 2: IE5 Mac Custom Aqua Icon+Label Button Bar (34px) ---- */
  .nav {{
    height: 34px;
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {hairline};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 8px;
    gap: 10px;
  }}
  .btn {{
    height: 30px;
    min-width: 40px;
    padding: 1px 4px;
    border-radius: 3px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
  }}
  .btn:hover {{
    background-color: {hover};
    border: 1px solid {hairline};
  }}
  .btn-lbl {{
    font-size: 9px;
    line-height: 10px;
    color: {ink};
    font-weight: 600;
    white-space: nowrap;
  }}
  .btn-off .btn-lbl {{
    color: {muted};
  }}
  .nav-sep {{
    width: 1px;
    height: 24px;
    background-color: {separator};
    margin: 0 2px;
  }}

  /* ---- Row 3: Dedicated IE5 Mac Address Bar Row (18px) ---- */
  .address-row {{
    height: 18px;
    background-color: {strip};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {hairline};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 4px 0 2px;
    gap: 4px;
  }}
  .address-badge {{
    height: 15px;
    padding: 0 8px;
    background-color: {aqua_badge};
    border-top: 1px solid #9CC0DEC0;
    border-left: 1px solid #9CC0DEC0;
    border-right: 1px solid {aqua_badge_border};
    border-bottom: 1px solid {aqua_badge_border};
    border-radius: 3px;
    color: #FFFFFF;
    font-size: 10px;
    font-weight: 700;
    line-height: 14px;
  }}
  .bar {{
    flex: 1;
    height: 15px;
    background-color: {pill};
    border-top: 1px solid #555555;
    border-left: 1px solid #555555;
    border-right: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_light};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 5px;
    gap: 5px;
  }}
  .bar-focused {{
    background-color: {pill_focus};
    border: 1px solid {accent};
  }}
  .at-badge {{
    width: 12px;
    height: 12px;
    border-radius: 6px;
    background-color: #D9E5F0;
    border: 1px solid #5A7D9A;
    color: #264B6B;
    font-size: 8px;
    font-weight: 700;
    line-height: 10px;
    text-align: center;
  }}
  .url {{
    font-size: 10px;
    color: {ink};
    white-space: nowrap;
    overflow: hidden;
  }}
  .caret {{
    width: 1.5px;
    height: 11px;
    background-color: {accent};
  }}

  /* ---- Row 4: Horizontal Favorites Toolbar Strip (18px) ---- */
  .favbar {{
    height: 18px;
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 8px;
    gap: 14px;
    overflow: hidden;
  }}
  .fav-pill {{
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 4px;
    height: 14px;
  }}
  .fav-pill-text {{
    font-size: 10px;
    color: {ink};
    white-space: nowrap;
  }}

  /* ---- IE5 Mac Left Vertical Explorer Bar Rail (28px wide) ---- */
  .exp-rail {{
    position: absolute;
    left: 0;
    bottom: 0;
    width: 28px;
    background-color: {rail_bg};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2px 0;
    gap: 1px;
  }}
  .rail-collapse {{
    width: 24px;
    height: 16px;
    background-color: {rail_tab_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 2px;
  }}
  .rail-icon {{
    width: 22px;
    height: 20px;
    background-color: {rail_tab_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 1px;
  }}
  .rail-icon-off {{
    background-color: {rail_bg};
  }}
  .exp-tab {{
    position: relative;
    width: 22px;
    height: 76px;
    overflow: visible;
  }}
  .exp-tab-bg {{
    position: absolute;
    left: 0;
    top: 0;
    pointer-events: none;
  }}
  .exp-tab-active {{
    font-weight: 700;
  }}
  /* Sideways 90°-rotated label centered inside the 22x76 vertical tab */
  .exp-tab-label {{
    position: absolute;
    left: -25px;
    top: 31px;
    width: 72px;
    height: 13px;
    line-height: 13px;
    font-size: 10px;
    font-weight: 700;
    color: {ink};
    text-align: center;
    white-space: nowrap;
    transform: rotate(90deg);
    pointer-events: none;
  }}
  .exp-tab-active .exp-tab-label {{
    color: {brand_teal};
  }}
  .rail-globe {{
    margin-top: auto;
    margin-bottom: 3px;
    width: 20px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
  }}

  /* ---- IE5 Mac Explorer Bar Drawer (260px wide) ---- */
  .exp-drawer {{
    position: absolute;
    left: 28px;
    bottom: 0;
    width: 260px;
    background-color: {drawer_bg};
    border-right: 2px solid {bevel_dark};
    display: flex;
    flex-direction: column;
  }}
  .exp-drawer-transparent {{
    background-color: transparent;
  }}
  .drawer-header {{
    height: 32px;
    background-color: {drawer_header_bg};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 6px;
    gap: 5px;
  }}
  .drawer-subbar {{
    height: 6px;
    background-color: {strip};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {hairline};
  }}
  .drawer-brand {{
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    font-size: 10px;
    line-height: 10px;
    font-weight: 700;
    color: {brand_teal};
  }}
  .mac-btn {{
    height: 20px;
    padding: 2px 7px;
    background-color: {rail_tab_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    border-radius: 2px;
    font-size: 10px;
    line-height: 14px;
    font-weight: 600;
    color: {ink};
    white-space: nowrap;
  }}
  .mac-btn-active {{
    background-color: {aqua_badge};
    color: #FFFFFF;
  }}
  .mac-btn-disabled {{
    color: {muted};
  }}
  .drawer-body {{
    flex: 1;
    background-color: {drawer_bg};
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }}
  .drawer-body-transparent {{
    flex: 1;
    background-color: transparent;
  }}
  .drawer-empty {{
    font-size: 11px;
    color: {favicon_ink};
    padding: 12px 10px;
    line-height: 1.4;
  }}
  .sb-row {{
    height: 24px;
    padding: 0 8px;
    background-color: {drawer_bg};
    border-bottom: 1px solid {strip};
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }}
  .sb-folder {{
    background-color: {nav};
    font-weight: 700;
  }}
  .sb-row-selected {{
    background-color: #D0E2F2;
  }}
  .sb-row-title {{
    font-size: 11px;
    color: {ink};
    white-space: nowrap;
    overflow: hidden;
  }}
  .ph-link-row {{
    height: 20px;
    padding: 0 8px;
    background-color: {drawer_bg};
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 5px;
  }}
  .ph-tri {{
    font-size: 10px;
    color: {brand_teal};
  }}
  .ph-link-text {{
    font-size: 11px;
    color: {link_blue};
    text-decoration: underline;
    white-space: nowrap;
    overflow: hidden;
  }}

  /* menu */
  .menu {{
    position: absolute;
    right: 10px;
    top: 54px;
    width: 170px;
    background-color: {menu_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    padding: 4px;
  }}
  .menu-item {{ font-size: 11px; color: {ink}; padding: 5px 8px; }}
  .menu-item:hover {{ background-color: {accent}; color: #FFFFFF; }}
  .menu-sep {{ height: 1px; background-color: {separator}; margin: 4px 2px; }}

  /* supervision bar */
  .supbar {{
    position: absolute;
    right: 8px; top: 1px;
    height: 24px;
    background-color: {menu_bg};
    border: 1px solid {bevel_dark};
    border-radius: 4px;
    display: flex; flex-direction: row; align-items: center;
    padding: 0 8px;
    gap: 6px;
  }}
  .sup-title {{ font-size: 11px; font-weight: 700; color: {ink}; }}
  .sup-state {{ font-size: 10px; color: {accent}; }}
  .sup-detail {{ font-size: 10px; color: {favicon_ink}; flex: 1; }}
  .sup-btn {{
    font-size: 10px; color: {ink};
    background-color: {rail_tab_bg};
    border: 1px solid {bevel_dark};
    border-radius: 3px;
    padding: 2px 8px;
  }}
  .sup-approve {{ background-color: {accent}; color: #FFFFFF; }}
  .sup-stop {{ color: #D70015; }}

  /* ---- Semantic X-Ray & Engine Telemetry Overlay ---- */
  .xray-box {{
    position: absolute;
    border: 1.5px solid #0A84FF;
    border-radius: 4px;
  }}
  .xray-vault {{
    border: 2px solid #059669;
    background-color: rgba(5, 150, 105, 0.14);
  }}
  .xray-freeze {{
    border: 2.5px solid #D97706;
    background-color: rgba(217, 119, 6, 0.18);
  }}
  .xray-pill {{
    display: inline-block;
    background-color: #0A84FF;
    color: #FFFFFF;
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 3px;
  }}
  .xray-pill-vault {{
    background-color: #047857;
    color: #FFFFFF;
    font-size: 10px;
    padding: 2px 6px;
  }}
  .xray-pill-freeze {{
    background-color: #B45309;
    color: #FFFFFF;
    font-size: 10px;
    padding: 2px 6px;
  }}
  .xray-hud {{
    position: absolute;
    right: 0; bottom: 0;
    height: 30px;
    background-color: #111827;
    border-top: 1px solid #374151;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 14px;
    gap: 16px;
  }}
  .xray-hud-brand {{
    font-size: 11px;
    color: #38BDF8;
  }}
  .xray-hud-metric {{
    font-size: 11px;
    color: #E5E7EB;
  }}
  .xray-hud-recall {{
    color: #34D399;
  }}

  .progress {{
    position: absolute;
    left: 0; right: 0; bottom: 0;
    height: 2px;
    background-color: {accent};
  }}
</style>
</head>
<body>
  {top_bars}
  {explorer_rail}
  {explorer_drawer}
  {xray_layer}
  {menu}
  {supbar}
  {progress}
</body>
</html>"#,
        surface_height = state.surface_height,
        strip = p.strip,
        tab_active = p.tab_active,
        nav = p.nav,
        hairline = p.hairline,
        bevel_light = p.bevel_light,
        bevel_dark = p.bevel_dark,
        pill = p.pill,
        pill_focus = p.pill_focus,
        ink = p.ink,
        muted = p.muted,
        hover = p.hover,
        menu_bg = p.menu_bg,
        separator = p.separator,
        favicon_bg = p.favicon_bg,
        favicon_ink = p.favicon_ink,
        accent = p.accent,
        aqua_badge = p.aqua_badge,
        aqua_badge_border = p.aqua_badge_border,
        rail_bg = p.rail_bg,
        rail_tab_bg = p.rail_tab_bg,
        drawer_bg = p.drawer_bg,
        drawer_header_bg = p.drawer_header_bg,
        brand_teal = p.brand_teal,
        link_blue = p.link_blue,
        top_bars = top_bars,
        explorer_rail = explorer_rail,
        explorer_drawer = explorer_drawer,
        xray_layer = xray_layer,
        menu = menu,
        supbar = supbar,
        progress = progress,
    )
}

fn button_class(enabled: bool) -> &'static str {
    if enabled { "btn" } else { "btn btn-off" }
}

fn exp_tab_class(active: bool) -> &'static str {
    if active {
        "exp-tab exp-tab-active"
    } else {
        "exp-tab"
    }
}

/// Tab favicon placeholder: the first letter of the title.
fn first_letter(title: &str) -> String {
    title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}
