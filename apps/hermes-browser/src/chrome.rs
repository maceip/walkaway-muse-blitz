//! The browser chrome, rendered by the same HTML/CSS engine as the page.
//!
//! Netscape 6 Classic-inspired slate chrome: compact functional menu, raised
//! glyph-and-label navigation, recessed Location field and spring-driven curved
//! tabs. Geometry lives in the canonical `gooey_tabs` module; no web wrapper or
//! page JavaScript participates in chrome interaction.
//!
//! Also renders the supervised Semantic X-Ray overlay layer (DOM element role
//! tags, Blind Vault shield, Engine Commit Freeze shield, and live-measured
//! Engine Telemetry bar) directly in the transparent chrome overlay document
//! so it paints at 60fps in the live WGPU window and in `--screenshot` /
//! `include_screenshot` PNGs without mutating the underlying page DOM.

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

struct Palette {
    strip: &'static str,
    highlight: &'static str,
    shadow: &'static str,
    tab_active: &'static str,
    nav: &'static str,
    hairline: &'static str,
    pill: &'static str,
    pill_focus: &'static str,
    ink: &'static str,
    muted: &'static str,
    hover: &'static str,
    menu_bg: &'static str,
    favicon_bg: &'static str,
    favicon_ink: &'static str,
    accent: &'static str,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            strip: "#8C99AD",
            highlight: "#F4F6FA",
            shadow: "#556176",
            tab_active: "#E9EDF3",
            nav: "#C4CCD8",
            hairline: "#7A879A",
            pill: "#F4F5F1",
            pill_focus: "#FFFFFF",
            ink: "#1E2C40",
            muted: "#637086",
            hover: "#DCE3ED",
            menu_bg: "#D4DBE5",
            favicon_bg: "#7387A4",
            favicon_ink: "#354B68",
            accent: "#254F82",
        },
        Theme::Dark => Palette {
            strip: "#273449",
            highlight: "#697A91",
            shadow: "#142034",
            tab_active: "#4D607A",
            nav: "#3B4B62",
            hairline: "#1C293C",
            pill: "#202D40",
            pill_focus: "#182438",
            ink: "#E8EDF5",
            muted: "#A3B2C5",
            hover: "#516780",
            menu_bg: "#3C4E67",
            favicon_bg: "#738CAD",
            favicon_ink: "#CAD8E9",
            accent: "#A6CEF8",
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
/// Every numeric value here comes from live runtime measurement (`Instant::elapsed`
/// and OS `getrusage`), never from static constants.
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
    /// Trusted, already-escaped local debugger renderer output only. Never pass
    /// page HTML here: the debugger is a separate local Hermes scratchpad.
    pub debugger_html: Option<&'a str>,
    /// None is the static/reduced-motion path. The host owns the frame clock.
    pub tab_animation: Option<&'a crate::gooey_tabs::TabAnimation>,
    /// Whole surface width in CSS pixels, used for bounded native tab layout.
    pub surface_width: f64,
    /// True when the current URL is https — drives the padlock.
    pub secure: bool,
    /// Height of the whole surface in CSS pixels. The chrome body spans it (and
    /// stays transparent below the nav row) so the menu is not clipped.
    pub surface_height: f64,
    pub theme: Theme,
    pub supervision: Option<SupervisionInfo<'a>>,
    pub telemetry: Option<EngineTelemetry>,
}

/// Menu (24) + traditional toolbar (50) + Location (32) + tab strip (38).
/// Main uses this same offset for page layout, pointer input, and screenshots.
pub const CHROME_HEIGHT: f64 = 144.0;
pub const DEBUGGER_HEIGHT: f64 = 300.0;

/// Keep debugger content below the chrome in short windows. Main uses this
/// same value to block page input under the opaque debugger pane.
pub fn debugger_height(surface_height: f64) -> f64 {
    if !surface_height.is_finite() {
        return 0.0;
    }
    (surface_height - CHROME_HEIGHT).clamp(0.0, DEBUGGER_HEIGHT)
}

pub fn render(state: &ChromeState<'_>) -> String {
    let p = palette(state.theme);
    let back_class = button_class(state.can_back);
    let forward_class = button_class(state.can_forward);
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

    let tabs = render_tabs(state, &p);
    let surface_height = if state.surface_height.is_finite() {
        state.surface_height.max(0.0)
    } else {
        CHROME_HEIGHT
    };
    let debugger = state.debugger_html.unwrap_or("");

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
            let min_y = CHROME_HEIGHT;
            let max_y = (state.surface_height - 32.0).max(min_y + 20.0);
            for b in &tel.overlays {
                let page_y = b.y + CHROME_HEIGHT;
                if page_y < min_y || page_y > max_y || b.width <= 1.0 || b.height <= 1.0 {
                    continue;
                }
                let x = b.x.max(0.0);
                let w = b.width.max(24.0);
                let h = b.height.max(18.0);
                if b.is_frozen_target {
                    let fp = b.freeze_fp_short.as_deref().unwrap_or("bound");
                    items.push_str(&format!(
                        r#"<div class="xray-box xray-freeze" id="xray-freeze-shield" style="left:{x:.1}px;top:{page_y:.1}px;width:{w:.1}px;height:{h:.1}px;">
                             <div class="xray-pill xray-pill-freeze" id="xray-tag-{id}">[{id}: ENGINE COMMIT FREEZE &#183; fp:{fp}]</div>
                           </div>"#,
                        x = x,
                        page_y = page_y,
                        w = w,
                        h = h,
                        id = escape_html(&b.id),
                        fp = escape_html(fp),
                    ));
                } else if b.is_vault_secret {
                    items.push_str(&format!(
                        r#"<div class="xray-box xray-vault" id="xray-vault-shield" style="left:{x:.1}px;top:{page_y:.1}px;width:{w:.1}px;height:{h:.1}px;">
                             <div class="xray-pill xray-pill-vault" id="xray-tag-{id}">[{id}: BLIND VAULT &#8212; Hidden from LLM/JS]</div>
                           </div>"#,
                        x = x,
                        page_y = page_y,
                        w = w,
                        h = h,
                        id = escape_html(&b.id),
                    ));
                } else {
                    items.push_str(&format!(
                        r#"<div class="xray-box" id="xray-box-{id}" style="left:{x:.1}px;top:{page_y:.1}px;width:{w:.1}px;height:{h:.1}px;">
                             <div class="xray-pill" id="xray-tag-{id}">[{id}: {role}]</div>
                           </div>"#,
                        x = x,
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
                r#"<div class="xray-hud" id="xray-telemetry-bar">
                     <div class="xray-hud-brand">STATIC HERMES + BLITZ ENGINE</div>
                     <div class="xray-hud-metric" id="xray-metric-timing">Layout+Observe: {ms:.2}ms</div>
                     <div class="xray-hud-metric" id="xray-metric-rss">RAM/RSS: {rss:.1}MB</div>
                     <div class="xray-hud-metric" id="xray-metric-sig">FormSignature: {sig}</div>
                     <div class="xray-hud-metric xray-hud-recall" id="xray-metric-recall">Recall: {recall}</div>
                   </div>"#,
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
        r#"<div class="menu" id="menu" role="menu" aria-label="Browser">
             <div class="menu-item" id="menu-home" role="menuitem">Home</div>
             <div class="menu-item" id="menu-reload" role="menuitem">Reload</div>
             <div class="menu-sep"></div>
             <div class="menu-item" id="menu-back" role="menuitem">Back</div>
             <div class="menu-item" id="menu-forward" role="menuitem">Forward</div>
             <div class="menu-sep"></div>
             <div class="menu-item" id="menu-debugger" role="menuitem">JavaScript debugger</div>
             <div class="menu-item" id="menu-close-tab" role="menuitem">Close tab</div>
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
  body {{ background-color: transparent; font-family: Tahoma, Verdana, Arial, sans-serif; position: relative; color: {ink}; }}

  /* Classic menu has actual commands, not inert File/Edit placeholders. */
  .menubar {{ height:24px; display:flex; align-items:center; gap:4px; padding:0 8px; background-color:{nav}; border-top:1px solid {highlight}; border-bottom:1px solid {shadow}; }}
  .menubtn {{ height:21px; padding:2px 9px; font-size:11px; display:flex; align-items:center; gap:7px; border:1px solid transparent; cursor:pointer; }}
  .menubtn:hover, .menubtn-open {{ background-color:{hover}; border-top-color:{highlight}; border-left-color:{highlight}; border-right-color:{shadow}; border-bottom-color:{shadow}; }}
  .menu-caption {{ font-size:10px; color:{muted}; margin-left:auto; white-space:nowrap; overflow:hidden; }}
  .down {{ width:0; height:0; border-left:3px solid transparent; border-right:3px solid transparent; border-top:4px solid {ink}; }}

  /* Traditional raised toolbar: real glyphs and labels, no capsule controls. */
  .nav {{ height:50px; background-color:{nav}; border-top:1px solid {highlight}; border-bottom:1px solid {shadow}; display:flex; align-items:center; padding:0 8px; gap:4px; }}
  .grip {{ width:4px; height:32px; flex-shrink:0; border-left:1px solid {highlight}; border-right:1px solid {shadow}; margin-right:3px; }}
  .btn {{ width:57px; height:43px; flex-shrink:0; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:2px; font-size:10px; border:1px solid transparent; cursor:pointer; }}
  .btn:hover {{ background-color:{hover}; border-top-color:{highlight}; border-left-color:{highlight}; border-right-color:{shadow}; border-bottom-color:{shadow}; }}
  .btn:active {{ border-top-color:{shadow}; border-left-color:{shadow}; border-right-color:{highlight}; border-bottom-color:{highlight}; }}
  .btn-off {{ color:{muted}; }}
  .btn-off:hover {{ background-color:{nav}; border-color:transparent; }}
  .glyph {{ height:23px; width:25px; display:flex; align-items:center; justify-content:center; position:relative; }}
  .nav-arrow {{ width:23px; height:17px; background-color:#38784E; border-bottom:1px solid {shadow}; clip-path:polygon(0% 50%, 42% 0%, 42% 29%, 100% 29%, 100% 71%, 42% 71%, 42% 100%); }}
  .nav-arrow-right {{ transform:rotate(180deg); }}
  .btn-off .nav-arrow {{ background-color:{muted}; }}
  .ring {{ width:19px; height:19px; border-radius:10px; border:3px solid {accent}; border-top-color:transparent; }}
  .arrow {{ position:absolute; top:0; right:0; width:0; height:0; border-top:5px solid transparent; border-bottom:5px solid transparent; border-left:7px solid {accent}; }}
  .home-roof {{ position:absolute; top:0; width:0; height:0; border-left:12px solid transparent; border-right:12px solid transparent; border-bottom:11px solid #945A46; }}
  .home-wall {{ position:absolute; bottom:1px; width:16px; height:12px; background-color:#D5B983; border:1px solid {shadow}; }}
  .home-door {{ position:absolute; bottom:1px; width:5px; height:8px; background-color:{accent}; }}
  .debug-glyph {{ font-family:monospace; font-size:17px; font-weight:bold; color:{accent}; }}
  .toolbar-divider {{ height:34px; width:2px; margin:0 4px; border-left:1px solid {shadow}; border-right:1px solid {highlight}; }}
  .brand {{ display:flex; align-items:center; gap:8px; margin-left:auto; padding:0 5px; min-width:0; }}
  .brand-word {{ font-family:Georgia, serif; font-weight:bold; font-size:17px; color:{accent}; white-space:nowrap; }}
  .compass {{ width:36px; height:36px; flex-shrink:0; background-color:#173658; border-top:2px solid {shadow}; border-left:2px solid {shadow}; border-right:2px solid {highlight}; border-bottom:2px solid {highlight}; display:flex; align-items:center; justify-content:center; }}
  .compass-disc {{ width:26px; height:26px; border:1px solid #B6C6D9; border-radius:13px; position:relative; }}
  .compass-needle {{ position:absolute; left:9px; top:2px; width:6px; height:20px; background-color:#E1EAF5; clip-path:polygon(50% 0%, 100% 70%, 50% 55%, 0% 70%); transform:rotate(28deg); }}
  .compass-loading .compass-disc {{ border:2px solid #E7CB83; }}
  .compass-loading .compass-needle {{ background-color:#E7CB83; }}

  .location-row {{ height:32px; display:flex; align-items:center; gap:8px; padding:0 10px; background-color:{nav}; border-top:1px solid {highlight}; border-bottom:1px solid {shadow}; }}
  .location-label {{ font-size:11px; flex-shrink:0; }}
  .bar {{ flex:1; min-width:0; height:24px; background-color:{pill}; border-top:2px solid {shadow}; border-left:2px solid {shadow}; border-bottom:2px solid {highlight}; border-right:2px solid {highlight}; display:flex; align-items:center; padding:0 7px; gap:7px; overflow:hidden; cursor:text; }}
  .bar-focused {{ background-color:{pill_focus}; border-top-color:{accent}; border-left-color:{accent}; }}
  .url {{ font-size:12px; color:{ink}; white-space:nowrap; overflow:hidden; min-width:0; }}
  .caret {{ width:1px; height:15px; background-color:{accent}; flex-shrink:0; }}
  .lock {{ display:flex; flex-direction:column; align-items:center; flex-shrink:0; }}
  .shackle {{ width:6px; height:5px; border:1.5px solid {favicon_ink}; border-bottom-width:0; border-radius:3px 3px 0 0; }}
  .lock-body {{ width:9px; height:7px; background-color:{favicon_ink}; }}
  .insecure-dot {{ width:7px; height:7px; border:1px solid {muted}; border-radius:4px; flex-shrink:0; }}
  .connection-label {{ font-size:10px; color:{muted}; white-space:nowrap; flex-shrink:0; }}

  /* The paths themselves change on each native spring tick. */
  .tabstrip {{ height:38px; background-color:{strip}; display:flex; align-items:flex-end; gap:8px; padding:0 8px; border-top:1px solid {highlight}; border-bottom:2px solid {shadow}; overflow:hidden; }}
  .tab-track {{ flex:1; min-width:0; height:32px; position:relative; overflow:hidden; }}
  .tab {{ position:absolute; top:0; height:32px; color:{ink}; cursor:pointer; }}
  .tab-shape {{ position:absolute; left:0; top:0; width:100%; height:32px; background-color:{nav}; border-top:1px solid {highlight}; border-bottom:1px solid {shadow}; pointer-events:none; }}
  .tab-active .tab-shape {{ background-color:{tab_active}; border-top:2px solid {highlight}; border-bottom:0; }}
  .tab-content {{ position:absolute; left:13px; right:10px; top:3px; height:26px; display:flex; align-items:center; gap:6px; overflow:hidden; }}
  .favicon {{ width:13px; height:15px; flex-shrink:0; border:1px solid {highlight}; background-color:{favicon_bg}; color:#FFFFFF; font-size:9px; line-height:13px; text-align:center; }}
  .tab-title {{ font-size:11px; white-space:nowrap; overflow:hidden; flex:1; min-width:0; }}
  .tab-active .tab-title {{ font-weight:bold; }}
  .tab-close {{ width:19px; height:19px; line-height:16px; text-align:center; font-size:15px; color:{favicon_ink}; flex-shrink:0; border:1px solid transparent; }}
  .tab-close:hover {{ border-top-color:{highlight}; border-left-color:{highlight}; border-right-color:{shadow}; border-bottom-color:{shadow}; background-color:{hover}; }}
  .tab-neck {{ position:absolute; top:0; height:32px; pointer-events:none; }}
  .tab-overflow {{ position:absolute; top:4px; height:25px; width:22px; line-height:21px; text-align:center; font-size:16px; background-color:{nav}; border-top:1px solid {highlight}; border-left:1px solid {highlight}; border-right:1px solid {shadow}; border-bottom:1px solid {shadow}; cursor:pointer; }}
  .newtab {{ width:30px; height:27px; flex-shrink:0; line-height:22px; margin-bottom:2px; font-size:20px; color:{ink}; text-align:center; background-color:{nav}; border-top:1px solid {highlight}; border-left:1px solid {highlight}; border-right:1px solid {shadow}; border-bottom:1px solid {shadow}; cursor:pointer; }}
  .newtab:hover, .tab-overflow:hover {{ background-color:{hover}; }}

  .menu {{ position:absolute; left:8px; top:24px; width:205px; background-color:{menu_bg}; border-top:2px solid {highlight}; border-left:2px solid {highlight}; border-right:2px solid {shadow}; border-bottom:2px solid {shadow}; padding:3px; z-index:90; }}
  .menu-item {{ font-size:12px; color:{ink}; padding:7px 12px; cursor:pointer; }}
  .menu-item:hover {{ background-color:{accent}; color:{pill_focus}; }}
  .menu-sep {{ height:2px; border-top:1px solid {shadow}; border-bottom:1px solid {highlight}; margin:3px 2px; }}
  #debugger-pane {{ position:absolute; left:0; right:0; bottom:0; height:{debugger_height}px; background-color:{menu_bg}; color:{ink}; overflow:hidden; z-index:50; border-top:2px solid {highlight}; }}
  @media (max-width:600px) {{ .brand-word, .connection-label, .menu-caption {{ display:none; }} .btn {{ width:48px; }} .toolbar-divider {{ margin:0 1px; }} .brand {{ padding:0; }} }}

  /* supervision bar: overlays the top of the page on the session tab */
  .supbar {{
    position: absolute;
    left: 0; right: 0; top: {chrome_height}px;
    z-index: 80;
    height: 46px;
    background-color: {menu_bg};
    border-bottom: 1px solid {hairline};
    display: flex; flex-direction: row; align-items: center;
    padding: 0 12px;
    gap: 8px;
  }}
  .sup-title {{ font-size: 12px; color: {ink}; flex-shrink:0; }}
  .sup-state {{ font-size: 11px; color: {accent}; flex-shrink:0; }}
  .sup-detail {{ font-size: 11px; color: {favicon_ink}; flex: 1; min-width:0; overflow:hidden; }}
  .sup-btn {{
    font-size: 12px; color: {ink}; flex-shrink:0;
    background-color: {pill};
    border-top:1px solid {highlight}; border-left:1px solid {highlight};
    border-right:1px solid {shadow}; border-bottom:1px solid {shadow};
    padding: 5px 10px;
  }}
  .sup-btn:hover {{ background-color: {hover}; }}
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
    left: 0; right: 0; bottom: {hud_bottom}px;
    z-index:60;
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
    left: 0; right: 0; top: {progress_top}px;
    height: 2px;
    background-color: {accent};
    pointer-events:none;
  }}
</style>
</head>
<body>
  <div class="menubar">
    <div class="menubtn {menu_class}" id="btn-menu" role="button" aria-haspopup="menu" aria-expanded="{menu_open}">Browser<div class="down"></div></div>
    <div class="menu-caption">Walkaway Navigator &#183; Classic</div>
  </div>
  <div class="nav">
    <div class="grip" aria-hidden="true"></div>
    <div class="{back_class}" id="btn-back" role="button" aria-disabled="{back_disabled}" title="Back"><div class="glyph"><div class="nav-arrow"></div></div><span>Back</span></div>
    <div class="{forward_class}" id="btn-forward" role="button" aria-disabled="{forward_disabled}" title="Forward"><div class="glyph"><div class="nav-arrow nav-arrow-right"></div></div><span>Forward</span></div>
    <div class="btn" id="btn-reload" role="button" title="Reload"><div class="glyph"><div class="ring"></div><div class="arrow"></div></div><span>Reload</span></div>
    <div class="btn" id="btn-home" role="button" title="Home"><div class="glyph"><div class="home-roof"></div><div class="home-wall"></div><div class="home-door"></div></div><span>Home</span></div>
    <div class="toolbar-divider" aria-hidden="true"></div>
    <div class="btn" id="btn-debugger" role="button" aria-pressed="{debugger_open}" title="Local Hermes JavaScript debugger"><div class="glyph debug-glyph">{{js}}</div><span>Debugger</span></div>
    <div class="brand"><div class="brand-word">Walkaway</div><div class="compass {compass_class}" id="walkaway-throbber" role="status" aria-label="{load_label}"><div class="compass-disc"><div class="compass-needle"></div></div></div></div>
  </div>
  <div class="location-row">
    <div class="location-label">Location:</div>
    <div class="{bar_class}" id="address" role="textbox" aria-label="Location" title="Enter a URL or search">
      {security}<div class="url">{url}</div>{caret}
    </div>
    <div class="connection-label">{connection_label}</div>
  </div>
  <div class="tabstrip">
    <div class="tab-track" role="tablist" aria-label="Browser tabs">{tabs}</div>
    <div class="newtab" id="btn-newtab" role="button" aria-label="New tab" title="New tab">+</div>
  </div>
  {xray_layer}
  {debugger}
  {menu}
  {supbar}
  {progress}
</body>
</html>"#,
        surface_height = surface_height,
        chrome_height = CHROME_HEIGHT,
        debugger_height = debugger_height(surface_height),
        hud_bottom = if state.debugger_html.is_some() {
            debugger_height(surface_height)
        } else {
            0.0
        },
        progress_top = CHROME_HEIGHT - 2.0,
        strip = p.strip,
        highlight = p.highlight,
        shadow = p.shadow,
        tab_active = p.tab_active,
        nav = p.nav,
        hairline = p.hairline,
        pill = p.pill,
        pill_focus = p.pill_focus,
        ink = p.ink,
        muted = p.muted,
        hover = p.hover,
        menu_bg = p.menu_bg,
        favicon_bg = p.favicon_bg,
        favicon_ink = p.favicon_ink,
        accent = p.accent,
        tabs = tabs,
        back_class = back_class,
        forward_class = forward_class,
        bar_class = bar_class,
        security = if state.secure {
            r#"<div class="lock"><div class="shackle"></div><div class="lock-body"></div></div>"#
        } else {
            r#"<div class="insecure-dot"></div>"#
        },
        url = escape_html(state.url),
        caret = caret,
        xray_layer = xray_layer,
        menu = menu,
        supbar = supbar,
        progress = progress,
        debugger = debugger,
        debugger_open = state.debugger_html.is_some(),
        menu_open = state.menu_open,
        menu_class = if state.menu_open { "menubtn-open" } else { "" },
        back_disabled = !state.can_back,
        forward_disabled = !state.can_forward,
        compass_class = if state.loading { "compass-loading" } else { "" },
        load_label = if state.loading {
            "Loading page"
        } else {
            "Page ready"
        },
        connection_label = if state.secure {
            "Secure connection"
        } else {
            "Not HTTPS"
        },
    )
}

fn render_tabs(state: &ChromeState<'_>, p: &Palette) -> String {
    use crate::gooey_tabs::{TAB_HEIGHT, layout, neck_path, tab_path};
    let selected = state.tabs.iter().position(|tab| tab.active).unwrap_or(0);
    let width = if state.surface_width.is_finite() {
        state.surface_width
    } else {
        800.0
    };
    // Strip padding (16), fixed new-tab control (30), and gap (8).
    let available = (width - 54.0).max(1.0);
    let geometry = layout(available, state.tabs.len(), selected, state.tab_animation);
    let mut html = String::new();
    for seam in &geometry.seams {
        let path = neck_path(seam.gap, seam.span, TAB_HEIGHT);
        if path.is_empty() {
            continue;
        }
        let color = if seam.left_index == selected || seam.right_index == selected {
            p.tab_active
        } else {
            p.nav
        };
        html.push_str(&format!(
            r#"<div class="tab-neck" aria-hidden="true" style="left:{x:.3}px;width:{span:.3}px;clip-path:path('{path}');background-color:{color};"></div>"#,
            x = seam.x, span = seam.span,
        ));
    }
    for slot in &geometry.slots {
        let index = slot.index;
        let tab = &state.tabs[index];
        let path = tab_path(
            slot.width,
            TAB_HEIGHT,
            slot.left_open,
            slot.right_open,
            slot.stretch,
        );
        let class = if tab.active { "tab tab-active" } else { "tab" };
        let title = escape_html(tab.title);
        html.push_str(&format!(
            r#"<div class="{class}" id="tab-{index}" role="tab" aria-selected="{active}" title="{title}" style="left:{x:.3}px;width:{width:.3}px;">
              <div class="tab-shape" aria-hidden="true" style="clip-path:path('{path}');"></div>
              <div class="tab-content"><div class="favicon" aria-hidden="true">{letter}</div><div class="tab-title">{title}</div><div class="tab-close" id="tab-close-{index}" role="button" aria-label="Close {title}">&#215;</div></div>
            </div>"#,
            active = tab.active, x = slot.x, width = slot.width, letter = escape_html(&first_letter(tab.title)),
        ));
    }
    // The IDs target real hidden neighbors using the existing tab selection
    // handler. Never duplicate an ID or put the new-tab control offscreen.
    if let Some(index) = geometry.previous {
        html.push_str(&format!(r#"<div class="tab-overflow" id="tab-{index}" role="button" aria-label="Earlier tabs" title="Select previous hidden tab" style="left:0;">&#8249;</div>"#));
    }
    if let Some(index) = geometry.next {
        html.push_str(&format!(r#"<div class="tab-overflow" id="tab-{index}" role="button" aria-label="Later tabs" title="Select next hidden tab" style="right:0;">&#8250;</div>"#));
    }
    html
}

fn button_class(enabled: bool) -> &'static str {
    if enabled { "btn" } else { "btn btn-off" }
}

/// Tab favicon placeholder: the first letter of the title. Favicons are not
/// decoded, so a generated initial is used rather than inventing an icon.
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
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state<'a>(tabs: &'a [TabInfo<'a>]) -> ChromeState<'a> {
        ChromeState {
            url: "https://example.test/",
            focused: false,
            can_back: true,
            can_forward: false,
            loading: false,
            tabs,
            menu_open: false,
            debugger_html: None,
            tab_animation: None,
            surface_width: 800.0,
            secure: true,
            surface_height: 600.0,
            theme: Theme::Light,
            supervision: None,
            telemetry: None,
        }
    }

    #[test]
    fn escapes_text_and_both_attribute_quotes() {
        assert_eq!(escape_html("<&>\"' café"), "&lt;&amp;&gt;&quot;&#39; café");
        assert_eq!(first_letter("< 7 wonders"), "7");
        assert_eq!(first_letter(" -- "), "?");
        assert_eq!(first_letter("éclair"), "É");
        let attack = "</div><script id='injected'>x</script>\"&";
        let tabs = [TabInfo {
            title: attack,
            active: true,
        }];
        let mut chrome = state(&tabs);
        chrome.url = attack;
        let html = render(&chrome);
        assert!(!html.contains(attack));
        assert!(!html.contains("<script"));
        assert!(html.contains(&escape_html(attack)));
    }

    #[test]
    fn stable_control_ids_and_functional_menu_are_preserved() {
        let tabs = [TabInfo {
            title: "Local",
            active: true,
        }];
        let mut chrome = state(&tabs);
        chrome.menu_open = true;
        let html = render(&chrome);
        for id in [
            "btn-back",
            "btn-forward",
            "btn-reload",
            "btn-home",
            "address",
            "btn-menu",
            "btn-newtab",
            "btn-debugger",
            "menu-home",
            "menu-reload",
            "menu-back",
            "menu-forward",
            "menu-close-tab",
            "menu-debugger",
            "tab-0",
            "tab-close-0",
        ] {
            assert_eq!(html.matches(&format!("id=\"{id}\"")).count(), 1, "#{id}");
        }
        assert!(!html.contains(">File<"));
        assert!(!html.contains(">Edit<"));
        assert!(html.contains("clip-path:path('M"));
        assert!(html.contains("Location:"));
    }

    #[test]
    fn trusted_debugger_fragment_and_height_contract() {
        let tabs = [TabInfo {
            title: "Local",
            active: true,
        }];
        let mut chrome = state(&tabs);
        let pane = "<div id=\"debugger-pane\">Local Hermes scratchpad</div>";
        chrome.debugger_html = Some(pane);
        let html = render(&chrome);
        assert!(html.contains(pane));
        assert!(html.contains("aria-pressed=\"true\""));
        assert_eq!(debugger_height(800.0), DEBUGGER_HEIGHT);
        assert_eq!(debugger_height(CHROME_HEIGHT + 80.0), 80.0);
        assert_eq!(debugger_height(50.0), 0.0);
        assert_eq!(debugger_height(f64::NAN), 0.0);
        assert_eq!(debugger_height(f64::INFINITY), 0.0);
    }

    #[test]
    fn supervision_actions_and_xray_offset_survive_restyle() {
        let tabs = [TabInfo {
            title: "Supervised",
            active: true,
        }];
        let mut chrome = state(&tabs);
        chrome.supervision = Some(SupervisionInfo {
            title: "Task <untrusted>",
            state: "awaiting_decision",
            detail: "Review & decide",
            can_decide: true,
            can_take: true,
            can_release: false,
            can_stop: true,
        });
        chrome.telemetry = Some(EngineTelemetry {
            layout_observe_ms: 0.25,
            rss_mb: 32.5,
            form_signature: Some("<sig>".into()),
            matched_recipe_id: None,
            recalled_count: 0,
            overlays: vec![XRayBox {
                id: "el_1".into(),
                role_tag: "textbox".into(),
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 22.0,
                is_vault_secret: true,
                is_frozen_target: false,
                freeze_fp_short: None,
            }],
        });
        let html = render(&chrome);
        for id in [
            "sup-bar",
            "sup-approve",
            "sup-reject",
            "sup-take",
            "sup-stop",
            "xray-vault-shield",
            "xray-telemetry-bar",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "#{id}");
        }
        assert!(!html.contains("id=\"sup-done\""));
        assert!(html.contains("top: 144px;"));
        assert!(html.contains("top:164.0px;"));
        assert!(html.contains("Task &lt;untrusted&gt;"));
        assert!(html.contains("FormSignature: &lt;sig&gt;"));
    }

    #[test]
    fn many_tabs_leave_new_tab_and_hidden_neighbors_reachable() {
        let tabs: Vec<_> = (0..80)
            .map(|index| TabInfo {
                title: "Long tab title",
                active: index == 40,
            })
            .collect();
        let mut chrome = state(&tabs);
        chrome.surface_width = 360.0;
        let html = render(&chrome);
        assert!(html.contains("id=\"tab-40\""));
        assert!(html.contains("aria-label=\"Earlier tabs\""));
        assert!(html.contains("aria-label=\"Later tabs\""));
        assert_eq!(html.matches("id=\"btn-newtab\"").count(), 1);
        let ids: Vec<_> = html
            .split("id=\"")
            .skip(1)
            .map(|s| s.split('"').next().unwrap())
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
    }
}
