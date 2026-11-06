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
//!      divider, `AutoFill`, `Print`, `Sherlock`) and the 3D Aqua `e` emblem on
//!      the far right.
//!   3. Full-height 3D cylindrical Aqua Blue `Address :` bar (`30px`) with `@`
//!      swirl badge, sunken white URL input (`#address`), and `▸ go` button
//!      (`#btn-go`) on the right.
//!   4. Fluted/ribbed horizontal pinstripe Favorites bar (`24px`: `@ Web Gallery`,
//!      `@ Office for Macintosh`, `@ Products for Mac`, `@ Microsoft Internet`,
//!      plus Aqua `»` chevron on the right).
//! - Left vertical Explorer Bar rail (`EXPLORER_RAIL_WIDTH = 36.0`) with
//!   `#btn-collapse-toolbar` (3D Aqua Blue `◀|||` / `|||▶`) and five interlocking
//!   angled folder tabs (`Favorites`, `History`, `Search`, `Scrapbook`,
//!   `Page Holder`) with horizontal fluted pinstripes and 90°-rotated sideways
//!   labels. When an Explorer Bar drawer (`EXPLORER_DRAWER_WIDTH = 280.0`) is open,
//!   the drawer sits on the left (`x = 0..280`) and the vertical tab rail attaches
//!   to the right edge of the drawer (`x = 280..316`), with the active tab
//!   merging seamlessly into the drawer and highlighted in bold Aqua Blue.

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
    favicon_bg: &'static str,
    favicon_ink: &'static str,
    accent: &'static str,
    aqua_badge: &'static str,
    rail_bg: &'static str,
    rail_tab_bg: &'static str,
    drawer_bg: &'static str,
    drawer_header_bg: &'static str,
    brand_teal: &'static str,
    link_blue: &'static str,
    rib_a: &'static str,
    rib_b: &'static str,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            strip: "#D4D4D4",
            tab_active: "#F2F2F2",
            nav: "#E8E8E8",
            hairline: "#707070",
            bevel_light: "#FFFFFF",
            bevel_dark: "#6A6A6A",
            pill: "#FFFFFF",
            pill_focus: "#FFFFFF",
            ink: "#111111",
            muted: "#7D8B96",
            menu_bg: "#E6E6E6",
            separator: "#B0B0B0",
            favicon_bg: "#2678B2",
            favicon_ink: "#FFFFFF",
            accent: "#16689E",
            aqua_badge: "#2476AE",
            rail_bg: "#D8D8D8",
            rail_tab_bg: "#E6E6E6",
            drawer_bg: "#EAEAEA",
            drawer_header_bg: "#DEDEDE",
            brand_teal: "#16689A",
            link_blue: "#0000EE",
            rib_a: "#F3F3F3",
            rib_b: "#DDDDDD",
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
            favicon_bg: "#2B6CB0",
            favicon_ink: "#FFFFFF",
            accent: "#38A3F8",
            aqua_badge: "#1E5B88",
            rail_bg: "#222225",
            rail_tab_bg: "#2C2C30",
            drawer_bg: "#1C1C1E",
            drawer_header_bg: "#28282B",
            brand_teal: "#58B4F8",
            link_blue: "#5AC8FA",
            rib_a: "#2A2A2E",
            rib_b: "#212124",
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

/// Total height of the IE5 Mac top chrome scaled for modern desktop windows:
/// Title/Tab Strip (26) + Ribbed Aqua Button Bar (48) + Cylindrical Aqua Address Bar (30) + Ribbed Favorites Strip (24) = 128.0.
pub const CHROME_HEIGHT: f64 = 128.0;
/// Width of the vertical IE5 Mac Explorer Bar folder-tab rail.
pub const EXPLORER_RAIL_WIDTH: f64 = 36.0;
/// Width of the collapsible Explorer Bar drawer panel when open.
pub const EXPLORER_DRAWER_WIDTH: f64 = 280.0;
/// Height of the Page Holder drawer's top control header bar.
pub const PH_HEADER_HEIGHT: f64 = 52.0;

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

    // Classic grey 3D `@` swirl icon used in Title bar, Address bar, and Favorites bar.
    let at_swirl_svg = r##"<svg class="at-svg" width="15" height="15" viewBox="0 0 16 16"><defs><linearGradient id="atg" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#FFFFFF"/><stop offset="100%" stop-color="#A8B0B8"/></linearGradient></defs><circle cx="8" cy="8" r="6.8" fill="url(#atg)" stroke="#555E68" stroke-width="1.2"/><text x="4.1" y="11.3" font-family="Geneva,Helvetica,Arial,sans-serif" font-size="9.5" font-weight="bold" fill="#222B34">@</text></svg>"##;

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
            r#"<div class="fav-pill" id="fav-bar-{i}" data-url="{url}">{at_swirl_svg}<span class="fav-pill-text">{title}</span></div>"#,
            i = i,
            url = escape_html(fav.url),
            at_swirl_svg = at_swirl_svg,
            title = escape_html(fav.title),
        ));
    }

    let secure_attr = if state.secure { "https" } else { "http" };

    // Fluted / ribbed horizontal pinstripe SVG background for the 48px Aqua button bar
    let nav_rib_bg = format!(
        r##"<svg class="rib-bg" width="2400" height="48" viewBox="0 0 2400 48"><defs><pattern id="navrib" width="16" height="4" patternUnits="userSpaceOnUse"><rect width="16" height="2" fill="{rib_a}"/><rect y="2" width="16" height="2" fill="{rib_b}"/></pattern></defs><rect width="2400" height="48" fill="url(#navrib)"/></svg>"##,
        rib_a = p.rib_a,
        rib_b = p.rib_b,
    );

    // Fluted / ribbed horizontal pinstripe SVG background for the 24px Favorites bar
    let fav_rib_bg = format!(
        r##"<svg class="rib-bg" width="2400" height="24" viewBox="0 0 2400 24"><defs><pattern id="favrib" width="16" height="4" patternUnits="userSpaceOnUse"><rect width="16" height="2" fill="{rib_a}"/><rect y="2" width="16" height="2" fill="{rib_b}"/></pattern></defs><rect width="2400" height="24" fill="url(#favrib)"/></svg>"##,
        rib_a = p.rib_a,
        rib_b = p.rib_b,
    );

    // 3D Cylindrical Aqua Blue tube SVG background for the 30px Address bar row
    let addr_aqua_tube_bg = r##"<svg class="rib-bg" width="2400" height="30" viewBox="0 0 2400 30"><defs><linearGradient id="aquatube" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#0E3958"/><stop offset="18%" stop-color="#2D80B5"/><stop offset="42%" stop-color="#76CBF7"/><stop offset="56%" stop-color="#48A2DA"/><stop offset="84%" stop-color="#175885"/><stop offset="100%" stop-color="#09263B"/></linearGradient></defs><rect width="2400" height="30" fill="url(#aquatube)"/><line x1="0" y1="0.5" x2="2400" y2="0.5" stroke="#051826" stroke-width="1"/><line x1="0" y1="29.5" x2="2400" y2="29.5" stroke="#051826" stroke-width="1"/></svg>"##;

    // 3D Glossy Aqua Blue SVG Icons for the top toolbar (28x24px)
    let back_icon_svg = if state.can_back {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="abk" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#7EE0FF"/><stop offset="45%" stop-color="#1A80C2"/><stop offset="100%" stop-color="#0A436E"/></linearGradient></defs><path d="M19,3 C20.5,2 22,3 22,5 L22,19 C22,21 20.5,22 19,21 L6,13.5 C4.5,12.5 4.5,11.5 6,10.5 Z" fill="url(#abk)" stroke="#08385C" stroke-width="1.3"/><path d="M19.5,4.5 L7.5,11.5 L19.5,9.5 Z" fill="#C4EEFF" fill-opacity="0.7"/></svg>"##
    } else {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="abkd" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#D2DEE6"/><stop offset="55%" stop-color="#A5B8C6"/><stop offset="100%" stop-color="#879AA8"/></linearGradient></defs><path d="M19,3 C20.5,2 22,3 22,5 L22,19 C22,21 20.5,22 19,21 L6,13.5 C4.5,12.5 4.5,11.5 6,10.5 Z" fill="url(#abkd)" stroke="#788A98" stroke-width="1.2"/><path d="M19.5,4.5 L7.5,11.5 L19.5,9.5 Z" fill="#ECF4F8" fill-opacity="0.6"/></svg>"##
    };

    let fwd_icon_svg = if state.can_forward {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="afw" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#7EE0FF"/><stop offset="45%" stop-color="#1A80C2"/><stop offset="100%" stop-color="#0A436E"/></linearGradient></defs><path d="M9,3 C7.5,2 6,3 6,5 L6,19 C6,21 7.5,22 9,21 L22,13.5 C23.5,12.5 23.5,11.5 22,10.5 Z" fill="url(#afw)" stroke="#08385C" stroke-width="1.3"/><path d="M8.5,4.5 L20.5,11.5 L8.5,9.5 Z" fill="#C4EEFF" fill-opacity="0.7"/></svg>"##
    } else {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="afwd" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#D2DEE6"/><stop offset="55%" stop-color="#A5B8C6"/><stop offset="100%" stop-color="#879AA8"/></linearGradient></defs><path d="M9,3 C7.5,2 6,3 6,5 L6,19 C6,21 7.5,22 9,21 L22,13.5 C23.5,12.5 23.5,11.5 22,10.5 Z" fill="url(#afwd)" stroke="#788A98" stroke-width="1.2"/><path d="M8.5,4.5 L20.5,11.5 L8.5,9.5 Z" fill="#ECF4F8" fill-opacity="0.6"/></svg>"##
    };

    let stop_icon_svg = if state.loading {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="astp" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#FF9A8B"/><stop offset="50%" stop-color="#D63626"/><stop offset="100%" stop-color="#7D190F"/></linearGradient></defs><path d="M7,4 L11,4 L14,8.5 L17,4 L21,4 L16.5,11.5 L21.5,20 L17.5,20 L14,14.5 L10.5,20 L6.5,20 L11.5,11.5 Z" fill="url(#astp)" stroke="#5E1109" stroke-width="1.1"/></svg>"##
    } else {
        r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="astpd" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#D2DEE6"/><stop offset="55%" stop-color="#A5B8C6"/><stop offset="100%" stop-color="#879AA8"/></linearGradient></defs><path d="M7,4 L11,4 L14,8.5 L17,4 L21,4 L16.5,11.5 L21.5,20 L17.5,20 L14,14.5 L10.5,20 L6.5,20 L11.5,11.5 Z" fill="url(#astpd)" stroke="#788A98" stroke-width="1.1"/></svg>"##
    };

    let refresh_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="arf" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#7EE0FF"/><stop offset="45%" stop-color="#1982C4"/><stop offset="100%" stop-color="#0A4570"/></linearGradient></defs><path d="M7,12 C7,7.2 11,4 15.5,4 C17.8,4 19.8,4.8 21.2,6.2 L23.5,4 L23.5,10.5 L17,10.5 L19.2,8.3 C18.2,7.3 16.9,6.8 15.5,6.8 C12.4,6.8 10,9.1 10,12 Z" fill="url(#arf)" stroke="#08385C" stroke-width="1"/><path d="M21,12 C21,16.8 17,20 12.5,20 C10.2,20 8.2,19.2 6.8,17.8 L4.5,20 L4.5,13.5 L11,13.5 L8.8,15.7 C9.8,16.7 11.1,17.2 12.5,17.2 C15.6,17.2 18,14.9 18,12 Z" fill="url(#arf)" stroke="#08385C" stroke-width="1"/></svg>"##;

    let home_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="ahm" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#84D8FF"/><stop offset="50%" stop-color="#1B80C0"/><stop offset="100%" stop-color="#0A4068"/></linearGradient></defs><polygon points="14,3 4,12 7.5,12 7.5,21 20.5,21 20.5,12 24,12" fill="url(#ahm)" stroke="#083659" stroke-width="1.2"/><rect x="12" y="14" width="4" height="7" fill="#072C47"/></svg>"##;

    let autofill_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="aaf" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#84D8FF"/><stop offset="45%" stop-color="#1B82C4"/><stop offset="100%" stop-color="#093D63"/></linearGradient></defs><rect x="5" y="14" width="16" height="7" rx="1.5" fill="#F8FAFC" stroke="#4A657A" stroke-width="1.1"/><line x1="8" y1="17.5" x2="17" y2="17.5" stroke="#286090" stroke-width="1.2"/><path d="M15,2 C18,2 21,5 20,9 L14,17 L8,14 L12,5 C13,3 14,2 15,2 Z" fill="url(#aaf)" stroke="#083659" stroke-width="1.2"/><ellipse cx="15.5" cy="6.5" rx="2.5" ry="1.6" fill="#C4EEFF" fill-opacity="0.75"/></svg>"##;

    let print_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="apr" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#82D6FF"/><stop offset="48%" stop-color="#1B80C0"/><stop offset="100%" stop-color="#0A3F66"/></linearGradient></defs><rect x="8" y="2" width="12" height="7" fill="#FFFFFF" stroke="#4A6478" stroke-width="1.1"/><rect x="4" y="8" width="20" height="10" rx="2.5" fill="url(#apr)" stroke="#083659" stroke-width="1.2"/><rect x="6.5" y="9.5" width="15" height="2.2" rx="1" fill="#C8EEFF" fill-opacity="0.75"/><rect x="7" y="15" width="14" height="6.5" fill="#FFFFFF" stroke="#4A6478" stroke-width="1.1"/><line x1="9.5" y1="18" x2="18.5" y2="18" stroke="#246A9E" stroke-width="1.2"/></svg>"##;

    let sherlock_icon_svg = r##"<svg width="28" height="24" viewBox="0 0 28 24"><defs><linearGradient id="ash" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#9BE2FF"/><stop offset="50%" stop-color="#1D84C6"/><stop offset="100%" stop-color="#0A3F66"/></linearGradient></defs><line x1="14" y1="13" x2="6" y2="21" stroke="#1E293B" stroke-width="3.4" stroke-linecap="round"/><circle cx="16.5" cy="9.5" r="6.5" fill="url(#ash)" stroke="#08385C" stroke-width="1.4"/><ellipse cx="14.8" cy="7.2" rx="3.2" ry="2" fill="#DDF6FF" fill-opacity="0.8"/></svg>"##;

    // 3D Glossy Aqua Blue `e` emblem on the far right of the toolbar (`media_1794004131120_b4ecd3e6.jpg`)
    let ie_aqua_emblem_svg = r##"<svg width="34" height="34" viewBox="0 0 34 34"><defs><radialGradient id="ieorb" cx="35%" cy="30%" r="70%"><stop offset="0%" stop-color="#B8ECFF"/><stop offset="45%" stop-color="#2894D8"/><stop offset="100%" stop-color="#0B4268"/></radialGradient></defs><ellipse cx="17" cy="17" rx="15" ry="9" transform="rotate(-24 17 17)" fill="none" stroke="#58B8F0" stroke-width="2.2"/><circle cx="17" cy="17" r="12" fill="url(#ieorb)" stroke="#083659" stroke-width="1.2"/><text x="10.5" y="23.5" font-family="Georgia,serif" font-size="21" font-weight="bold" fill="#FFFFFF">e</text></svg>"##;

    let beaded_sep_svg = r##"<svg class="nav-sep-svg" width="6" height="38" viewBox="0 0 6 38"><circle cx="3" cy="3" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="7" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="11" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="15" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="19" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="23" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="27" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="31" r="1.1" fill="#6E6E6E"/><circle cx="3" cy="35" r="1.1" fill="#6E6E6E"/></svg>"##;

    let grip_ridges_left_svg = r##"<svg class="grip-svg" width="110" height="16" viewBox="0 0 110 16"><line x1="2" y1="3" x2="108" y2="3" stroke="#FFFFFF" stroke-width="1"/><line x1="2" y1="4" x2="108" y2="4" stroke="#8C8C8C" stroke-width="1"/><line x1="2" y1="6" x2="108" y2="6" stroke="#FFFFFF" stroke-width="1"/><line x1="2" y1="7" x2="108" y2="7" stroke="#8C8C8C" stroke-width="1"/><line x1="2" y1="9" x2="108" y2="9" stroke="#FFFFFF" stroke-width="1"/><line x1="2" y1="10" x2="108" y2="10" stroke="#8C8C8C" stroke-width="1"/><line x1="2" y1="12" x2="108" y2="12" stroke="#FFFFFF" stroke-width="1"/><line x1="2" y1="13" x2="108" y2="13" stroke="#8C8C8C" stroke-width="1"/></svg>"##;
    let grip_ridges_right_svg = r##"<svg class="grip-svg-right" width="1600" height="16" viewBox="0 0 1600 16"><line x1="4" y1="3" x2="1596" y2="3" stroke="#FFFFFF" stroke-width="1"/><line x1="4" y1="4" x2="1596" y2="4" stroke="#8C8C8C" stroke-width="1"/><line x1="4" y1="6" x2="1596" y2="6" stroke="#FFFFFF" stroke-width="1"/><line x1="4" y1="7" x2="1596" y2="7" stroke="#8C8C8C" stroke-width="1"/><line x1="4" y1="9" x2="1596" y2="9" stroke="#FFFFFF" stroke-width="1"/><line x1="4" y1="10" x2="1596" y2="10" stroke="#8C8C8C" stroke-width="1"/><line x1="4" y1="12" x2="1596" y2="12" stroke="#FFFFFF" stroke-width="1"/><line x1="4" y1="13" x2="1596" y2="13" stroke="#8C8C8C" stroke-width="1"/></svg>"##;

    let top_bars = if state.toolbar_collapsed {
        String::new()
    } else {
        format!(
            r##"<div class="tabstrip">
                 <div class="win-box"></div>
                 <div class="grip-left">{grip_ridges_left_svg}</div>
                 {tabs}
                 <div class="newtab" id="btn-newtab">+</div>
                 <div class="grip-right">{grip_ridges_right_svg}</div>
                 <div class="win-box"></div>
                 <div class="win-box"></div>
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
                 <div class="btn" id="btn-menu">
                   {sherlock_icon_svg}
                   <div class="btn-lbl">Sherlock</div>
                 </div>
                 <div class="ie-emblem">{ie_aqua_emblem_svg}</div>
               </div>
               <div class="address-row">
                 {addr_aqua_tube_bg}
                 <div class="address-badge">Address :</div>
                 <div class="{bar_class}" id="address" data-scheme="{secure_attr}">
                   {at_swirl_svg}
                   <div class="url">{url}</div>
                   {caret}
                 </div>
                 <div class="go-btn" id="btn-go">
                   <span class="go-tri">&#9656;</span>
                   <span class="go-lbl">go</span>
                 </div>
               </div>
               <div class="favbar">
                 {fav_rib_bg}
                 {fav_strip_items}
                 <div class="fav-chevron">&#187;</div>
               </div>"##,
            grip_ridges_left_svg = grip_ridges_left_svg,
            grip_ridges_right_svg = grip_ridges_right_svg,
            tabs = tabs,
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
            sherlock_icon_svg = sherlock_icon_svg,
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

    // 3D Glossy Aqua Blue `◀|||` / `|||▶` collapse toggle icon at the top of the Explorer Bar rail
    let collapse_svg = if state.toolbar_collapsed {
        r##"<svg width="26" height="18" viewBox="0 0 26 18"><defs><linearGradient id="aclp" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#7EE0FF"/><stop offset="50%" stop-color="#1B80C0"/><stop offset="100%" stop-color="#093B61"/></linearGradient></defs><rect x="2" y="3" width="2.5" height="12" rx="1" fill="url(#aclp)" stroke="#083659" stroke-width="0.7"/><rect x="6" y="3" width="2.5" height="12" rx="1" fill="url(#aclp)" stroke="#083659" stroke-width="0.7"/><rect x="10" y="3" width="2.5" height="12" rx="1" fill="url(#aclp)" stroke="#083659" stroke-width="0.7"/><polygon points="15,2.5 24,9 15,15.5" fill="url(#aclp)" stroke="#083659" stroke-width="0.9"/></svg>"##
    } else {
        r##"<svg width="26" height="18" viewBox="0 0 26 18"><defs><linearGradient id="aclp" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#7EE0FF"/><stop offset="50%" stop-color="#1B80C0"/><stop offset="100%" stop-color="#093B61"/></linearGradient></defs><polygon points="11,2.5 2,9 11,15.5" fill="url(#aclp)" stroke="#083659" stroke-width="0.9"/><rect x="13.5" y="3" width="2.5" height="12" rx="1" fill="url(#aclp)" stroke="#083659" stroke-width="0.7"/><rect x="17.5" y="3" width="2.5" height="12" rx="1" fill="url(#aclp)" stroke="#083659" stroke-width="0.7"/><rect x="21.5" y="3" width="2.5" height="12" rx="1" fill="url(#aclp)" stroke="#083659" stroke-width="0.7"/></svg>"##
    };

    let fav_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Favorites));
    let hist_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::History));
    let srch_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Search));
    let scrp_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Scrapbook));
    let ph_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::PageHolder));

    // Interlocking vertical folder tabs with horizontal fluted/ribbed pinstripes and `/` diagonal joints
    let tab_svg_inactive = format!(
        r##"<svg class="exp-tab-bg" width="34" height="90" viewBox="0 0 34 90"><defs><pattern id="tabrib" width="12" height="4" patternUnits="userSpaceOnUse"><rect width="12" height="2" fill="{rib_a}"/><rect y="2" width="12" height="2" fill="{rib_b}"/></pattern></defs><polygon points="0,0 33,0 33,76 0,90" fill="url(#tabrib)"/><line x1="0.5" y1="0" x2="0.5" y2="90" stroke="#555555" stroke-width="1"/><line x1="1.5" y1="1" x2="1.5" y2="89" stroke="#FFFFFF" stroke-width="1"/><line x1="33" y1="0" x2="33" y2="76" stroke="#4A4A4A" stroke-width="1.2"/><line x1="0" y1="90" x2="33" y2="76" stroke="#4A4A4A" stroke-width="1.4"/><line x1="0" y1="88.5" x2="32.5" y2="74.5" stroke="#FFFFFF" stroke-width="1"/></svg>"##,
        rib_a = p.rib_a,
        rib_b = p.rib_b,
    );
    // Active tab merges seamlessly with the open drawer on its left (no left border line, solid drawer background fill, bold Aqua Blue text)
    let tab_svg_active = format!(
        r##"<svg class="exp-tab-bg" width="34" height="90" viewBox="0 0 34 90"><polygon points="0,0 33,0 33,76 0,90" fill="{drawer_bg}"/><line x1="0" y1="1" x2="33" y2="1" stroke="#FFFFFF" stroke-width="1.4"/><line x1="33" y1="0" x2="33" y2="76" stroke="#4A4A4A" stroke-width="1.3"/><line x1="0" y1="90" x2="33" y2="76" stroke="#4A4A4A" stroke-width="1.5"/></svg>"##,
        drawer_bg = p.drawer_bg,
    );

    let fav_bg = if state.explorer_tab == Some(ExplorerTab::Favorites) { &tab_svg_active } else { &tab_svg_inactive };
    let hist_bg = if state.explorer_tab == Some(ExplorerTab::History) { &tab_svg_active } else { &tab_svg_inactive };
    let srch_bg = if state.explorer_tab == Some(ExplorerTab::Search) { &tab_svg_active } else { &tab_svg_inactive };
    let scrp_bg = if state.explorer_tab == Some(ExplorerTab::Scrapbook) { &tab_svg_active } else { &tab_svg_inactive };
    let ph_bg = if state.explorer_tab == Some(ExplorerTab::PageHolder) { &tab_svg_active } else { &tab_svg_inactive };

    let rail_rib_bg = format!(
        r##"<svg class="rib-bg" width="36" height="1600" viewBox="0 0 36 1600"><defs><pattern id="railrib" width="12" height="4" patternUnits="userSpaceOnUse"><rect width="12" height="2" fill="{rib_a}"/><rect y="2" width="12" height="2" fill="{rib_b}"/></pattern></defs><rect width="36" height="1600" fill="url(#railrib)"/></svg>"##,
        rib_a = p.rib_a,
        rib_b = p.rib_b,
    );

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
             <div class="rail-globe">
               <svg width="18" height="18" viewBox="0 0 18 18"><defs><radialGradient id="glbg" cx="35%" cy="30%" r="65%"><stop offset="0%" stop-color="#A6E8FF"/><stop offset="50%" stop-color="#1E82C4"/><stop offset="100%" stop-color="#093658"/></radialGradient></defs><circle cx="9" cy="9" r="7" fill="url(#glbg)" stroke="#072842" stroke-width="1.2"/><ellipse cx="9" cy="9" rx="3.5" ry="7" fill="none" stroke="#B5E6FF" stroke-width="0.9"/><line x1="2" y1="9" x2="16" y2="9" stroke="#B5E6FF" stroke-width="0.9"/></svg>
             </div>
           </div>"##,
    );

    // Camera-and-page icon used for Scrapbook entries (PDF Page 1 Figure 6).
    let sb_doc_icon = r##"<svg width="18" height="15" viewBox="0 0 18 15"><rect x="1" y="2.5" width="11" height="10" rx="1.8" fill="#7A8B99" stroke="#2F3E4C" stroke-width="1.1"/><circle cx="6.5" cy="7.5" r="2.6" fill="#D9E6F2" stroke="#2F3E4C" stroke-width="1"/><rect x="10" y="1" width="7" height="12.5" fill="#FFFFFF" stroke="#4A5568" stroke-width="1"/><line x1="11.5" y1="4.5" x2="15" y2="4.5" stroke="#286090" stroke-width="1"/><line x1="11.5" y1="7.5" x2="15" y2="7.5" stroke="#718096" stroke-width="1"/></svg>"##;

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

    // Classic IE5 Mac Bottom Aqua Horizontal Scrollbar + "Internet zone" Status Bar (`media_1794004131111_bc216b24.jpg`)
    let bottom_aqua_bar = format!(
        r##"<div class="aqua-bottom-wrap" style="left:{content_x:.1}px;">
             <div class="aqua-hscroll">
               <svg class="rib-bg" width="2400" height="15" viewBox="0 0 2400 15"><defs><linearGradient id="aqscr" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#14649C"/><stop offset="30%" stop-color="#42B0ED"/><stop offset="55%" stop-color="#7AD4FF"/><stop offset="85%" stop-color="#1D84C6"/><stop offset="100%" stop-color="#0D4B78"/></linearGradient></defs><rect width="2400" height="15" fill="url(#aqscr)"/><line x1="0" y1="0.5" x2="2400" y2="0.5" stroke="#083252" stroke-width="1"/><line x1="0" y1="14.5" x2="2400" y2="14.5" stroke="#083252" stroke-width="1"/></svg>
             </div>
             <div class="aqua-statusbar">
               <svg class="rib-bg" width="2400" height="19" viewBox="0 0 2400 19"><defs><pattern id="strib" width="16" height="4" patternUnits="userSpaceOnUse"><rect width="16" height="2" fill="{rib_a}"/><rect y="2" width="16" height="2" fill="{rib_b}"/></pattern></defs><rect width="2400" height="19" fill="url(#strib)"/></svg>
               <div class="status-zone">
                 <svg width="13" height="13" viewBox="0 0 18 18"><defs><radialGradient id="glbs" cx="35%" cy="30%" r="65%"><stop offset="0%" stop-color="#A6E8FF"/><stop offset="50%" stop-color="#1E82C4"/><stop offset="100%" stop-color="#093658"/></radialGradient></defs><circle cx="9" cy="9" r="7" fill="url(#glbs)" stroke="#072842" stroke-width="1.2"/></svg>
                 <span>Internet zone</span>
               </div>
             </div>
           </div>"##,
        content_x = content_x,
        rib_a = p.rib_a,
        rib_b = p.rib_b,
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
  .rib-bg {{
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
  }}
  .at-svg {{
    flex-shrink: 0;
    pointer-events: none;
  }}

  /* ---- Row 1: IE5 Mac Platinum/Aqua Window Title & Tab Strip (26px) ---- */
  .tabstrip {{
    height: 26px;
    background-color: {strip};
    border-top: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_dark};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 8px;
    gap: 6px;
    overflow: hidden;
  }}
  .win-box {{
    width: 13px;
    height: 13px;
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid #4A4A4A;
    border-bottom: 1px solid #4A4A4A;
    flex-shrink: 0;
    pointer-events: none;
  }}
  .grip-left {{
    width: 110px;
    height: 16px;
    flex-shrink: 0;
    overflow: hidden;
    pointer-events: none;
  }}
  .grip-right {{
    flex: 1;
    height: 16px;
    overflow: hidden;
    pointer-events: none;
  }}
  .grip-svg, .grip-svg-right {{
    pointer-events: none;
  }}
  .tab {{
    height: 21px;
    max-width: 240px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    padding: 0 9px;
    background-color: {strip};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
    color: {ink};
    font-size: 12px;
    font-weight: 700;
    flex-shrink: 0;
  }}
  .tab-active {{
    background-color: {tab_active};
    border-bottom: 1px solid {tab_active};
  }}
  .favicon {{
    width: 14px;
    height: 14px;
    border-radius: 7px;
    background-color: {favicon_bg};
    color: {favicon_ink};
    font-size: 9px;
    font-weight: 700;
    text-align: center;
    line-height: 14px;
    flex-shrink: 0;
    pointer-events: none;
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
    color: {ink};
  }}
  .newtab {{
    width: 22px;
    height: 20px;
    font-size: 14px;
    font-weight: 700;
    line-height: 18px;
    text-align: center;
    color: {ink};
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
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
    padding: 0 16px;
    gap: 10px;
    overflow: hidden;
  }}
  .btn {{
    position: relative;
    min-width: 68px;
    height: 42px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 2px 8px;
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
    padding: 2px 6px 2px 0;
    overflow: hidden;
  }}
  .address-badge {{
    position: relative;
    width: 82px;
    height: 26px;
    color: #FFFFFF;
    font-size: 12.5px;
    font-weight: 700;
    line-height: 26px;
    padding-left: 12px;
    letter-spacing: 0.02em;
    pointer-events: none;
  }}
  .bar {{
    position: relative;
    flex: 1;
    height: 24px;
    background-color: {pill};
    border-top: 1.5px solid #072235;
    border-left: 1.5px solid #072235;
    border-right: 1px solid #8CCDF5;
    border-bottom: 1px solid #8CCDF5;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 8px;
    gap: 7px;
  }}
  .bar-focused {{
    background-color: {pill_focus};
    border: 1.5px solid {accent};
  }}
  .url {{
    color: {ink};
    font-size: 13px;
    line-height: 22px;
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
    gap: 4px;
    margin-left: 4px;
    color: #FFFFFF;
    font-size: 12.5px;
    font-weight: 700;
  }}
  .go-tri, .go-lbl {{
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
    gap: 20px;
    overflow: hidden;
  }}
  .fav-pill {{
    position: relative;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    height: 20px;
    padding: 0 4px;
    font-size: 11.5px;
    font-weight: 600;
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
    color: {brand_teal};
    font-size: 15px;
    font-weight: 800;
    line-height: 20px;
    pointer-events: none;
  }}

  /* ---- Left Vertical Explorer Bar Rail (36px wide, Sideways Folder Tabs) ---- */
  .exp-rail {{
    position: absolute;
    bottom: 0;
    width: 36px;
    background-color: {rail_bg};
    border-right: 1px solid #4A4A4A;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding-top: 2px;
    gap: 0px;
    overflow: hidden;
  }}
  .rail-collapse {{
    position: relative;
    width: 33px;
    height: 26px;
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
  .rail-collapse svg {{
    pointer-events: none;
  }}
  .rail-icon {{
    position: relative;
    width: 33px;
    height: 28px;
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
  .rail-icon svg {{
    pointer-events: none;
  }}
  .rail-icon-off {{
    opacity: 0.65;
  }}

  /* Interlocking Angled Folder Tabs with 90-degree Rotated Sideways Text */
  .exp-tab {{
    position: relative;
    width: 34px;
    height: 90px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: {ink};
    margin-bottom: -12px;
  }}
  .exp-tab-active {{
    color: {brand_teal};
  }}
  .exp-tab-bg {{
    position: absolute;
    top: 0;
    left: 0;
    width: 34px;
    height: 90px;
    pointer-events: none;
  }}
  .exp-tab-label {{
    position: relative;
    width: 82px;
    height: 18px;
    font-size: 11.5px;
    font-weight: 700;
    line-height: 18px;
    text-align: center;
    white-space: nowrap;
    transform: rotate(90deg);
    pointer-events: none;
  }}
  .rail-globe {{
    position: relative;
    margin-top: auto;
    margin-bottom: 6px;
    margin-left: 5px;
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }}

  /* ---- Explorer Bar Collapsible Drawer (280px wide, left of the vertical tab rail) ---- */
  .exp-drawer {{
    position: absolute;
    left: 0;
    bottom: 0;
    width: 280px;
    background-color: {drawer_bg};
    border-left: 4px solid {rail_bg};
    border-top: 4px solid {rail_bg};
    display: flex;
    flex-direction: column;
  }}
  .exp-drawer-transparent {{
    background-color: transparent;
  }}
  .drawer-header {{
    height: 44px;
    background-color: {drawer_header_bg};
    border-top: 1px solid #333333;
    border-left: 1px solid #333333;
    border-right: 1px solid #333333;
    border-bottom: 1px solid #333333;
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 10px;
    gap: 7px;
  }}
  .drawer-brand {{
    color: {brand_teal};
    font-size: 12.5px;
    font-weight: 700;
    line-height: 13px;
    margin-right: auto;
    pointer-events: none;
  }}
  .drawer-subbar {{
    height: 4px;
    background-color: {rail_bg};
    border-bottom: 1px solid {bevel_dark};
  }}
  .mac-btn {{
    height: 24px;
    padding: 0 10px;
    background-color: {nav};
    border-top: 1px solid {bevel_light};
    border-left: 1px solid {bevel_light};
    border-right: 1.5px solid #222222;
    border-bottom: 1.5px solid #222222;
    color: {ink};
    font-size: 11.5px;
    font-weight: 600;
    line-height: 21px;
    white-space: nowrap;
  }}
  .mac-btn-active {{
    background-color: #B8D6EC;
    border-top: 1.5px solid #222222;
    border-left: 1.5px solid #222222;
    border-right: 1px solid {bevel_light};
    border-bottom: 1px solid {bevel_light};
  }}
  .mac-btn-disabled {{
    color: {muted};
    border-right: 1px solid {bevel_dark};
    border-bottom: 1px solid {bevel_dark};
  }}
  .drawer-body {{
    flex: 1;
    background-color: {drawer_bg};
    border-left: 1px solid #333333;
    border-right: 1px solid #333333;
    overflow: hidden;
    display: flex;
    flex-direction: column;
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
    padding: 3px 10px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 7px;
    background-color: #FFFFFF;
    border-bottom: 1px solid #EAEAEA;
  }}
  .ph-tri {{
    color: #16689A;
    font-size: 11px;
    line-height: 18px;
    flex-shrink: 0;
    pointer-events: none;
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

  /* ---- Bottom Aqua Blue Scrollbar + Internet zone Status Bar ---- */
  .aqua-bottom-wrap {{
    position: absolute;
    right: 0;
    bottom: 0;
    height: 34px;
    display: flex;
    flex-direction: column;
    pointer-events: none;
  }}
  .aqua-hscroll {{
    position: relative;
    height: 15px;
    overflow: hidden;
  }}
  .aqua-statusbar {{
    position: relative;
    height: 19px;
    border-top: 1px solid {bevel_light};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 8px;
    overflow: hidden;
  }}
  .status-zone {{
    position: relative;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    color: {ink};
    font-size: 11px;
    font-weight: 600;
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
    top: 76px;
    left: 520px;
    width: 170px;
    background-color: {menu_bg};
    border: 1px solid {hairline};
    padding: 4px;
  }}
  .menu-item {{
    height: 24px;
    padding: 0 10px;
    font-size: 12px;
    line-height: 24px;
    color: {ink};
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
        menu_bg = p.menu_bg,
        separator = p.separator,
        favicon_bg = p.favicon_bg,
        favicon_ink = p.favicon_ink,
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

fn first_letter(s: &str) -> String {
    s.chars()
        .find(|c| c.is_alphanumeric())
        .and_then(|c| c.to_uppercase().next())
        .unwrap_or('M')
        .to_string()
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
