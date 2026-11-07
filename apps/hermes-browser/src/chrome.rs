//! The browser chrome, rendered by the same HTML/CSS engine as the page.
//!
//! Faithfully implements the Internet Explorer 5 Macintosh Edition Aqua chrome
//! and Collapsible Explorer Bar shown in the reference screenshots, scaled cleanly
//! for modern desktop windows:
//! - Multi-row IE5 Mac Aqua top chrome (`CHROME_HEIGHT = 128.0`):
//!   1. Classic Mac OS Platinum/Aqua title & tab bar (`26px`) with window collapse
//!      box, 6-line horizontal grip ridges across the bar, tabs (`#tab-N`,
//!      `#tab-close-N`, `#btn-newtab`), and right window zoom/shade boxes.
//!   2. Fluted/ribbed horizontal pinstripe Aqua button bar (`48px`) with 3D glossy
//!      Aqua Blue icons (`Back`, `Forward`, `Stop`, `Refresh`, `Home`, beaded
//!      divider, `AutoFill`, `Print`, `Mail`, `Sherlock`) and the 3D Aqua `e`
//!      emblem on the far right.
//!   3. Full-height 3D cylindrical Aqua Blue `Address :` bar (`30px`) with `@`
//!      swirl badge, sunken white URL input (`#address`), and `▸ go` button
//!      (`#btn-go`) on the right.
//!   4. Fluted/ribbed horizontal pinstripe Favorites bar (`24px`: `@ Web Gallery`,
//!      `@ Office for Macintosh`, `@ Products for Mac`, `@ Microsoft Internet`,
//!      `@ Apple`, `@ Google`, `@ Developer Hub`, plus Aqua `»` chevron `#btn-fav-overflow`
//!      on the right).
//! - Left vertical Explorer Bar rail (`EXPLORER_RAIL_WIDTH = 36.0`) with
//!   `#btn-collapse-toolbar` (3D Aqua Blue `◀|||` / `|||▶`) and five interlocking
//!   angled folder tabs (`Favorites`, `History`, `Search`, `Scrapbook`,
//!   `Page Holder`) with horizontal fluted pinstripes and 90°-rotated sideways
//!   labels. When an Explorer Bar drawer (`EXPLORER_DRAWER_WIDTH = 286.0`) is open,
//!   the drawer sits on the left (`x = 0..286`) and the vertical tab rail attaches
//!   to the right edge of the drawer (`x = 286..322`), with the active tab
//!   merging seamlessly into the drawer and highlighted in bold Aqua Blue.

use std::fmt::Write;

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
    menu_bg: &'static str,
    separator: &'static str,
    accent: &'static str,
    aqua_badge: &'static str,
    rail_bg: &'static str,
    rail_tab_bg: &'static str,
    drawer_bg: &'static str,
    drawer_header_bg: &'static str,
    brand_teal: &'static str,
    link_blue: &'static str,
    rib_base: &'static str,
    rib_hi: &'static str,
    rib_lo: &'static str,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            strip: "#D4D4D4",
            tab_active: "#F2F2F2",
            nav: "#E6E6E6",
            hairline: "#555555",
            bevel_light: "#FFFFFF",
            bevel_dark: "#6A6A6A",
            pill: "#FFFFFF",
            pill_focus: "#FFFFFF",
            ink: "#111111",
            muted: "#727E88",
            menu_bg: "#E6E6E6",
            separator: "#B0B0B0",
            accent: "#16689E",
            aqua_badge: "#2476AE",
            rail_bg: "#D8D8D8",
            rail_tab_bg: "#E6E6E6",
            drawer_bg: "#EAEAEA",
            drawer_header_bg: "#DEDEDE",
            brand_teal: "#146696",
            link_blue: "#0000EE",
            rib_base: "#E6E6E6",
            rib_hi: "#F7F7F7",
            rib_lo: "#D2D2D2",
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
            muted: "#7E8894",
            menu_bg: "#2C2C2E",
            separator: "#3A3A3C",
            accent: "#38A3F8",
            aqua_badge: "#1E5B88",
            rail_bg: "#222225",
            rail_tab_bg: "#2C2C30",
            drawer_bg: "#1C1C1E",
            drawer_header_bg: "#28282B",
            brand_teal: "#58B4F8",
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

/// A passkey ceremony a page started, waiting for a human to allow or deny
/// it.
///
/// This is the browser-chrome half of the interstitial. The glasses DAT card
/// is the primary surface; this mirrors the same decision in the window the
/// person is already looking at.
#[derive(Clone, Debug, Default)]
pub struct PasskeyPrompt {
    /// The site that is asking.
    pub origin: String,
    /// The relying party the ceremony is for.
    pub rp_id: String,
    /// The account the ceremony would use, when the browser knows one.
    pub account_hint: Option<String>,
    /// The prompt text the broker produced.
    pub prompt: String,
}

/// How long a page ceremony may wait for a human before it fails.
pub const PASSKEY_PROMPT_TIMEOUT_SECS: u64 = 60;

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
    /// A page-initiated passkey ceremony waiting for a human.
    pub passkey_prompt: Option<PasskeyPrompt>,
}

/// Total height of the IE5 Mac top chrome scaled for modern desktop windows:
/// Title/Tab Strip (26) + Ribbed Aqua Button Bar (48) + Cylindrical Aqua Address Bar (30) + Ribbed Favorites Strip (24) = 128.0.
pub const CHROME_HEIGHT: f64 = 128.0;
/// Width of the vertical IE5 Mac Explorer Bar folder-tab rail.
pub const EXPLORER_RAIL_WIDTH: f64 = 28.0;
/// Width of the collapsible Explorer Bar drawer panel when open.
pub const EXPLORER_DRAWER_WIDTH: f64 = 286.0;
/// Height of the Page Holder drawer's top control header bar.
pub const PH_HEADER_HEIGHT: f64 = 52.0;

/// Generates a `<div class="rib-bg">` wrapping an SVG with explicit horizontal fluted/ribbed
/// Aqua pinstripe `<path>` lines (`anyrender_svg` renders `<path>` strokes directly).
fn ribbed_rect_svg(width: u32, height: u32, base: &str, hi: &str, lo: &str) -> String {
    let mut hi_path = String::new();
    let mut lo_path = String::new();
    let mut y = 1u32;
    while y < height {
        let _ = write!(hi_path, "M0,{}.5 H{} ", y, width);
        if y + 1 < height {
            let _ = write!(lo_path, "M0,{}.5 H{} ", y + 1, width);
        }
        y += 3;
    }
    format!(
        r##"<div class="rib-bg"><svg width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="{width}" height="{height}" fill="{base}"/><path d="{hi_path}" stroke="{hi}" stroke-width="1"/><path d="{lo_path}" stroke="{lo}" stroke-width="1"/></svg></div>"##
    )
}

/// Generates an interlocking angled IE5 Mac Explorer Bar folder tab (`27x90px`) wrapped in
/// `<div class="exp-tab-bg">` with horizontal fluted/ribbed pinstripes and authentic shoulders:
/// - Inactive boundary lines slope UP-RIGHT (`/`, from `(0, 90)` to `(26, 77)`).
/// - The active tab (`active == true`) pops to the front: its top shoulder slopes
///   DOWN-RIGHT (`\`, from `(0, 0)` to `(26, 13)`) and its bottom shoulder slopes
///   UP-RIGHT (`/`, from `(0, 90)` to `(26, 77)`), while the tab immediately above
///   it (`next_active == true`) tucks behind that `\` shoulder (`(0, 77)` to `(26, 90)`).
fn folder_tab_svg(
    first_tab: bool,
    active: bool,
    next_active: bool,
    drawer_open: bool,
    base: &str,
    hi: &str,
    lo: &str,
    active_base: &str,
) -> String {
    let fill_col = if active { active_base } else { base };

    let (tl_y, tr_y) = if first_tab {
        (0u32, 0u32)
    } else if active {
        (0u32, 13u32)
    } else {
        (13u32, 0u32)
    };

    let (bl_y, br_y) = if next_active {
        (77u32, 90u32)
    } else {
        (90u32, 77u32)
    };

    let poly_pts = format!("0,{tl_y} 26,{tr_y} 26,{br_y} 0,{bl_y}");

    let mut ribs = String::new();
    let rib_hi = if active { "#FAFAFA" } else { hi };
    let rib_lo = if active { "#DCDCDC" } else { lo };
    let mut y = 2u32;
    while y < 89 {
        let yf = y as f64;
        let mut x_left = 1u32;
        let mut x_right = 25u32;
        if tl_y == 13 && tr_y == 0 && yf < 13.0 {
            x_left = ((13.0 - yf) * (26.0 / 13.0)).ceil() as u32;
        } else if tl_y == 0 && tr_y == 13 && yf < 13.0 {
            x_right = ((yf * (26.0 / 13.0)).floor() as u32).min(25);
        }
        if bl_y == 90 && br_y == 77 && yf > 77.0 {
            x_right = x_right.min(((90.0 - yf) * (26.0 / 13.0)).floor() as u32);
        } else if bl_y == 77 && br_y == 90 && yf > 77.0 {
            x_left = x_left.max(((yf - 77.0) * (26.0 / 13.0)).ceil() as u32);
        }
        if x_right > x_left + 1 {
            let stroke_col = if (y / 2) % 2 == 0 { rib_hi } else { rib_lo };
            let _ = write!(
                ribs,
                r#"<line x1="{x_left}" y1="{y}.5" x2="{x_right}" y2="{y}.5" stroke="{stroke_col}" stroke-width="1"/>"#
            );
        }
        y += 2;
    }

    let left_border = if active && drawer_open {
        String::new()
    } else {
        format!(
            r##"<line x1="0.5" y1="{tl_y}" x2="0.5" y2="{bl_y}" stroke="#333333" stroke-width="1.1"/><line x1="1.5" y1="{tl_y}" x2="1.5" y2="{bl_y}" stroke="#FFFFFF" stroke-width="1"/>"##
        )
    };

    let top_border = if first_tab {
        r##"<line x1="0" y1="0.5" x2="26" y2="0.5" stroke="#444444" stroke-width="1.2"/><line x1="0" y1="1.5" x2="25" y2="1.5" stroke="#FFFFFF" stroke-width="1"/>"##.to_string()
    } else if active {
        r##"<line x1="0" y1="0" x2="26" y2="13" stroke="#333333" stroke-width="1.3"/><line x1="1" y1="1.5" x2="25" y2="13.5" stroke="#FFFFFF" stroke-width="1.1"/>"##.to_string()
    } else {
        r##"<line x1="0" y1="13" x2="26" y2="0" stroke="#444444" stroke-width="1.2"/><line x1="1" y1="13.8" x2="25.5" y2="1.2" stroke="#FFFFFF" stroke-width="1"/>"##.to_string()
    };

    let bottom_border = if next_active {
        r##"<line x1="0" y1="77" x2="26" y2="90" stroke="#333333" stroke-width="1.3"/>"##
    } else {
        r##"<line x1="0" y1="90" x2="26" y2="77" stroke="#333333" stroke-width="1.3"/><line x1="1" y1="88.6" x2="25" y2="76.2" stroke="#B5B5B5" stroke-width="1"/>"##
    };

    format!(
        r##"<div class="exp-tab-bg"><svg width="27" height="90" viewBox="0 0 27 90"><polygon points="{poly_pts}" fill="{fill_col}"/>{ribs}{top_border}{left_border}<line x1="26" y1="{tr_y}" x2="26" y2="{br_y}" stroke="#333333" stroke-width="1.2"/><line x1="24.8" y1="{tr_y}" x2="24.8" y2="{br_y}" stroke="#FFFFFF" stroke-width="0.9"/>{bottom_border}</svg></div>"##
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
    // In IE5 Mac (Figure 6), when the Explorer Bar drawer is open, the drawer
    // occupies x = 0..EXPLORER_DRAWER_WIDTH and the vertical folder-tab rail
    // attaches to the right edge of the drawer at x = EXPLORER_DRAWER_WIDTH.
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

    // Classic IE5 Mac 3D silver circular `@` badge (`16x16px`): raised metallic disc with a clean,
    // high-contrast `@` glyph (open spiral tail on the right so it never looks like concentric rings).
    let at_swirl_svg = r##"<svg class="at-svg" width="16" height="16" viewBox="0 0 16 16"><defs><radialGradient id="atg" cx="30%" cy="25%" r="75%"><stop offset="0%" stop-color="#FFFFFF"/><stop offset="60%" stop-color="#E0E0E0"/><stop offset="100%" stop-color="#9A9A9A"/></radialGradient></defs><circle cx="8" cy="8" r="6.8" fill="url(#atg)" stroke="#707070" stroke-width="0.9"/><path d="M9.7,6.0 L9.7,9.1 C9.7,9.8 10.6,9.9 11.2,9.2 C11.9,8.4 12.0,6.9 11.4,5.6 C10.5,3.9 8.3,3.2 6.3,4.0 C4.2,4.9 3.4,7.3 4.2,9.5 C5.0,11.6 7.3,12.5 9.7,11.9" fill="none" stroke="#1F1F1F" stroke-width="1.2" stroke-linecap="round"/><path d="M9.6,6.6 C9.1,5.9 8.3,5.6 7.5,5.7 C6.2,5.9 5.5,7.0 5.7,8.3 C5.9,9.5 6.9,10.2 8.1,10.0 C8.8,9.9 9.3,9.5 9.6,8.9" fill="none" stroke="#1F1F1F" stroke-width="1.2" stroke-linecap="round"/></svg>"##;

    // A key glyph for the passkey interstitial.
    let key_svg = r##"<svg class="pk-key-svg" width="44" height="44" viewBox="0 0 44 44"><defs><linearGradient id="pkg" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#F6D98A"/><stop offset="55%" stop-color="#D9A93C"/><stop offset="100%" stop-color="#A8791F"/></linearGradient></defs><circle cx="22" cy="15" r="9.5" fill="none" stroke="url(#pkg)" stroke-width="4"/><path d="M22 24.5 L22 39" stroke="url(#pkg)" stroke-width="4" stroke-linecap="round"/><path d="M22 31.5 L29 31.5" stroke="url(#pkg)" stroke-width="4" stroke-linecap="round"/><path d="M22 36 L27.5 36" stroke="url(#pkg)" stroke-width="4" stroke-linecap="round"/></svg>"##;

    let mut tabs = String::new();
    for (index, tab) in state.tabs.iter().enumerate() {
        let class = if tab.active { "tab tab-active" } else { "tab" };
        tabs.push_str(&format!(
            r#"<div class="{class}" id="tab-{index}">
                 {at_swirl_svg}
                 <div class="tab-title">{title}</div>
                 <div class="tab-close" id="tab-close-{index}">&#215;</div>
               </div>"#,
            class = class,
            index = index,
            at_swirl_svg = at_swirl_svg,
            title = escape_html(tab.title),
        ));
    }

    // Horizontal Favorites Bar items rendered directly from `state.favorites`
    let mut fav_strip_items = String::new();
    for (i, fav) in state.favorites.iter().take(7).enumerate() {
        fav_strip_items.push_str(&format!(
            r#"<div class="fav-pill" id="fav-bar-{i}" data-url="{url}">{at_swirl_svg}<span class="fav-pill-text">{title}</span></div>"#,
            i = i,
            url = escape_html(fav.url),
            at_swirl_svg = at_swirl_svg,
            title = escape_html(fav.title),
        ));
    }

    let secure_attr = if state.secure { "https" } else { "http" };

    // Fluted / ribbed horizontal pinstripe SVG backgrounds wrapped in `<div class="rib-bg">`
    let title_rib_bg = ribbed_rect_svg(2400, 26, "#EAEAEA", "#F8F8F8", "#DCDCDC");
    let nav_rib_bg = ribbed_rect_svg(2400, 48, p.rib_base, p.rib_hi, p.rib_lo);
    let fav_rib_bg = ribbed_rect_svg(2400, 24, p.rib_base, p.rib_hi, p.rib_lo);

    // 3D Cylindrical Deep Bondi-Teal Aqua tube SVG background for the 30px Address bar row
    let addr_aqua_tube_bg = r##"<div class="rib-bg"><svg width="2400" height="30" viewBox="0 0 2400 30"><defs><linearGradient id="aquatube" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#0E4463"/><stop offset="22%" stop-color="#3784AB"/><stop offset="44%" stop-color="#88C8E4"/><stop offset="60%" stop-color="#3B89B0"/><stop offset="86%" stop-color="#14567A"/><stop offset="100%" stop-color="#092E46"/></linearGradient></defs><rect width="2400" height="30" fill="url(#aquatube)"/><line x1="0" y1="0.5" x2="2400" y2="0.5" stroke="#062133" stroke-width="1"/><line x1="0" y1="29.5" x2="2400" y2="29.5" stroke="#062133" stroke-width="1"/></svg></div>"##;

    // Mac OS X Aqua Oblong Toolbar-Toggle Pill Button on the right of the Title Bar
    let aqua_pill_svg = r##"<svg width="24" height="12" viewBox="0 0 24 12"><defs><linearGradient id="aqpil" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#FFFFFF"/><stop offset="55%" stop-color="#DCE4EC"/><stop offset="100%" stop-color="#AEBCC8"/></linearGradient></defs><rect x="1" y="1" width="22" height="10" rx="5" fill="url(#aqpil)" stroke="#52606C" stroke-width="1.1"/><ellipse cx="12" cy="4" rx="7" ry="1.8" fill="#FFFFFF" fill-opacity="0.85"/></svg>"##;

    // 3D Translucent Bondi-Teal Aqua Glass SVG Icons for the top toolbar (`media_1794020340993_5f6a11ae.jpg`)
    let back_icon_svg = if state.can_back {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="abk" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#8AE0FA"/><stop offset="48%" stop-color="#2286AE"/><stop offset="100%" stop-color="#0B4663"/></linearGradient></defs><path d="M19,3.5 C20.5,2.5 22,3.5 22,5.5 L22,18.5 C22,20.5 20.5,21.5 19,20.5 L6.5,13.5 C5,12.5 5,11.5 6.5,10.5 Z" fill="url(#abk)" stroke="#093B54" stroke-width="1.2"/><path d="M19.5,5 L8,11.5 L19.5,9.5 Z" fill="#D4F4FF" fill-opacity="0.7"/></svg>"##
    } else {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="abkd" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#DAE6EC"/><stop offset="55%" stop-color="#B0C4CE"/><stop offset="100%" stop-color="#92A8B4"/></linearGradient></defs><path d="M19,3.5 C20.5,2.5 22,3.5 22,5.5 L22,18.5 C22,20.5 20.5,21.5 19,20.5 L6.5,13.5 C5,12.5 5,11.5 6.5,10.5 Z" fill="url(#abkd)" stroke="#859AA6" stroke-width="1.1"/><path d="M19.5,5 L8,11.5 L19.5,9.5 Z" fill="#F0F7FA" fill-opacity="0.65"/></svg>"##
    };

    let fwd_icon_svg = if state.can_forward {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="afw" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#8AE0FA"/><stop offset="48%" stop-color="#2286AE"/><stop offset="100%" stop-color="#0B4663"/></linearGradient></defs><path d="M9,3.5 C7.5,2.5 6,3.5 6,5.5 L6,18.5 C6,20.5 7.5,21.5 9,20.5 L21.5,13.5 C23,12.5 23,11.5 21.5,10.5 Z" fill="url(#afw)" stroke="#093B54" stroke-width="1.2"/><path d="M8.5,5 L20,11.5 L8.5,9.5 Z" fill="#D4F4FF" fill-opacity="0.7"/></svg>"##
    } else {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="afwd" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#DAE6EC"/><stop offset="55%" stop-color="#B0C4CE"/><stop offset="100%" stop-color="#92A8B4"/></linearGradient></defs><path d="M9,3.5 C7.5,2.5 6,3.5 6,5.5 L6,18.5 C6,20.5 7.5,21.5 9,20.5 L21.5,13.5 C23,12.5 23,11.5 21.5,10.5 Z" fill="url(#afwd)" stroke="#859AA6" stroke-width="1.1"/><path d="M8.5,5 L20,11.5 L8.5,9.5 Z" fill="#F0F7FA" fill-opacity="0.65"/></svg>"##
    };

    let stop_icon_svg = if state.loading {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="astp" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#8AE0FA"/><stop offset="50%" stop-color="#2286AE"/><stop offset="100%" stop-color="#0B4663"/></linearGradient></defs><path d="M7,3.5 Q14,9.5 21,3.5 Q15.5,12 21,20.5 Q14,14.5 7,20.5 Q12.5,12 7,3.5 Z" fill="url(#astp)" stroke="#093B54" stroke-width="1.2"/></svg>"##
    } else {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="astpd" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#DAE6EC"/><stop offset="55%" stop-color="#B0C4CE"/><stop offset="100%" stop-color="#92A8B4"/></linearGradient></defs><path d="M7,3.5 Q14,9.5 21,3.5 Q15.5,12 21,20.5 Q14,14.5 7,20.5 Q12.5,12 7,3.5 Z" fill="url(#astpd)" stroke="#859AA6" stroke-width="1.1"/></svg>"##
    };

    let refresh_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="arf" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#8AE0FA"/><stop offset="48%" stop-color="#2286AE"/><stop offset="100%" stop-color="#0B4663"/></linearGradient></defs><path d="M7,12 C7,7.2 11,4 15.5,4 C17.8,4 19.8,4.8 21.2,6.2 L23.5,4 L23.5,10.5 L17,10.5 L19.2,8.3 C18.2,7.3 16.9,6.8 15.5,6.8 C12.4,6.8 10,9.1 10,12 Z" fill="url(#arf)" stroke="#093B54" stroke-width="1"/><path d="M21,12 C21,16.8 17,20 12.5,20 C10.2,20 8.2,19.2 6.8,17.8 L4.5,20 L4.5,13.5 L11,13.5 L8.8,15.7 C9.8,16.7 11.1,17.2 12.5,17.2 C15.6,17.2 18,14.9 18,12 Z" fill="url(#arf)" stroke="#093B54" stroke-width="1"/></svg>"##;

    // Arrow 1 Target: IE5 Mac Bondi-Teal Aqua Glass `Home` icon with dark teal roof chevron,
    // right chimney, translucent aqua house walls, white left window square, and dark teal right door!
    let home_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="ahmw" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#9BE4FA"/><stop offset="55%" stop-color="#389EC2"/><stop offset="100%" stop-color="#146485"/></linearGradient><linearGradient id="ahmr" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#2A88AC"/><stop offset="100%" stop-color="#093C55"/></linearGradient></defs><rect x="18" y="4" width="3" height="5.5" fill="#0B4663" stroke="#072E42" stroke-width="0.8"/><polygon points="14,3.5 5.5,11 7,11 7,20.5 21,20.5 21,11 22.5,11" fill="url(#ahmw)" stroke="#093D57" stroke-width="1.1"/><polygon points="14,2.2 3.2,11.5 5.8,13.2 14,5.8 22.2,13.2 24.8,11.5" fill="url(#ahmr)" stroke="#072E42" stroke-width="0.9"/><rect x="8.8" y="12.2" width="4.2" height="4.2" fill="#FFFFFF" stroke="#0B4663" stroke-width="1"/><rect x="14.8" y="12.5" width="4.2" height="8" fill="#093D57"/></svg>"##;

    let autofill_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="aaf" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#DAE6EC"/><stop offset="55%" stop-color="#A8C0CC"/><stop offset="100%" stop-color="#829CA8"/></linearGradient></defs><path d="M16,2.5 C19.5,2.5 22,5.5 20.5,9.5 L12.5,19.5 L7.5,16.5 L12.5,5.5 C13.5,3.5 14.5,2.5 16,2.5 Z" fill="url(#aaf)" stroke="#78909C" stroke-width="1.1"/><ellipse cx="16.2" cy="6.8" rx="2.4" ry="1.5" fill="#F2F8FA" fill-opacity="0.8"/><line x1="6" y1="20.5" x2="18" y2="20.5" stroke="#859AA6" stroke-width="1.2"/></svg>"##;

    let print_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="apr" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#DAE6EC"/><stop offset="55%" stop-color="#A8C0CC"/><stop offset="100%" stop-color="#829CA8"/></linearGradient></defs><rect x="8" y="2.5" width="12" height="6.5" fill="#F4F8FA" stroke="#8298A4" stroke-width="1"/><rect x="4.5" y="8" width="19" height="9.5" rx="2" fill="url(#apr)" stroke="#78909C" stroke-width="1.1"/><rect x="7" y="15" width="14" height="6" fill="#F4F8FA" stroke="#8298A4" stroke-width="1"/></svg>"##;

    let mail_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="aml" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#88DCF8"/><stop offset="55%" stop-color="#288BB0"/><stop offset="100%" stop-color="#0D4D6B"/></linearGradient></defs><rect x="4" y="5" width="20" height="14" rx="1.5" fill="url(#aml)" stroke="#093B54" stroke-width="1.2"/><polygon points="4.8,5.5 14,13.2 23.2,5.5" fill="#CEEFFB" stroke="#093B54" stroke-width="1"/><polyline points="4.8,18.5 11.5,12 14,13.8 16.5,12 23.2,18.5" fill="none" stroke="#0B435E" stroke-width="1"/></svg>"##;

    // Arrow 2 Target: Sculpted 3D Bondi-Teal Aqua lowercase `e` logo (`38x36px`) on the right of the toolbar
    // with open counter eye, open lower-right gap, orbital swoosh ring, and glossy specular highlight!
    let ie_aqua_emblem_svg = r##"<svg width="38" height="36" viewBox="0 0 38 36"><defs><linearGradient id="iee" x1="0.15" y1="0.05" x2="0.85" y2="0.95"><stop offset="0%" stop-color="#9EE8FF"/><stop offset="38%" stop-color="#38A2CA"/><stop offset="78%" stop-color="#146B90"/><stop offset="100%" stop-color="#093C56"/></linearGradient><linearGradient id="iering" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#B8F0FF"/><stop offset="50%" stop-color="#2E96BE"/><stop offset="100%" stop-color="#0B4766"/></linearGradient></defs><path d="M5,25 C1.5,21 6,12 16,7 C26,2 35,4 36,8.5 C37,12 31,17 24,20.5" fill="none" stroke="url(#iering)" stroke-width="2.2" stroke-linecap="round"/><path fill-rule="evenodd" d="M31.5,18.5 C31.8,10.5 26.2,4.8 18.8,4.8 C10.8,4.8 5.2,10.8 5.2,18.5 C5.2,26.5 11.0,32.0 19.2,32.0 C24.8,32.0 29.2,29.4 31.2,25.0 L25.4,22.8 C24.0,25.4 21.8,26.8 19.0,26.8 C14.8,26.8 11.8,24.0 11.5,19.8 L31.4,19.8 Z M11.6,15.2 C12.2,11.5 15.0,9.4 18.8,9.4 C22.5,9.4 25.2,11.6 25.6,15.2 Z" fill="url(#iee)" stroke="#083A54" stroke-width="1.1"/><path d="M9.5,11.5 C12,7.8 16,6.2 20.5,6.5" fill="none" stroke="#E0F8FF" stroke-width="1.8" stroke-linecap="round"/><path d="M3.5,23.5 C6.5,27.5 16,25 26,19.5" fill="none" stroke="#7ED8F8" stroke-width="1.8" stroke-linecap="round"/></svg>"##;

    let beaded_sep_svg = r##"<svg class="nav-sep-svg" width="6" height="38" viewBox="0 0 6 38"><circle cx="3" cy="3" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="7" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="11" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="15" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="19" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="23" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="27" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="31" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="35" r="1.1" fill="#6E6E6E"/></svg>"##;

    let top_bars = if state.toolbar_collapsed {
        String::new()
    } else {
        format!(
            r##"<div class="tabstrip">
                 {title_rib_bg}
                 <div class="title-tabs-wrap">
                   {tabs}
                   <div class="newtab" id="btn-newtab">+</div>
                 </div>
                 <div class="aqua-pill-btn" id="btn-menu">{aqua_pill_svg}</div>
               </div>
               <div class="nav">
                 {nav_rib_bg}
                 <div class="{back_class}" id="btn-back">
                   {back_icon_svg}
                   <div class="btn-lbl">Back</div>
                 </div>
                 <div class="{forward_class}" id="btn-forward">
                   {fwd_icon_svg}
                   <div class="btn-lbl">Forward</div>
                 </div>
                 <div class="{stop_class}" id="btn-stop">
                   {stop_icon_svg}
                   <div class="btn-lbl">Stop</div>
                 </div>
                 <div class="btn" id="btn-reload">
                   {refresh_icon_svg}
                   <div class="btn-lbl">Refresh</div>
                 </div>
                 <div class="btn" id="btn-home">
                   {home_icon_svg}
                   <div class="btn-lbl">Home</div>
                 </div>
                 <div class="nav-sep">{beaded_sep_svg}</div>
                 <div class="btn" id="btn-autofill">
                   {autofill_icon_svg}
                   <div class="btn-lbl">AutoFill</div>
                 </div>
                 <div class="btn" id="btn-print">
                   {print_icon_svg}
                   <div class="btn-lbl">Print</div>
                 </div>
                 <div class="btn" id="btn-mail">
                   {mail_icon_svg}
                   <div class="btn-lbl">Mail</div>
                 </div>
                 <div class="ie-emblem">{ie_aqua_emblem_svg}</div>
               </div>
               <div class="address-row">
                 {addr_aqua_tube_bg}
                 <div class="address-badge">Address:</div>
                 <div class="{bar_class}" id="address" data-scheme="{secure_attr}">
                   {at_swirl_svg}
                   <div class="url">{url}</div>
                   {caret}
                 </div>
                 <div class="go-btn" id="btn-go">
                   <svg width="8" height="10" viewBox="0 0 8 10"><polygon points="1,1 7,5 1,9" fill="#FFFFFF"/></svg>
                   <span class="go-lbl">go</span>
                 </div>
               </div>
               <div class="favbar">
                 {fav_rib_bg}
                 {fav_strip_items}
                 <div class="fav-chevron" id="btn-fav-overflow">&#187;</div>
               </div>"##,
            title_rib_bg = title_rib_bg,
            tabs = tabs,
            aqua_pill_svg = aqua_pill_svg,
            nav_rib_bg = nav_rib_bg,
            back_class = back_class,
            forward_class = forward_class,
            stop_class = stop_class,
            back_icon_svg = back_icon_svg,
            fwd_icon_svg = fwd_icon_svg,
            stop_icon_svg = stop_icon_svg,
            refresh_icon_svg = refresh_icon_svg,
            home_icon_svg = home_icon_svg,
            beaded_sep_svg = beaded_sep_svg,
            autofill_icon_svg = autofill_icon_svg,
            print_icon_svg = print_icon_svg,
            mail_icon_svg = mail_icon_svg,
            ie_aqua_emblem_svg = ie_aqua_emblem_svg,
            addr_aqua_tube_bg = addr_aqua_tube_bg,
            bar_class = bar_class,
            secure_attr = secure_attr,
            at_swirl_svg = at_swirl_svg,
            url = escape_html(state.url),
            caret = caret,
            fav_rib_bg = fav_rib_bg,
            fav_strip_items = fav_strip_items,
        )
    };

    // Compact 3D Aqua navigation buttons rendered inside the left Explorer Bar rail when the top toolbar is collapsed (PDF Page 2 right).
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
        format!(
            r##"<div class="{b_cls}" id="btn-back">{back_icon_svg}</div>
               <div class="{f_cls}" id="btn-forward">{fwd_icon_svg}</div>
               <div class="{s_cls}" id="btn-stop">{stop_icon_svg}</div>
               <div class="rail-icon" id="btn-reload">{refresh_icon_svg}</div>"##,
            b_cls = b_cls,
            f_cls = f_cls,
            s_cls = s_cls,
            back_icon_svg = back_icon_svg,
            fwd_icon_svg = fwd_icon_svg,
            stop_icon_svg = stop_icon_svg,
            refresh_icon_svg = refresh_icon_svg,
        )
    } else {
        String::new()
    };

    // Arrow 3 Target: 3D Bondi-Aqua `◀|||` / `|||▶` collapse toggle icon at the top of the slim 28px Explorer Bar rail
    let collapse_svg = if state.toolbar_collapsed {
        r##"<svg width="24" height="16" viewBox="0 0 24 16"><defs><linearGradient id="aclp" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#88E0FA"/><stop offset="50%" stop-color="#2286AE"/><stop offset="100%" stop-color="#093B56"/></linearGradient></defs><rect x="2" y="2.5" width="2.2" height="11" rx="0.8" fill="url(#aclp)" stroke="#073047" stroke-width="0.6"/><rect x="5.5" y="2.5" width="2.2" height="11" rx="0.8" fill="url(#aclp)" stroke="#073047" stroke-width="0.6"/><rect x="9" y="2.5" width="2.2" height="11" rx="0.8" fill="url(#aclp)" stroke="#073047" stroke-width="0.6"/><polygon points="13.5,2 22,8 13.5,14" fill="url(#aclp)" stroke="#073047" stroke-width="0.8"/></svg>"##
    } else {
        r##"<svg width="24" height="16" viewBox="0 0 24 16"><defs><linearGradient id="aclp" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#88E0FA"/><stop offset="50%" stop-color="#2286AE"/><stop offset="100%" stop-color="#093B56"/></linearGradient></defs><polygon points="10.5,2 2,8 10.5,14" fill="url(#aclp)" stroke="#073047" stroke-width="0.8"/><rect x="12.5" y="2.5" width="2.2" height="11" rx="0.8" fill="url(#aclp)" stroke="#073047" stroke-width="0.6"/><rect x="16" y="2.5" width="2.2" height="11" rx="0.8" fill="url(#aclp)" stroke="#073047" stroke-width="0.6"/><rect x="19.5" y="2.5" width="2.2" height="11" rx="0.8" fill="url(#aclp)" stroke="#073047" stroke-width="0.6"/></svg>"##
    };

    let is_fav = state.explorer_tab == Some(ExplorerTab::Favorites);
    let is_hist = state.explorer_tab == Some(ExplorerTab::History);
    let is_srch = state.explorer_tab == Some(ExplorerTab::Search);
    let is_scrp = state.explorer_tab == Some(ExplorerTab::Scrapbook);
    let is_ph = state.explorer_tab == Some(ExplorerTab::PageHolder);

    let fav_cls = exp_tab_class(is_fav);
    let hist_cls = exp_tab_class(is_hist);
    let srch_cls = exp_tab_class(is_srch);
    let scrp_cls = exp_tab_class(is_scrp);
    let ph_cls = if is_ph {
        "exp-tab exp-tab-last exp-tab-active"
    } else {
        "exp-tab exp-tab-last"
    };

    let fav_bg = folder_tab_svg(
        true,
        is_fav,
        is_hist,
        drawer_open,
        p.rib_base,
        p.rib_hi,
        p.rib_lo,
        p.drawer_bg,
    );
    let hist_bg = folder_tab_svg(
        false,
        is_hist,
        is_srch,
        drawer_open,
        p.rib_base,
        p.rib_hi,
        p.rib_lo,
        p.drawer_bg,
    );
    let srch_bg = folder_tab_svg(
        false,
        is_srch,
        is_scrp,
        drawer_open,
        p.rib_base,
        p.rib_hi,
        p.rib_lo,
        p.drawer_bg,
    );
    let scrp_bg = folder_tab_svg(
        false,
        is_scrp,
        is_ph,
        drawer_open,
        p.rib_base,
        p.rib_hi,
        p.rib_lo,
        p.drawer_bg,
    );
    let ph_bg = folder_tab_svg(
        false,
        is_ph,
        false,
        drawer_open,
        p.rib_base,
        p.rib_hi,
        p.rib_lo,
        p.drawer_bg,
    );

    let rail_rib_bg = ribbed_rect_svg(28, 1200, p.rib_base, p.rib_hi, p.rib_lo);
    let drawer_rib_bg = ribbed_rect_svg(286, 1200, p.rib_base, p.rib_hi, p.rib_lo);
    let rail_bottom_border_cls = if drawer_open {
        "rail-globe rail-globe-bordered"
    } else {
        "rail-globe"
    };

    let explorer_rail = format!(
        r##"<div class="exp-rail" id="exp-rail" style="top:{top_chrome_h:.1}px;left:{rail_left_x:.1}px;">
             {rail_rib_bg}
             <div class="rail-collapse" id="btn-collapse-toolbar">{collapse_svg}</div>
             {rail_compact_nav}
             <div class="{fav_cls}" id="exp-favorites">{fav_bg}<div class="exp-tab-label">Favorites</div></div>
             <div class="{hist_cls}" id="exp-history">{hist_bg}<div class="exp-tab-label">History</div></div>
             <div class="{srch_cls}" id="exp-search">{srch_bg}<div class="exp-tab-label">Search</div></div>
             <div class="{scrp_cls}" id="exp-scrapbook">{scrp_bg}<div class="exp-tab-label">Scrapbook</div></div>
             <div class="{ph_cls}" id="exp-pageholder">{ph_bg}<div class="exp-tab-label">Page Holder</div></div>
             <div class="{rail_bottom_border_cls}"></div>
           </div>"##,
    );

    // Camera-and-page icon used for Scrapbook entries (PDF Page 1 Figure 6).
    let sb_doc_icon = r##"<svg width="18" height="15" viewBox="0 0 18 15"><rect x="1" y="2.5" width="11" height="10" rx="1.8" fill="#7A8B99" stroke="#2F3E4C" stroke-width="1.1"/><circle cx="6.5" cy="7.5" r="2.6" fill="#D9E6F2" stroke="#2F3E4C" stroke-width="1"/><rect x="10" y="1" width="7" height="12.5" fill="#FFFFFF" stroke="#4A5568" stroke-width="1"/><line x1="11.5" y1="4.5" x2="15" y2="4.5" stroke="#286090" stroke-width="1"/><line x1="11.5" y1="7.5" x2="15" y2="7.5" stroke="#718096" stroke-width="1"/></svg>"##;
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
                         {at_swirl_svg}
                         <span class="sb-row-title">{title}</span>
                       </div>"#,
                    i = i,
                    url = escape_html(fav.url),
                    at_swirl_svg = at_swirl_svg,
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
                             {at_swirl_svg}
                             <span class="sb-row-title">{title}</span>
                           </div>"#,
                        i = i,
                        url = escape_html(entry.url),
                        at_swirl_svg = at_swirl_svg,
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

    // Arrow 4 Target: Mac OS X Aqua 15px Right & Bottom Scrollbar Tracks + 15x15 Bottom-Right Diagonal Resize Grip Box (`media_1794020340993_5f6a11ae.jpg`)
    let bottom_aqua_bar = format!(
        r##"<div class="aqua-vscroll" style="top:{top_chrome_h:.1}px;">
             <svg width="15" height="1200" viewBox="0 0 15 1200"><rect width="15" height="1200" fill="#F4F4F4"/><line x1="0.5" y1="0" x2="0.5" y2="1200" stroke="#9A9A9A" stroke-width="1"/></svg>
           </div>
           <div class="aqua-bottom-wrap" style="left:{content_x:.1}px;">
             <div class="aqua-hscroll">
               <svg width="2400" height="15" viewBox="0 0 2400 15"><rect width="2400" height="15" fill="#F4F4F4"/><line x1="0" y1="0.5" x2="2400" y2="0.5" stroke="#9A9A9A" stroke-width="1"/></svg>
             </div>
             <div class="aqua-resize-corner">
               <svg width="15" height="15" viewBox="0 0 15 15"><rect width="15" height="15" fill="#ECECEC"/><line x1="0.5" y1="0" x2="0.5" y2="15" stroke="#8C8C8C" stroke-width="1"/><line x1="0" y1="0.5" x2="15" y2="0.5" stroke="#8C8C8C" stroke-width="1"/><line x1="4" y1="13" x2="13" y2="4" stroke="#7A7A7A" stroke-width="1"/><line x1="5" y1="13" x2="13" y2="5" stroke="#FFFFFF" stroke-width="1"/><line x1="7" y1="13" x2="13" y2="7" stroke="#7A7A7A" stroke-width="1"/><line x1="8" y1="13" x2="13" y2="8" stroke="#FFFFFF" stroke-width="1"/><line x1="10" y1="13" x2="13" y2="10" stroke="#7A7A7A" stroke-width="1"/></svg>
             </div>
           </div>"##,
        top_chrome_h = top_chrome_h,
        content_x = content_x,
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
                 <div class="menu-item" id="menu-home">{at_swirl_svg}<span>Home</span></div>
                 <div class="menu-item" id="menu-reload">{at_swirl_svg}<span>Reload</span></div>
                 <div class="menu-sep"></div>
                 <div class="menu-item" id="menu-back">{at_swirl_svg}<span>Back</span></div>
                 <div class="menu-item" id="menu-forward">{at_swirl_svg}<span>Forward</span></div>
                 <div class="menu-sep"></div>
                 <div class="menu-item" id="menu-close-tab">{at_swirl_svg}<span>Close tab</span></div>
               </div>"#,
            at_swirl_svg = at_swirl_svg,
        )
    } else {
        String::new()
    };

    let _ = (p.strip, p.tab_active);

    // The passkey interstitial. A page asked for a credential, and a human
    // has to say yes before the ceremony signs anything. The modal is the
    // browser-chrome half of that prompt; the glasses DAT card is the
    // primary one.
    let passkey_prompt = match &state.passkey_prompt {
        Some(prompt) => {
            let origin = escape_html(if prompt.origin.is_empty() {
                &prompt.rp_id
            } else {
                &prompt.origin
            });
            let account = match &prompt.account_hint {
                Some(hint) if !hint.is_empty() => format!(
                    r#"<div class="pk-account" id="pk-account">{}</div>"#,
                    escape_html(hint)
                ),
                _ => String::new(),
            };
            format!(
                r#"<div class="pk-scrim" id="pk-scrim">
                     <div class="pk-modal" id="pk-modal" role="alertdialog" aria-modal="true" aria-labelledby="pk-title">
                       <div class="pk-icon" id="pk-icon">{key_svg}</div>
                       <div class="pk-title" id="pk-title">Use a passkey?</div>
                       <div class="pk-site" id="pk-site">{origin}</div>
                       {account}
                       <div class="pk-detail" id="pk-detail">{detail}</div>
                       <div class="pk-actions">
                         <div class="pk-btn pk-deny" id="pk-deny">Deny</div>
                         <div class="pk-btn pk-allow" id="pk-allow">Allow</div>
                       </div>
                     </div>
                   </div>"#,
                origin = origin,
                account = account,
                detail = escape_html(if prompt.prompt.is_empty() {
                    "This site is asking to use a passkey."
                } else {
                    &prompt.prompt
                }),
                key_svg = key_svg,
            )
        }
        None => String::new(),
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
  .at-svg, .ph-tri-svg {{
    flex-shrink: 0;
    pointer-events: none;
  }}

  /* ---- Row 1: Mac OS X Aqua Pinstriped Window Title & Tab Strip (26px) ---- */
  .tabstrip {{
    position: relative;
    height: 26px;
    background-color: #EAEAEA;
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid #7A7A7A;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 10px;
    gap: 8px;
    overflow: hidden;
  }}
  .traffic-lights {{
    position: relative;
    width: 54px;
    height: 16px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    pointer-events: none;
  }}
  .traffic-svg {{
    pointer-events: none;
  }}
  .title-tabs-wrap {{
    position: relative;
    flex: 1;
    height: 24px;
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 6px;
    overflow: hidden;
  }}
  .aqua-pill-btn {{
    position: relative;
    width: 28px;
    height: 16px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }}
  .aqua-pill-btn svg {{
    pointer-events: none;
  }}
  .tab {{
    height: 20px;
    max-width: 280px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    background-color: transparent;
    color: {ink};
    font-size: 12.5px;
    font-weight: 600;
    flex-shrink: 0;
  }}
  .tab-active {{
    color: #000000;
  }}
  .tab-title {{
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    line-height: 20px;
    pointer-events: none;
  }}
  .tab-close {{
    width: 15px;
    height: 15px;
    font-size: 12px;
    line-height: 14px;
    text-align: center;
    color: #555555;
  }}
  .newtab {{
    width: 20px;
    height: 18px;
    font-size: 13px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    color: #444444;
    background-color: #E2E2E2;
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid #777777;
    border-bottom: 1px solid #777777;
    flex-shrink: 0;
  }}

  /* ---- Row 2: IE5 Mac Fluted/Ribbed Aqua Icon Button Bar (48px) ---- */
  .nav {{
    position: relative;
    height: 48px;
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid #4A4A4A;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 18px;
    gap: 12px;
    overflow: hidden;
  }}
  .btn {{
    position: relative;
    min-width: 72px;
    height: 42px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 2px 10px;
    color: {ink};
    gap: 2px;
  }}
  .btn svg {{
    pointer-events: none;
  }}
  .btn-lbl {{
    font-size: 11.5px;
    line-height: 13px;
    font-weight: 600;
    text-align: center;
    white-space: nowrap;
    pointer-events: none;
  }}
  .btn-off {{
    color: {muted};
  }}
  .nav-sep {{
    position: relative;
    width: 10px;
    height: 38px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0 4px;
    pointer-events: none;
  }}
  .nav-sep-svg {{
    pointer-events: none;
  }}
  .ie-emblem {{
    position: relative;
    margin-left: auto;
    width: 38px;
    height: 38px;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }}

  /* ---- Row 3: IE5 Mac 3D Cylindrical Aqua Blue Address Bar (30px) ---- */
  .address-row {{
    position: relative;
    height: 30px;
    background-color: {aqua_badge};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 3px 6px 3px 0;
    overflow: hidden;
  }}
  .address-badge {{
    position: relative;
    width: 82px;
    height: 24px;
    color: #FFFFFF;
    font-size: 12px;
    font-weight: 500;
    line-height: 24px;
    text-align: right;
    padding-right: 9px;
    letter-spacing: 0.01em;
    pointer-events: none;
  }}
  .bar {{
    position: relative;
    flex: 1;
    height: 23px;
    background-color: {pill};
    border: 1.5px solid #081E2E;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 7px;
    gap: 6px;
  }}
  .bar-focused {{
    background-color: {pill_focus};
    border: 1.5px solid {accent};
  }}
  .url {{
    color: {ink};
    font-size: 12.5px;
    line-height: 21px;
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
  }}
  .caret {{
    width: 1.5px;
    height: 15px;
    background-color: {ink};
    pointer-events: none;
  }}
  .go-btn {{
    position: relative;
    width: 50px;
    height: 24px;
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 5px;
    margin-left: 4px;
    color: #FFFFFF;
    font-size: 12.5px;
    font-weight: 700;
  }}
  .go-btn svg, .go-lbl {{
    pointer-events: none;
  }}

  /* ---- Row 4: IE5 Mac Fluted/Ribbed Favorites Bar (24px) ---- */
  .favbar {{
    position: relative;
    height: 24px;
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 12px;
    gap: 18px;
    overflow: hidden;
  }}
  .fav-pill {{
    position: relative;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 5px;
    height: 20px;
    padding: 0 4px;
    font-size: 11.5px;
    font-weight: 500;
    color: {ink};
    white-space: nowrap;
  }}
  .fav-pill-text {{
    line-height: 20px;
    pointer-events: none;
  }}
  .fav-chevron {{
    position: relative;
    margin-left: auto;
    padding: 0 4px;
    color: {brand_teal};
    font-size: 16px;
    font-weight: 800;
    line-height: 20px;
  }}

  /* ---- Left Vertical Explorer Bar Rail (28px wide, Sideways Folder Tabs) ---- */
  .exp-rail {{
    position: absolute;
    bottom: 0;
    width: 28px;
    background-color: {rail_bg};
    border-right: 1px solid #4A4A4A;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding-top: 0px;
    gap: 0px;
    overflow: hidden;
  }}
  .rail-collapse {{
    position: relative;
    width: 27px;
    height: 22px;
    flex-shrink: 0;
    background-color: transparent;
    border-top: 1px solid #555555;
    border-left: 1px solid #333333;
    border-right: 1px solid #333333;
    border-bottom: 1px solid #555555;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-top: 4px;
    margin-bottom: 0px;
  }}
  .rail-collapse svg {{
    pointer-events: none;
  }}
  .rail-icon {{
    position: relative;
    width: 27px;
    height: 28px;
    flex-shrink: 0;
    background-color: {rail_tab_bg};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid #333333;
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 2px;
  }}
  .rail-icon svg {{
    pointer-events: none;
  }}
  .rail-icon-off {{
    opacity: 0.65;
  }}

  /* Interlocking Angled Folder Tabs with 90-degree Rotated Sideways Text */
  .exp-tab {{
    position: relative;
    width: 27px;
    height: 90px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: {ink};
    margin-bottom: -13px;
    padding-bottom: 2px;
  }}
  .exp-tab-last {{
    margin-bottom: 0px;
  }}
  .exp-tab-active {{
    color: #207194;
  }}
  .exp-tab-bg {{
    position: absolute;
    top: 0;
    left: 0;
    width: 27px;
    height: 90px;
    pointer-events: none;
  }}
  .exp-tab-label {{
    position: relative;
    width: 82px;
    height: 16px;
    font-size: 11px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    white-space: nowrap;
    transform: rotate(90deg);
    pointer-events: none;
  }}
  .rail-globe {{
    position: relative;
    flex: 1;
    width: 27px;
    pointer-events: none;
  }}
  .rail-globe-bordered {{
    border-left: 1px solid #333333;
  }}

  /* ---- Explorer Bar Collapsible Drawer (286px wide, left of the vertical tab rail) ---- */
  .exp-drawer {{
    position: absolute;
    left: 0;
    bottom: 0;
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

  /* ---- Page Holder Links View Rows (PDF Page 2 Right) ---- */
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

  /* ---- Internet Scrapbook Rows (PDF Page 1 Figure 6) ---- */
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
  .sb-row svg {{
    flex-shrink: 0;
    pointer-events: none;
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

  /* ---- Right 15px Vertical Scrollbar Track + Bottom 15px Horizontal Scrollbar Track + 15x15 Resize Corner ---- */
  .aqua-vscroll {{
    position: absolute;
    right: 0;
    bottom: 15px;
    width: 15px;
    overflow: hidden;
    pointer-events: none;
  }}
  .aqua-bottom-wrap {{
    position: absolute;
    right: 0;
    bottom: 0;
    height: 15px;
    display: flex;
    flex-direction: row;
    pointer-events: none;
  }}
  .aqua-hscroll {{
    position: relative;
    flex: 1;
    height: 15px;
    overflow: hidden;
  }}
  .aqua-resize-corner {{
    position: relative;
    width: 15px;
    height: 15px;
    flex-shrink: 0;
    overflow: hidden;
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
    top: 126px;
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
    right: 0;
    bottom: 34px;
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
    right: 0;
    bottom: 34px;
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

  /* ---- Passkey interstitial ----
     A page asked for a credential. The modal covers the chrome until a human
     allows or denies it. There is deliberately no click-outside and no
     Escape: a stray gesture must never approve a signing ceremony. */
  .pk-scrim {{
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(0, 0, 0, 0.28);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    z-index: 90;
  }}
  .pk-modal {{
    width: 340px;
    padding: 20px 22px 18px 22px;
    background-color: #F6F6F6;
    border: 1px solid #9A9A9A;
    border-radius: 12px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.32);
    display: flex;
    flex-direction: column;
    align-items: center;
    pointer-events: auto;
  }}
  .pk-icon {{
    margin-bottom: 8px;
  }}
  .pk-key-svg {{
    pointer-events: none;
  }}
  .pk-title {{
    font-size: 15px;
    font-weight: 700;
    color: #1C1C1E;
    margin-bottom: 6px;
  }}
  .pk-site {{
    font-size: 12px;
    color: #4A4A4E;
    margin-bottom: 4px;
    word-break: break-all;
    text-align: center;
  }}
  .pk-account {{
    font-size: 12px;
    color: #4A4A4E;
    margin-bottom: 4px;
    text-align: center;
  }}
  .pk-detail {{
    font-size: 11px;
    color: #6E7176;
    margin-top: 6px;
    margin-bottom: 14px;
    text-align: center;
  }}
  .pk-actions {{
    display: flex;
    flex-direction: row;
    gap: 10px;
    width: 100%;
  }}
  .pk-btn {{
    flex: 1;
    height: 26px;
    line-height: 26px;
    text-align: center;
    font-size: 12px;
    font-weight: 600;
    border-radius: 6px;
    border: 1px solid #8E8E93;
    background-color: #FFFFFF;
    color: #1C1C1E;
    cursor: pointer;
  }}
  .pk-allow {{
    background-color: #1B8A38;
    border-color: #146B2C;
    color: #FFFFFF;
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
  {passkey_prompt}
</body>
</html>"#,
        surface_height = state.surface_height,
        passkey_prompt = passkey_prompt,
        nav = p.nav,
        hairline = p.hairline,
        bevel_light = p.bevel_light,
        bevel_dark = p.bevel_dark,
        pill = p.pill,
        pill_focus = p.pill_focus,
        ink = p.ink,
        muted = p.muted,
        menu_bg = p.menu_bg,
        separator = p.separator,
        accent = p.accent,
        aqua_badge = p.aqua_badge,
        rail_bg = p.rail_bg,
        rail_tab_bg = p.rail_tab_bg,
        drawer_bg = p.drawer_bg,
        drawer_header_bg = p.drawer_header_bg,
        brand_teal = p.brand_teal,
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

fn button_class(enabled: bool) -> &'static str {
    if enabled {
        "btn"
    } else {
        "btn btn-off"
    }
}

fn exp_tab_class(active: bool) -> &'static str {
    if active {
        "exp-tab exp-tab-active"
    } else {
        "exp-tab"
    }
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
