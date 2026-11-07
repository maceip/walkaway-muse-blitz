//! The browser chrome, rendered by the same HTML/CSS engine as the page.
//!
//! Faithfully implements the 4 walls of the Internet Explorer 5 Macintosh Edition
//! (Mac OS X Cheetah/Puma Aqua) window (`media_1794020340993_5f6a11ae.jpg` /
//! `media_1794021684097_68c9727c.jpg`) at 1:1 native pixel fidelity:
//! - Top Wall (`CHROME_HEIGHT = 116.0`):
//!   1. `23px` Mac OS X Aqua pinstriped title bar (`title_left` 67x23 gumdrops,
//!      centered `title_at` 16x16 + window title, `title_right` 17x23 cap).
//!   2. `41px` 4px-period horizontal pinstriped Aqua toolbar with contiguous 1:1
//!      native sprites (`Back`, `Forward`, `Stop`, `Refresh`, `Home`, beaded
//!      separator, `AutoFill`, `Print`, `Mail`) and the 42x41 `e` emblem on the right.
//!   3. `27px` cylindrical Bondi-Teal Aqua `Address:` bar (`addr_left` 71x27,
//!      18px sunken white input `#address` with `addr_at` 19x18, `addr_right` 51x27 `> go`).
//!   4. `25px` 4px-period horizontal pinstriped Favorites bar (`fav_left` 23x25,
//!      `@ Live Home Page`, `@ Apple`, `@ Apple Support`, `@ Apple Store`,
//!      `@ iTools`, `@ Mac OS X`, `@ Microsoft MacTopia`, and `fav_right` 25x25 `>>`).
//! - Left Wall (`EXPLORER_RAIL_WIDTH = 23.0`):
//!   - `23x23` `◄|||` collapse button (`#btn-collapse-toolbar`), `23x368` continuous
//!     5-tab vertical Explorer Bar strip (`Favorites`, `History`, `Search`,
//!     `Scrapbook`, `Page Holder`), and `23px` lower pinstriped rail with `1px`
//!     solid black inner right border.
//! - Right Wall (`1px`):
//!   - `1px solid #919191` outer right window frame border (no 15px scrollbar track).
//! - Bottom Wall (`16px`):
//!   - `23x16` `bottom_left` corner cap, `16px` 4px-period pinstriped bottom bar
//!     with `1px solid #010101` inner top border, and `16x16` `bottom_right` resize grip.

use std::fmt::Write;

use crate::ie5_sprites::*;

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
    nav: &'static str,
    hairline: &'static str,
    bevel_light: &'static str,
    pill: &'static str,
    ink: &'static str,
    muted: &'static str,
    menu_bg: &'static str,
    separator: &'static str,
    accent: &'static str,
    rail_bg: &'static str,
    drawer_bg: &'static str,
    drawer_header_bg: &'static str,
    link_blue: &'static str,
    rib_base: &'static str,
    rib_hi: &'static str,
    rib_lo: &'static str,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            nav: "#ECECEC",
            hairline: "#555555",
            bevel_light: "#FFFFFF",
            pill: "#FFFFFF",
            ink: "#111111",
            muted: "#6E6E6E",
            menu_bg: "#ECECEC",
            separator: "#B0B0B0",
            accent: "#16689E",
            rail_bg: "#ECECEC",
            drawer_bg: "#EEEEEE",
            drawer_header_bg: "#DEDEDE",
            link_blue: "#0000EE",
            rib_base: "#ECECEC",
            rib_hi: "#FCFCFC",
            rib_lo: "#DEDEDE",
        },
        Theme::Dark => Palette {
            nav: "#262629",
            hairline: "#48484C",
            bevel_light: "#3E3E42",
            pill: "#1B1B1D",
            ink: "#F2F2F7",
            muted: "#7E8894",
            menu_bg: "#2C2C2E",
            separator: "#3A3A3C",
            accent: "#38A3F8",
            rail_bg: "#222225",
            drawer_bg: "#1C1C1E",
            drawer_header_bg: "#28282B",
            link_blue: "#5AC8FA",
            rib_base: "#262629",
            rib_hi: "#303034",
            rib_lo: "#1D1D20",
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

/// Exact 1:1 height of the IE5 Mac top chrome (`media_1794020340993_5f6a11ae.jpg`):
/// Title Bar (23) + Aqua Toolbar (41) + Cylindrical Address Bar (27) + Favorites Bar (25) = 116.0.
pub const CHROME_HEIGHT: f64 = 116.0;
/// Exact 1:1 width of the vertical IE5 Mac Explorer Bar folder-tab rail (`23px`).
pub const EXPLORER_RAIL_WIDTH: f64 = 23.0;
/// Width of the collapsible Explorer Bar drawer panel when open.
pub const EXPLORER_DRAWER_WIDTH: f64 = 286.0;
/// Height of the Page Holder drawer's top control header bar.
pub const PH_HEADER_HEIGHT: f64 = 52.0;

fn ribbed_rect_svg(width: u32, height: u32, base: &str, hi: &str, lo: &str) -> String {
    let mut hi_path = String::new();
    let mut lo_path = String::new();
    let mut y = 1u32;
    while y < height {
        let _ = write!(hi_path, "M0,{}.5 H{} ", y, width);
        if y + 1 < height {
            let _ = write!(lo_path, "M0,{}.5 H{} ", y + 1, width);
        }
        y += 4;
    }
    format!(
        r##"<div class="rib-bg"><svg width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="{width}" height="{height}" fill="{base}"/><path d="{hi_path}" stroke="{hi}" stroke-width="1"/><path d="{lo_path}" stroke="{lo}" stroke-width="1"/></svg></div>"##
    )
}

pub fn render(state: &ChromeState<'_>) -> String {
    let p = palette(state.theme);
    let top_chrome_h = if state.toolbar_collapsed {
        0.0
    } else {
        CHROME_HEIGHT
    };
    let drawer_open = state.explorer_tab.is_some();
    let rail_left_x = if drawer_open {
        EXPLORER_DRAWER_WIDTH
    } else {
        0.0
    };
    let content_x = EXPLORER_RAIL_WIDTH
        + if drawer_open {
            EXPLORER_DRAWER_WIDTH
        } else {
            0.0
        };

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

    let single_tab = state.tabs.len() <= 1;
    let tab_close_cls = if single_tab {
        "tab-close tab-close-single"
    } else {
        "tab-close"
    };
    let newtab_cls = if single_tab {
        "newtab newtab-single"
    } else {
        "newtab"
    };

    let mut tabs = String::new();
    for (index, tab) in state.tabs.iter().enumerate() {
        let class = if tab.active { "tab tab-active" } else { "tab" };
        tabs.push_str(&format!(
            r#"<div class="{class}" id="tab-{index}">
                 {SPRITE_TITLE_AT}
                 <div class="tab-title">{title}</div>
                 <div class="{tab_close_cls}" id="tab-close-{index}">&#215;</div>
               </div>"#,
            class = class,
            index = index,
            SPRITE_TITLE_AT = SPRITE_TITLE_AT,
            title = escape_html(tab.title),
            tab_close_cls = tab_close_cls,
        ));
    }

    // Horizontal Favorites Bar items rendered directly from `state.favorites`
    let mut fav_strip_items = String::new();
    for (i, fav) in state.favorites.iter().take(7).enumerate() {
        fav_strip_items.push_str(&format!(
            r#"<div class="fav-pill" id="fav-bar-{i}" data-url="{url}">{SPRITE_FAV_AT}<span class="fav-pill-text">{title}</span></div>"#,
            i = i,
            url = escape_html(fav.url),
            SPRITE_FAV_AT = SPRITE_FAV_AT,
            title = escape_html(fav.title),
        ));
    }

    let secure_attr = if state.secure { "https" } else { "http" };

    // Exact 1px scanline SVG backgrounds from `media_1794020340993_5f6a11ae.jpg`
    let title_scan_bg = title_bg(2400);
    let nav_scan_bg = nav_bg(2400);
    let addr_scan_bg = addr_mid_bg(2400);
    let fav_scan_bg = fav_bg(2400);

    let back_btn_svg = if state.can_back {
        SPRITE_BTN_BACK_ON
    } else {
        SPRITE_BTN_BACK_OFF
    };
    let fwd_btn_svg = if state.can_forward {
        SPRITE_BTN_FWD_ON
    } else {
        SPRITE_BTN_FWD_OFF
    };
    let stop_btn_svg = if state.loading {
        SPRITE_BTN_STOP_ON
    } else {
        SPRITE_BTN_STOP_OFF
    };

    let top_bars = if state.toolbar_collapsed {
        String::new()
    } else {
        format!(
            r##"<div class="tabstrip">
                 {title_scan_bg}
                 <div class="traffic-lights">{SPRITE_TITLE_LEFT}</div>
                 <div class="title-tabs-wrap">
                   {tabs}
                   <div class="{newtab_cls}" id="btn-newtab">+</div>
                 </div>
                 <div class="aqua-pill-btn" id="btn-menu">{SPRITE_TITLE_RIGHT}</div>
               </div>
               <div class="nav">
                 {nav_scan_bg}
                 <div class="btn btn-back" id="btn-back">{back_btn_svg}</div>
                 <div class="btn btn-fwd" id="btn-forward">{fwd_btn_svg}</div>
                 <div class="btn btn-stop" id="btn-stop">{stop_btn_svg}</div>
                 <div class="btn btn-refresh" id="btn-reload">{SPRITE_BTN_REFRESH}</div>
                 <div class="btn btn-home" id="btn-home">{SPRITE_BTN_HOME}</div>
                 <div class="nav-sep">{SPRITE_NAV_SEP}</div>
                 <div class="btn btn-autofill" id="btn-autofill">{SPRITE_BTN_AUTOFILL}</div>
                 <div class="btn btn-print" id="btn-print">{SPRITE_BTN_PRINT}</div>
                 <div class="btn btn-mail" id="btn-mail">{SPRITE_BTN_MAIL}</div>
                 <div class="ie-emblem">{SPRITE_NAV_LOGO_E}</div>
               </div>
               <div class="address-row">
                 <div class="address-badge">{SPRITE_ADDR_LEFT}</div>
                 <div class="{bar_class}" id="address" data-scheme="{secure_attr}">
                   {addr_scan_bg}
                   <div class="addr-inner">
                     {SPRITE_ADDR_AT}
                     <div class="url">{url}</div>
                     {caret}
                   </div>
                 </div>
                 <div class="go-btn" id="btn-go">{SPRITE_ADDR_RIGHT}</div>
               </div>
               <div class="favbar">
                 {fav_scan_bg}
                 <div class="fav-left-cap">{SPRITE_FAV_LEFT}</div>
                 <div class="fav-items-wrap">{fav_strip_items}</div>
                 <div class="fav-chevron" id="btn-fav-overflow">{SPRITE_FAV_RIGHT}</div>
               </div>"##,
            title_scan_bg = title_scan_bg,
            SPRITE_TITLE_LEFT = SPRITE_TITLE_LEFT,
            tabs = tabs,
            newtab_cls = newtab_cls,
            SPRITE_TITLE_RIGHT = SPRITE_TITLE_RIGHT,
            nav_scan_bg = nav_scan_bg,
            back_btn_svg = back_btn_svg,
            fwd_btn_svg = fwd_btn_svg,
            stop_btn_svg = stop_btn_svg,
            SPRITE_BTN_REFRESH = SPRITE_BTN_REFRESH,
            SPRITE_BTN_HOME = SPRITE_BTN_HOME,
            SPRITE_NAV_SEP = SPRITE_NAV_SEP,
            SPRITE_BTN_AUTOFILL = SPRITE_BTN_AUTOFILL,
            SPRITE_BTN_PRINT = SPRITE_BTN_PRINT,
            SPRITE_BTN_MAIL = SPRITE_BTN_MAIL,
            SPRITE_NAV_LOGO_E = SPRITE_NAV_LOGO_E,
            SPRITE_ADDR_LEFT = SPRITE_ADDR_LEFT,
            bar_class = bar_class,
            secure_attr = secure_attr,
            addr_scan_bg = addr_scan_bg,
            SPRITE_ADDR_AT = SPRITE_ADDR_AT,
            url = escape_html(state.url),
            caret = caret,
            SPRITE_ADDR_RIGHT = SPRITE_ADDR_RIGHT,
            fav_scan_bg = fav_scan_bg,
            SPRITE_FAV_LEFT = SPRITE_FAV_LEFT,
            fav_strip_items = fav_strip_items,
            SPRITE_FAV_RIGHT = SPRITE_FAV_RIGHT,
        )
    };

    // Compact 23x22 navigation buttons rendered inside the left Explorer Bar rail when top toolbar is collapsed.
    let rail_compact_nav = if state.toolbar_collapsed {
        let b_svg = if state.can_back {
            SPRITE_RAIL_BACK_ON
        } else {
            SPRITE_RAIL_BACK_OFF
        };
        let f_svg = if state.can_forward {
            SPRITE_RAIL_FWD_ON
        } else {
            SPRITE_RAIL_FWD_OFF
        };
        let s_svg = if state.loading {
            SPRITE_RAIL_STOP_ON
        } else {
            SPRITE_RAIL_STOP_OFF
        };
        format!(
            r##"<div class="rail-icon" id="btn-back">{b_svg}</div>
               <div class="rail-icon" id="btn-forward">{f_svg}</div>
               <div class="rail-icon" id="btn-stop">{s_svg}</div>
               <div class="rail-icon" id="btn-reload">{SPRITE_RAIL_REFRESH}</div>"##,
            b_svg = b_svg,
            f_svg = f_svg,
            s_svg = s_svg,
            SPRITE_RAIL_REFRESH = SPRITE_RAIL_REFRESH,
        )
    } else {
        String::new()
    };

    let collapse_svg = if state.toolbar_collapsed {
        SPRITE_RAIL_COLLAPSE_CLOSED
    } else {
        SPRITE_RAIL_COLLAPSE_OPEN
    };

    let rail_tabs_svg = match state.explorer_tab {
        None => SPRITE_RAIL_TABS_NONE,
        Some(ExplorerTab::Favorites) => SPRITE_RAIL_TABS_FAV,
        Some(ExplorerTab::History) => SPRITE_RAIL_TABS_HIST,
        Some(ExplorerTab::Search) => SPRITE_RAIL_TABS_SRCH,
        Some(ExplorerTab::Scrapbook) => SPRITE_RAIL_TABS_SCRP,
        Some(ExplorerTab::PageHolder) => SPRITE_RAIL_TABS_PH,
    };

    let rail_lower_svg = rail_lower_bg(1200, drawer_open);
    let drawer_rib_bg = ribbed_rect_svg(286, 1200, p.rib_base, p.rib_hi, p.rib_lo);

    let explorer_rail = format!(
        r##"<div class="exp-rail" id="exp-rail" style="top:{top_chrome_h:.1}px;left:{rail_left_x:.1}px;">
             <div class="rail-collapse" id="btn-collapse-toolbar">{collapse_svg}</div>
             {rail_compact_nav}
             <div class="exp-tabs-wrap">
               <div class="exp-tabs-bg">{rail_tabs_svg}</div>
               <div class="exp-tab" id="exp-favorites" style="height:68px;"></div>
               <div class="exp-tab" id="exp-history" style="height:60px;"></div>
               <div class="exp-tab" id="exp-search" style="height:61px;"></div>
               <div class="exp-tab" id="exp-scrapbook" style="height:81px;"></div>
               <div class="exp-tab" id="exp-pageholder" style="height:98px;"></div>
             </div>
             <div class="rail-globe">{rail_lower_svg}</div>
           </div>"##,
    );

    // Camera-and-page icon used for Scrapbook entries (PDF Page 1 Figure 6).
    let sb_doc_icon = r##"<svg class="sp-svg" width="18" height="15" viewBox="0 0 18 15"><rect x="1" y="2.5" width="11" height="10" rx="1.8" fill="#7A8B99" stroke="#2F3E4C" stroke-width="1.1"/><circle cx="6.5" cy="7.5" r="2.6" fill="#D9E6F2" stroke="#2F3E4C" stroke-width="1"/><rect x="10" y="1" width="7" height="12.5" fill="#FFFFFF" stroke="#4A5568" stroke-width="1"/><line x1="11.5" y1="4.5" x2="15" y2="4.5" stroke="#286090" stroke-width="1"/><line x1="11.5" y1="7.5" x2="15" y2="7.5" stroke="#718096" stroke-width="1"/></svg>"##;
    let link_tri_svg = r##"<svg class="ph-tri-svg" width="8" height="10" viewBox="0 0 8 10"><polygon points="1,1 7,5 1,9" fill="#146696"/></svg>"##;
    let btn_tri_svg = r##"<svg class="ph-tri-svg" width="6" height="10" viewBox="0 0 6 10"><polygon points="1,4 5,7 1,10" fill="#111111"/></svg>"##;

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
                                 {link_tri_svg}
                                 <span class="ph-link-text">{label}</span>
                               </div>"#,
                            i = i,
                            url = escape_html(link.url),
                            link_tri_svg = link_tri_svg,
                            label = escape_html(link.label),
                        ));
                    }
                }
                format!(r#"<div class="drawer-body drawer-body-white" id="ph-links-list">{rows}</div>"#)
            } else if state.page_holder.has_doc {
                r#"<div class="drawer-body-transparent" id="ph-canvas"></div>"#.to_string()
            } else {
                r#"<div class="drawer-body"><div class="drawer-empty" id="ph-empty">Click "Add" to place a copy of the current page in the Page Holder for reference while browsing.</div></div>"#.to_string()
            };

            let drawer_bg_cls = if !state.page_holder.links_mode && state.page_holder.has_doc {
                "exp-drawer exp-drawer-transparent"
            } else {
                "exp-drawer"
            };
            let frame_cls = if !state.page_holder.links_mode && state.page_holder.has_doc {
                "drawer-frame drawer-frame-transparent"
            } else {
                "drawer-frame"
            };

            let ph_url_attr = escape_html(state.page_holder.url);
            let ph_title_attr = escape_html(state.page_holder.title);
            format!(
                r#"<div class="{drawer_bg_cls}" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     {drawer_rib_bg}
                     <div class="{frame_cls}">
                       <div class="drawer-inner">
                         <div class="drawer-header" id="ph-header" data-held-url="{ph_url_attr}" data-held-title="{ph_title_attr}">
                           <div class="drawer-brand"><div>Page</div><div>Holder</div></div>
                           <div class="mac-btn" id="ph-add">Add</div>
                           <div class="{links_btn_cls}" id="ph-links">Links</div>
                         </div>
                         {body_html}
                       </div>
                     </div>
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
                                 {link_tri_svg}
                                 <span class="sb-row-title">{title}</span>
                               </div>"#,
                            cls = cls,
                            i = i,
                            id = escape_html(item.id),
                            ts = escape_html(item.timestamp),
                            link_tri_svg = link_tri_svg,
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
                     {drawer_rib_bg}
                     <div class="drawer-frame">
                       <div class="drawer-inner">
                         <div class="drawer-header" id="sb-header">
                           <div class="drawer-brand"><div>Internet</div><div>Scrapbook</div></div>
                           <div class="mac-btn" id="sb-add">Add</div>
                           <div class="{delete_btn_cls}" id="sb-delete">Delete</div>
                           <div class="mac-btn" id="sb-organize"><span>Organize</span>{btn_tri_svg}</div>
                         </div>
                         <div class="drawer-body" id="sb-list">{rows}</div>
                       </div>
                     </div>
                   </div>"#,
            )
        }
        Some(ExplorerTab::Favorites) => {
            let mut rows = String::new();
            for (i, fav) in state.favorites.iter().enumerate() {
                rows.push_str(&format!(
                    r#"<div class="sb-row" id="fav-item-{i}" data-url="{url}">
                         {SPRITE_FAV_AT}
                         <span class="sb-row-title">{title}</span>
                       </div>"#,
                    i = i,
                    url = escape_html(fav.url),
                    SPRITE_FAV_AT = SPRITE_FAV_AT,
                    title = escape_html(fav.title),
                ));
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     {drawer_rib_bg}
                     <div class="drawer-frame">
                       <div class="drawer-inner">
                         <div class="drawer-header" id="fav-header">
                           <div class="drawer-brand"><div>Internet</div><div>Favorites</div></div>
                           <div class="mac-btn" id="fav-add">Add</div>
                         </div>
                         <div class="drawer-body" id="fav-list">{rows}</div>
                       </div>
                     </div>
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
                             {SPRITE_FAV_AT}
                             <span class="sb-row-title">{title}</span>
                           </div>"#,
                        i = i,
                        url = escape_html(entry.url),
                        SPRITE_FAV_AT = SPRITE_FAV_AT,
                        title = escape_html(entry.title),
                    ));
                }
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     {drawer_rib_bg}
                     <div class="drawer-frame">
                       <div class="drawer-inner">
                         <div class="drawer-header" id="hist-header">
                           <div class="drawer-brand"><div>Browsing</div><div>History</div></div>
                         </div>
                         <div class="drawer-body" id="hist-list">{rows}</div>
                       </div>
                     </div>
                   </div>"#,
            )
        }
        Some(ExplorerTab::Search) => {
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     {drawer_rib_bg}
                     <div class="drawer-frame">
                       <div class="drawer-inner">
                         <div class="drawer-header" id="search-header">
                           <div class="drawer-brand"><div>Search</div><div>Assistant</div></div>
                           <div class="mac-btn" id="exp-search-focus">Address</div>
                         </div>
                         <div class="drawer-body drawer-body-white" id="search-list">
                           <div class="ph-link-row" id="search-item-0">
                             {link_tri_svg}
                             <span class="ph-link-text">DuckDuckGo Lite</span>
                           </div>
                           <div class="ph-link-row" id="search-item-1">
                             {link_tri_svg}
                             <span class="ph-link-text">Wikipedia Encyclopedia</span>
                           </div>
                           <div class="ph-link-row" id="search-item-2">
                             {link_tri_svg}
                             <span class="ph-link-text">AOL Portal</span>
                           </div>
                         </div>
                       </div>
                     </div>
                   </div>"#,
            )
        }
    };

    // Wall 3 (Right 1px window border) + Wall 4 (Bottom 16px pinstriped bar + 16x16 resize grip corner)
    let bottom_scan_bg = bottom_mid_bg(2400);
    let bottom_aqua_bar = format!(
        r##"<div class="window-right-wall" style="top:{top_chrome_h:.1}px;"></div>
           <div class="window-bottom-wall">
             <div class="bottom-left-cap">{SPRITE_BOTTOM_LEFT}</div>
             <div class="bottom-mid">{bottom_scan_bg}</div>
             <div class="bottom-right-cap">{SPRITE_BOTTOM_RIGHT}</div>
           </div>"##,
        top_chrome_h = top_chrome_h,
        SPRITE_BOTTOM_LEFT = SPRITE_BOTTOM_LEFT,
        bottom_scan_bg = bottom_scan_bg,
        SPRITE_BOTTOM_RIGHT = SPRITE_BOTTOM_RIGHT,
    );

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
        format!(
            r#"<div class="menu" id="menu">
                 <div class="menu-item" id="menu-home">{SPRITE_FAV_AT}<span>Home</span></div>
                 <div class="menu-item" id="menu-reload">{SPRITE_FAV_AT}<span>Reload</span></div>
                 <div class="menu-sep"></div>
                 <div class="menu-item" id="menu-back">{SPRITE_FAV_AT}<span>Back</span></div>
                 <div class="menu-item" id="menu-forward">{SPRITE_FAV_AT}<span>Forward</span></div>
                 <div class="menu-sep"></div>
                 <div class="menu-item" id="menu-close-tab">{SPRITE_FAV_AT}<span>Close tab</span></div>
               </div>"#,
            SPRITE_FAV_AT = SPRITE_FAV_AT,
        )
    } else {
        String::new()
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
  .rib-bg {{
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
  }}
  .sp-svg, .ph-tri-svg, svg, image {{
    flex-shrink: 0;
    pointer-events: none;
  }}

  /* ---- Row 1: Mac OS X Aqua Pinstriped Window Title Bar (23px) ---- */
  .tabstrip {{
    position: relative;
    height: 23px;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0;
    gap: 0;
    overflow: hidden;
  }}
  .traffic-lights {{
    position: relative;
    width: 67px;
    height: 23px;
    flex-shrink: 0;
    pointer-events: none;
  }}
  .title-tabs-wrap {{
    position: relative;
    flex: 1;
    height: 22px;
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 6px;
    overflow: hidden;
  }}
  .aqua-pill-btn {{
    position: relative;
    width: 17px;
    height: 23px;
    flex-shrink: 0;
  }}
  .tab {{
    height: 20px;
    max-width: 340px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 5px;
    padding: 0 4px 1px 4px;
    background-color: transparent;
    color: #181818;
    font-size: 13px;
    font-weight: 700;
    flex-shrink: 0;
  }}
  .tab-active {{
    color: #000000;
  }}
  .tab-title {{
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    line-height: 18px;
    pointer-events: none;
  }}
  .tab-close {{
    width: 14px;
    height: 14px;
    font-size: 11px;
    line-height: 13px;
    text-align: center;
    color: #555555;
  }}
  .tab-close-single {{
    color: transparent;
  }}
  .newtab {{
    width: 18px;
    height: 16px;
    font-size: 12px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
    color: #444444;
    background-color: #E2E2E2;
    border: 1px solid #888888;
    flex-shrink: 0;
  }}
  .newtab-single {{
    color: transparent;
    background-color: transparent;
    border: none;
  }}

  /* ---- Row 2: IE5 Mac 41px Aqua Toolbar ---- */
  .nav {{
    position: relative;
    height: 41px;
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    padding: 0;
    gap: 0;
    overflow: hidden;
  }}
  .btn {{
    position: relative;
    height: 41px;
    flex-shrink: 0;
  }}
  .btn-back {{ width: 72px; }}
  .btn-fwd {{ width: 63px; }}
  .btn-stop {{ width: 55px; }}
  .btn-refresh {{ width: 53px; }}
  .btn-home {{ width: 51px; }}
  .btn-autofill {{ width: 60px; }}
  .btn-print {{ width: 52px; }}
  .btn-mail {{ width: 50px; }}
  .nav-sep {{
    position: relative;
    width: 11px;
    height: 41px;
    flex-shrink: 0;
    pointer-events: none;
  }}
  .ie-emblem {{
    position: relative;
    margin-left: auto;
    width: 42px;
    height: 41px;
    flex-shrink: 0;
    pointer-events: none;
  }}

  /* ---- Row 3: IE5 Mac 27px Cylindrical Bondi-Teal Address Bar ---- */
  .address-row {{
    position: relative;
    height: 27px;
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    padding: 0;
    gap: 0;
    overflow: hidden;
  }}
  .address-badge {{
    position: relative;
    width: 71px;
    height: 27px;
    flex-shrink: 0;
    pointer-events: none;
  }}
  .bar {{
    position: relative;
    flex: 1;
    height: 27px;
    overflow: hidden;
  }}
  .bar-focused {{
    position: relative;
  }}
  .addr-inner {{
    position: relative;
    margin-top: 5px;
    height: 18px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 3px;
    padding-right: 6px;
    pointer-events: none;
  }}
  .url {{
    color: #111111;
    font-size: 11.5px;
    line-height: 18px;
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
  }}
  .caret {{
    width: 1px;
    height: 13px;
    background-color: #000000;
    pointer-events: none;
  }}
  .go-btn {{
    position: relative;
    width: 51px;
    height: 27px;
    flex-shrink: 0;
  }}

  /* ---- Row 4: IE5 Mac 25px Favorites Bar ---- */
  .favbar {{
    position: relative;
    height: 25px;
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    padding: 0;
    gap: 0;
    overflow: hidden;
  }}
  .fav-left-cap {{
    position: relative;
    width: 23px;
    height: 25px;
    flex-shrink: 0;
    pointer-events: none;
  }}
  .fav-items-wrap {{
    position: relative;
    height: 24px;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding-left: 7px;
    gap: 12px;
    overflow: hidden;
  }}
  .fav-pill {{
    position: relative;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 4px;
    height: 19px;
    padding: 0 2px;
    font-size: 10.5px;
    font-weight: 500;
    color: #141414;
    white-space: nowrap;
  }}
  .fav-pill-text {{
    line-height: 18px;
    pointer-events: none;
  }}
  .fav-chevron {{
    position: relative;
    margin-left: auto;
    width: 25px;
    height: 25px;
    flex-shrink: 0;
  }}

  /* ---- Wall 2: Left Vertical Explorer Bar Rail (23px wide) ---- */
  .exp-rail {{
    position: absolute;
    bottom: 16px;
    width: 23px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 0;
    gap: 0;
    overflow: hidden;
  }}
  .rail-collapse {{
    position: relative;
    width: 23px;
    height: 23px;
    flex-shrink: 0;
  }}
  .rail-icon {{
    position: relative;
    width: 23px;
    height: 22px;
    flex-shrink: 0;
  }}
  .exp-tabs-wrap {{
    position: relative;
    width: 23px;
    height: 368px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
  }}
  .exp-tabs-bg {{
    position: absolute;
    top: 0;
    left: 0;
    width: 23px;
    height: 368px;
    pointer-events: none;
  }}
  .exp-tab {{
    position: relative;
    width: 23px;
    flex-shrink: 0;
  }}
  .rail-globe {{
    position: relative;
    flex: 1;
    width: 23px;
    overflow: hidden;
    pointer-events: none;
  }}

  /* ---- Explorer Bar Collapsible Drawer (286px wide, left of the vertical tab rail) ---- */
  .exp-drawer {{
    position: absolute;
    left: 0;
    bottom: 16px;
    width: 286px;
    background-color: {rail_bg};
    padding: 4px 0 0 6px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }}
  .exp-drawer-transparent {{
    background-color: transparent;
  }}
  .drawer-frame {{
    position: relative;
    flex: 1;
    background-color: transparent;
    border-top: 1px solid #333333;
    border-left: 1px solid #333333;
    padding: 4px 4px 4px 4px;
    display: flex;
    flex-direction: column;
  }}
  .drawer-frame-transparent {{
    background-color: transparent;
  }}
  .drawer-inner {{
    flex: 1;
    border: 1px solid #4A4A4A;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }}
  .drawer-header {{
    height: 44px;
    background-color: {drawer_header_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-bottom: 1.5px solid #222222;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 6px 0 7px;
    gap: 4px;
  }}
  .drawer-brand {{
    color: #207194;
    font-size: 14px;
    font-weight: 700;
    line-height: 14.5px;
    letter-spacing: -0.02em;
    margin-right: auto;
    pointer-events: none;
  }}
  .mac-btn {{
    height: 25px;
    padding: 0 7px;
    background-color: #DCDCDC;
    border-top: 1.5px solid {bevel_light};
    border-left: 1.5px solid {bevel_light};
    border-right: 1.5px solid #1A1A1A;
    border-bottom: 1.5px solid #1A1A1A;
    color: {ink};
    font-size: 11.5px;
    font-weight: 500;
    line-height: 21px;
    white-space: nowrap;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 4px;
  }}
  .mac-btn span {{
    pointer-events: none;
  }}
  .mac-btn-active {{
    background-color: #B8D6EC;
    border-top: 1.5px solid #1A1A1A;
    border-left: 1.5px solid #1A1A1A;
    border-right: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_light};
  }}
  .mac-btn-disabled {{
    color: #888888;
    background-color: #DEDEDE;
    border-top: 1px solid #EFEFEF;
    border-left: 1px solid #EFEFEF;
    border-right: 1px solid #888888;
    border-bottom: 1px solid #888888;
  }}
  .drawer-body {{
    flex: 1;
    background-color: {drawer_bg};
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }}
  .drawer-body-white {{
    background-color: #FFFFFF;
  }}
  .drawer-body-transparent {{
    flex: 1;
    background-color: transparent;
  }}
  .drawer-empty {{
    padding: 16px 14px;
    color: {muted};
    font-size: 12px;
    line-height: 17px;
  }}
  .ph-link-row {{
    min-height: 24px;
    padding: 3px 12px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    background-color: #FFFFFF;
  }}
  .ph-link-text {{
    color: {link_blue};
    font-size: 12px;
    line-height: 18px;
    text-decoration: underline;
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
  }}
  .sb-row {{
    height: 26px;
    padding: 0 10px 0 22px;
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid #D8D8D8;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    background-color: {drawer_bg};
  }}
  .sb-folder {{
    padding-left: 10px;
    background-color: {drawer_header_bg};
    font-weight: 700;
  }}
  .sb-row-selected {{
    background-color: #C4E0F5;
  }}
  .sb-row-title {{
    color: {ink};
    font-size: 12px;
    line-height: 24px;
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
  }}

  /* ---- Wall 3 (1px Right Border) & Wall 4 (16px Bottom Pinstriped Bar + Resize Grip) ---- */
  .window-right-wall {{
    position: absolute;
    right: 0;
    bottom: 16px;
    width: 1px;
    background-color: #919191;
    pointer-events: none;
  }}
  .window-bottom-wall {{
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 16px;
    display: flex;
    flex-direction: row;
    pointer-events: none;
  }}
  .bottom-left-cap {{
    position: relative;
    width: 23px;
    height: 16px;
    flex-shrink: 0;
  }}
  .bottom-mid {{
    position: relative;
    flex: 1;
    height: 16px;
    overflow: hidden;
  }}
  .bottom-right-cap {{
    position: relative;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }}

  .progress {{
    position: absolute;
    left: 0;
    right: 0;
    top: {top_chrome_h:.1}px;
    height: 3px;
    background-color: #1E82C4;
  }}
  .menu {{
    position: absolute;
    top: 116px;
    right: 8px;
    width: 210px;
    background-color: {menu_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1.5px solid #222222;
    border-bottom: 1.5px solid #222222;
    padding: 6px 8px;
  }}
  .menu-item {{
    height: 24px;
    padding: 0 6px;
    font-size: 12px;
    font-weight: 600;
    line-height: 24px;
    color: {ink};
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }}
  .menu-item span {{
    pointer-events: none;
  }}
  .menu-sep {{
    height: 1px;
    background-color: {separator};
    margin: 3px 6px;
  }}
  .supbar {{
    position: absolute;
    left: {content_x:.1}px;
    right: 1px;
    bottom: 16px;
    height: 36px;
    background-color: {nav};
    border-top: 1px solid {hairline};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 12px;
    gap: 8px;
  }}
  .sup-title {{
    color: {ink};
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }}
  .sup-state {{
    height: 20px;
    padding: 0 8px;
    border-radius: 4px;
    background-color: {pill};
    color: {ink};
    font-size: 11px;
    line-height: 20px;
    white-space: nowrap;
  }}
  .sup-detail {{
    flex: 1;
    color: {muted};
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
  }}
  .sup-btn {{
    height: 24px;
    padding: 0 10px;
    background-color: {pill};
    border: 1px solid {hairline};
    color: {ink};
    font-size: 11px;
    font-weight: 500;
    line-height: 22px;
    white-space: nowrap;
  }}
  .sup-approve {{
    background-color: {accent};
    color: #FFFFFF;
  }}
  .sup-stop {{
    color: #FF3B30;
  }}
  .xray-box {{
    position: absolute;
    border: 1.5px solid #0A84FF;
    background-color: rgba(10, 132, 255, 0.06);
    pointer-events: none;
  }}
  .xray-vault {{
    border: 2px solid #FF3B30;
    background-color: rgba(255, 59, 48, 0.14);
  }}
  .xray-freeze {{
    border: 2px solid #30D158;
    background-color: rgba(48, 209, 88, 0.14);
  }}
  .xray-pill {{
    display: inline-block;
    background-color: #0A84FF;
    color: #FFFFFF;
    font-size: 9px;
    font-weight: 700;
    line-height: 13px;
    padding: 0 4px;
    white-space: nowrap;
  }}
  .xray-pill-vault {{
    background-color: #FF3B30;
    color: #FFFFFF;
  }}
  .xray-pill-freeze {{
    background-color: #1B8A38;
    color: #FFFFFF;
  }}
  .xray-hud {{
    position: absolute;
    right: 1px;
    bottom: 16px;
    height: 24px;
    background-color: #111318;
    border-top: 1px solid #2C313C;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 12px;
    gap: 14px;
  }}
  .xray-hud-brand {{
    color: #5AC8FA;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }}
  .xray-hud-metric {{
    color: #E5E5EA;
    font-size: 10px;
    font-weight: 500;
    white-space: nowrap;
  }}
  .xray-hud-recall {{
    color: #30D158;
    font-weight: 700;
  }}
</style>
</head>
<body>
  {top_bars}
  {explorer_drawer}
  {explorer_rail}
  {bottom_aqua_bar}
  {progress}
  {xray_layer}
  {supbar}
  {menu}
</body>
</html>"#,
        surface_height = state.surface_height,
        nav = p.nav,
        hairline = p.hairline,
        bevel_light = p.bevel_light,
        pill = p.pill,
        ink = p.ink,
        muted = p.muted,
        menu_bg = p.menu_bg,
        separator = p.separator,
        accent = p.accent,
        rail_bg = p.rail_bg,
        drawer_bg = p.drawer_bg,
        drawer_header_bg = p.drawer_header_bg,
        link_blue = p.link_blue,
        top_chrome_h = top_chrome_h,
        content_x = content_x,
        top_bars = top_bars,
        explorer_drawer = explorer_drawer,
        explorer_rail = explorer_rail,
        bottom_aqua_bar = bottom_aqua_bar,
        progress = progress,
        xray_layer = xray_layer,
        supbar = supbar,
        menu = menu,
    )
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
