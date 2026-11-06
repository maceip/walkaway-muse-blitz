//! The browser chrome, rendered by the same HTML/CSS engine as the page.
//!
//! Netscape 6 Modern-inspired silver-blue chrome: sculpted circular tools
//! and a compass with honest loading state. Live macOS tabs and their inline
//! addresses belong to the original Swift strip in `native_tabs`; an empty
//! `tabs` slice reserves its one native slot. `gooey_tabs` is only the existing
//! offscreen/non-native fallback, with the active address inside its tab, not
//! a claim of Swift parity. No page JavaScript participates in chrome interaction.
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
    metal_top: &'static str,
    metal_mid: &'static str,
    metal_bottom: &'static str,
    tool_top: &'static str,
    tool_mid: &'static str,
    tool_bottom: &'static str,
    tool_ink: &'static str,
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
            accent: "#3D426F",
            metal_top: "#F1F4FA",
            metal_mid: "#D6DFED",
            metal_bottom: "#A2B1C9",
            tool_top: "#EAF3FF",
            tool_mid: "#9DB3D3",
            tool_bottom: "#627EA7",
            tool_ink: "#263A61",
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
            accent: "#C1C9F4",
            metal_top: "#6D7F9C",
            metal_mid: "#4E617F",
            metal_bottom: "#303D58",
            tool_top: "#C1D5EF",
            tool_mid: "#7E98BC",
            tool_bottom: "#4C648D",
            tool_ink: "#142744",
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

/// Menu y0..24, Modern navigation y24..74, native address-in-tabs y74..112.
/// Main uses this same offset for page layout, pointer input, and screenshots.
pub const CHROME_HEIGHT: f64 = 112.0;
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
    // Blitz resolves this shell at fixed timestamps: no CSS animation that
    // merely appears to run, no invented completion fraction. Native chrome
    // owns the live NSProgressIndicator; this stripe and label express only
    // the same real loading boolean. Keep the stripe above the native tab row.
    let progress = if state.loading {
        r#"<div class="progress progress-indeterminate" id="page-load-progress" role="progressbar" aria-label="Loading page" aria-valuetext="Loading…"></div>"#
    } else {
        ""
    };
    // Keep the existing action ID. The host treats it as cancellation while
    // loading and as Reload otherwise; there is no second inert Stop control.
    let (reload_label, reload_title, reload_glyph) = if state.loading {
        (
            "Stop",
            "Stop loading page",
            r#"<div class="stop-glyph"></div>"#,
        )
    } else {
        (
            "Reload",
            "Reload page",
            r#"<div class="ring"></div><div class="arrow"></div>"#,
        )
    };
    let (load_label, load_status) = if state.loading {
        ("Loading page", "Loading…")
    } else if state.url.is_empty() {
        ("Ready", "Ready")
    } else {
        ("Page ready", "Done")
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

  /* Modern's pale steel shell and blue-violet accents; only real commands. */
  .menubar {{ height:24px; display:flex; align-items:center; gap:4px; padding:0 10px; background-color:{metal_mid}; background-image:linear-gradient(to bottom, {metal_top} 0%, {metal_mid} 100%); border-top:1px solid {highlight}; border-bottom:1px solid {hairline}; }}
  .menubtn {{ height:21px; padding:1px 9px; font-size:11px; display:flex; align-items:center; gap:7px; border:1px solid transparent; border-radius:7px 7px 3px 3px; cursor:pointer; }}
  .menubtn:hover, .menubtn-open {{ background-color:{hover}; background-image:linear-gradient(to bottom, {metal_top} 0%, {metal_bottom} 100%); border-color:{hairline}; box-shadow:inset 0 1px 0 {highlight}; }}
  .down {{ width:0; height:0; border-left:3px solid transparent; border-right:3px solid transparent; border-top:4px solid {ink}; }}

  /* Explicit stops are painted by Blitz; sculpted faces, not square OS bevels. */
  .nav {{ height:50px; background-color:{nav}; background-image:linear-gradient(to bottom, {metal_top} 0%, {metal_mid} 42%, {nav} 60%, {metal_bottom} 100%); border-top:1px solid {highlight}; border-bottom:1px solid {shadow}; display:flex; align-items:center; padding:0 8px; gap:4px; }}
  .grip {{ width:5px; height:30px; flex-shrink:0; border:1px solid {hairline}; border-radius:4px; background-color:{metal_bottom}; box-shadow:inset 1px 0 0 {highlight}; margin-right:3px; }}
  .btn {{ width:62px; height:46px; flex-shrink:0; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:1px; font-size:11px; line-height:12px; border:1px solid transparent; border-radius:11px; cursor:pointer; }}
  .btn:hover {{ background-color:{hover}; background-image:linear-gradient(to bottom, {metal_top} 0%, {tool_mid} 100%); border-color:{hairline}; box-shadow:inset 0 1px 0 {highlight}; }}
  .btn:active, .btn[aria-pressed="true"] {{ background-color:{metal_bottom}; background-image:linear-gradient(to bottom, {metal_bottom} 0%, {metal_mid} 100%); border-color:{shadow}; }}
  .btn-off {{ color:{muted}; cursor:default; }}
  .btn-off:hover {{ background-color:transparent; background-image:none; border-color:transparent; box-shadow:none; }}
  .glyph {{ height:28px; width:28px; flex-shrink:0; display:flex; align-items:center; justify-content:center; position:relative; }}
  .tool-face, .nav-disc {{ background-color:{tool_mid}; background-image:linear-gradient(to bottom, {tool_top} 0%, {tool_mid} 48%, {tool_bottom} 100%); border:1px solid {shadow}; box-shadow:inset 0 1px 0 #F1F7FF, inset 0 -1px 0 #56729B, 0 1px 0 {highlight}; }}
  .tool-face {{ border-radius:8px; }}
  .nav-disc {{ height:30px; width:30px; border-radius:50%; }}
  .btn-off .nav-disc {{ background-color:{nav}; background-image:linear-gradient(to bottom, {metal_top} 0%, {nav} 100%); border-color:{hairline}; box-shadow:inset 0 1px 0 {highlight}; }}
  .nav-arrow {{ width:19px; height:15px; background-color:{tool_ink}; background-image:linear-gradient(to bottom, #617BA7 0%, {tool_ink} 100%); clip-path:polygon(0% 50%, 45% 0%, 45% 30%, 100% 30%, 100% 70%, 45% 70%, 45% 100%); }}
  .nav-arrow-right {{ transform:rotate(180deg); }}
  .btn-off .nav-arrow {{ background-color:{muted}; background-image:none; }}
  .ring {{ width:18px; height:18px; border-radius:50%; border:3px solid {tool_ink}; border-top-color:transparent; transform:rotate(-28deg); }}
  .arrow {{ position:absolute; top:2px; right:2px; width:0; height:0; border-top:4px solid transparent; border-bottom:4px solid transparent; border-left:6px solid {tool_ink}; transform:rotate(28deg); }}
  .stop-glyph {{ width:15px; height:15px; border:1px solid #702C40; border-radius:3px; background-color:#9B4058; background-image:linear-gradient(to bottom, #D18395 0%, #8B334D 100%); box-shadow:inset 0 1px 0 #F0B8C1; }}
  .home-roof {{ position:absolute; top:3px; width:0; height:0; border-left:10px solid transparent; border-right:10px solid transparent; border-bottom:9px solid {tool_ink}; }}
  .home-wall {{ position:absolute; bottom:3px; width:14px; height:11px; background-color:#DFE9F8; background-image:linear-gradient(to bottom, #EFF4FF 0%, #A9BBD7 100%); border:1px solid {tool_ink}; }}
  .home-door {{ position:absolute; bottom:3px; width:4px; height:7px; background-color:{tool_ink}; }}
  .debug-glyph {{ font-family:monospace; font-size:15px; font-weight:bold; color:{tool_ink}; }}
  .toolbar-divider {{ height:30px; width:2px; flex-shrink:0; margin:0 4px; border-left:1px solid {hairline}; border-right:1px solid {highlight}; border-radius:1px; }}
  .brand {{ display:flex; align-items:center; gap:9px; margin-left:auto; padding:0 2px; min-width:0; flex-shrink:0; }}
  .load-status {{ font-size:10px; line-height:13px; color:{ink}; white-space:nowrap; }}
  .load-status-busy {{ color:{accent}; font-weight:bold; }}
  .compass {{ width:42px; height:42px; flex-shrink:0; border:1px solid {shadow}; border-radius:50%; background-color:{tool_mid}; background-image:linear-gradient(to bottom, #F5F8FF 0%, #A5B9D5 48%, #51698F 100%); box-shadow:inset 0 1px 0 #FFFFFF, 0 1px 0 {highlight}; display:flex; align-items:center; justify-content:center; }}
  .compass-disc {{ width:32px; height:32px; border:2px solid #536C97; border-radius:50%; position:relative; background-color:#242C58; background-image:radial-gradient(circle at 35% 25%, #657FB2 0%, #303D6D 52%, #1C2349 100%); }}
  .compass-orbit {{ position:absolute; left:1px; top:6px; width:26px; height:15px; border:1px solid #A9BCD9; border-radius:50%; transform:rotate(-28deg); }}
  .compass-needle {{ position:absolute; left:9px; top:2px; width:10px; height:24px; background-color:#E5EFFF; background-image:linear-gradient(to bottom, #FFFFFF 0%, #C5D6ED 50%, #6C8ABA 100%); clip-path:polygon(50% 0%, 100% 80%, 50% 60%, 0% 80%); transform:rotate(28deg); }}
  .compass-star {{ position:absolute; left:5px; top:5px; width:2px; height:2px; border-radius:50%; background-color:#F3F8FF; box-shadow:16px 2px 0 #F3F8FF, 12px 18px 0 #A9BFDF; }}
  .compass-loading .compass-disc {{ border-color:#E9C780; }}
  .compass-loading .compass-orbit {{ border-color:#FFE7B0; }}

  /* Inline address in the active fallback tab only; native Swift owns its field. */
  .tab-address {{ flex:1; min-width:0; height:24px; display:flex; align-items:center; gap:4px; padding:0 4px; border:1px solid transparent; border-radius:4px; overflow:hidden; cursor:text; }}
  .tab-address-focused {{ background-color:{pill_focus}; border-color:{accent}; }}
  .url {{ font-size:12px; color:{ink}; white-space:nowrap; overflow:hidden; min-width:0; }}
  .url-placeholder {{ color:{muted}; }}
  .caret {{ width:1px; height:15px; background-color:{accent}; flex-shrink:0; }}
  .lock {{ display:flex; flex-direction:column; align-items:center; flex-shrink:0; }}
  .shackle {{ width:6px; height:5px; border:1.5px solid {favicon_ink}; border-bottom-width:0; border-radius:3px 3px 0 0; }}
  .lock-body {{ width:9px; height:7px; background-color:{favicon_ink}; border-radius:1px; }}
  .insecure-dot {{ width:7px; height:7px; border:1px solid {muted}; border-radius:50%; flex-shrink:0; }}

  /* Existing offscreen/non-native fallback only. Native Swift tabs are untouched. */
  .tabstrip {{ height:38px; background-color:{strip}; display:flex; align-items:flex-end; gap:8px; padding:0 8px; border-top:1px solid {highlight}; border-bottom:2px solid {shadow}; overflow:hidden; }}
  .tab-track {{ flex:1; min-width:0; height:32px; position:relative; overflow:hidden; }}
  .tab {{ position:absolute; top:0; height:32px; color:{ink}; cursor:pointer; }}
  .tab-shape {{ position:absolute; left:0; top:0; width:100%; height:32px; background-color:{nav}; border-top:1px solid {highlight}; border-bottom:1px solid {shadow}; pointer-events:none; }}
  .tab-active .tab-shape {{ background-color:{tab_active}; border-top:2px solid {highlight}; border-bottom:0; }}
  .tab-content {{ position:absolute; left:13px; right:10px; top:3px; height:26px; display:flex; align-items:center; gap:6px; overflow:hidden; }}
  .favicon {{ width:13px; height:15px; flex-shrink:0; border:1px solid {highlight}; background-color:{favicon_bg}; color:#FFFFFF; font-size:9px; line-height:13px; text-align:center; }}
  .tab-title {{ font-size:11px; white-space:nowrap; overflow:hidden; flex:1; min-width:0; }}
  .tab-close {{ width:19px; height:19px; line-height:16px; text-align:center; font-size:15px; color:{favicon_ink}; flex-shrink:0; border:1px solid transparent; }}
  .tab-close:hover {{ border-top-color:{highlight}; border-left-color:{highlight}; border-right-color:{shadow}; border-bottom-color:{shadow}; background-color:{hover}; }}
  .tab-neck {{ position:absolute; top:0; height:32px; pointer-events:none; }}
  .tab-overflow {{ position:absolute; top:4px; height:25px; width:22px; line-height:21px; text-align:center; font-size:16px; background-color:{nav}; border-top:1px solid {highlight}; border-left:1px solid {highlight}; border-right:1px solid {shadow}; border-bottom:1px solid {shadow}; cursor:pointer; }}
  .newtab {{ width:30px; height:27px; flex-shrink:0; line-height:22px; margin-bottom:2px; font-size:20px; color:{ink}; text-align:center; background-color:{nav}; border-top:1px solid {highlight}; border-left:1px solid {highlight}; border-right:1px solid {shadow}; border-bottom:1px solid {shadow}; cursor:pointer; }}
  .newtab:hover, .tab-overflow:hover {{ background-color:{hover}; }}

  .menu {{ position:absolute; left:8px; top:24px; width:205px; background-color:{menu_bg}; background-image:linear-gradient(to bottom, {metal_top} 0%, {menu_bg} 100%); border:1px solid {shadow}; border-radius:6px; box-shadow:inset 0 1px 0 {highlight}, 0 3px 8px rgba(25,35,60,0.25); padding:3px; z-index:90; }}
  .menu-item {{ font-size:12px; color:{ink}; padding:7px 12px; border-radius:3px; cursor:pointer; }}
  .menu-item:hover {{ background-color:#3D426F; background-image:linear-gradient(to bottom, #6579A6 0%, #3D426F 100%); color:#FFFFFF; }}
  .menu-sep {{ height:2px; border-top:1px solid {hairline}; border-bottom:1px solid {highlight}; margin:3px 2px; }}
  #debugger-pane {{ position:absolute; left:0; right:0; bottom:0; height:{debugger_height}px; background-color:{menu_bg}; color:{ink}; overflow:hidden; z-index:50; border-top:2px solid {highlight}; }}
  @media (max-width:600px) {{ .btn {{ width:48px; font-size:10px; }} .toolbar-divider {{ margin:0 1px; }} .brand {{ padding:0; gap:5px; }} }}
  @media (max-width:420px) {{ .nav {{ padding:0 6px; gap:3px; }} .grip, .toolbar-divider {{ display:none; }} .btn {{ width:44px; }} .compass {{ width:34px; height:34px; }} .compass-disc {{ width:28px; height:28px; }} }}

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
    position:absolute;
    left:0; right:0; top:71px;
    height:3px;
    background-color:{tool_bottom};
    background-image:repeating-linear-gradient(to right, {tool_bottom} 0px, {tool_bottom} 8px, {metal_mid} 8px, {metal_mid} 14px);
    pointer-events:none;
  }}
</style>
</head>
<body data-chrome-theme="netscape-modern">
  <div class="menubar" id="chrome-menubar">
    <div class="menubtn {menu_class}" id="btn-menu" role="button" aria-haspopup="menu" aria-expanded="{menu_open}">Browser<div class="down" aria-hidden="true"></div></div>
  </div>
  <div class="nav" id="chrome-navigation" role="toolbar" aria-label="Navigation">
    <div class="grip" aria-hidden="true"></div>
    <div class="{back_class}" id="btn-back" role="button" aria-label="Back" aria-disabled="{back_disabled}" title="Back"><div class="glyph nav-disc" aria-hidden="true"><div class="nav-arrow"></div></div><span>Back</span></div>
    <div class="{forward_class}" id="btn-forward" role="button" aria-label="Forward" aria-disabled="{forward_disabled}" title="Forward"><div class="glyph nav-disc" aria-hidden="true"><div class="nav-arrow nav-arrow-right"></div></div><span>Forward</span></div>
    <div class="btn" id="btn-reload" role="button" aria-label="{reload_title}" title="{reload_title}"><div class="glyph tool-face" aria-hidden="true">{reload_glyph}</div><span>{reload_label}</span></div>
    <div class="btn" id="btn-home" role="button" aria-label="Home" title="Home"><div class="glyph tool-face" aria-hidden="true"><div class="home-roof"></div><div class="home-wall"></div><div class="home-door"></div></div><span>Home</span></div>
    <div class="toolbar-divider" aria-hidden="true"></div>
    <div class="btn" id="btn-debugger" role="button" aria-label="JavaScript debugger" aria-pressed="{debugger_open}" title="Local Hermes JavaScript debugger"><div class="glyph tool-face debug-glyph" aria-hidden="true">JS</div><span>Debugger</span></div>
    <div class="brand"><div class="load-status {status_class}" id="page-load-status" role="status" aria-live="polite">{load_status}</div><div class="compass {compass_class}" id="walkaway-throbber" role="img" aria-label="{load_label}" title="{load_label}"><div class="compass-disc" aria-hidden="true"><div class="compass-orbit"></div><div class="compass-star"></div><div class="compass-needle"></div></div></div></div>
  </div>
  {tab_row}
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
        metal_top = p.metal_top,
        metal_mid = p.metal_mid,
        metal_bottom = p.metal_bottom,
        tool_top = p.tool_top,
        tool_mid = p.tool_mid,
        tool_bottom = p.tool_bottom,
        tool_ink = p.tool_ink,
        tab_row = if state.tabs.is_empty() {
            r#"<div class="tabstrip" id="native-tab-slot" aria-hidden="true"></div>"#.to_string()
        } else {
            format!(
                r#"<div class="tabstrip"><div class="tab-track" role="tablist" aria-label="Browser tabs">{tabs}</div><div class="newtab" id="btn-newtab" role="button" aria-label="New tab" title="New tab">+</div></div>"#
            )
        },
        back_class = back_class,
        forward_class = forward_class,
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
        status_class = if state.loading {
            "load-status-busy"
        } else {
            ""
        },
        load_label = load_label,
        load_status = load_status,
        reload_label = reload_label,
        reload_title = reload_title,
        reload_glyph = reload_glyph,
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
        let content = if tab.active {
            let address_class = if state.focused {
                "tab-address tab-address-focused"
            } else {
                "tab-address"
            };
            let security = if state.url.is_empty() {
                ""
            } else if state.secure {
                r#"<div class="lock" role="img" aria-label="HTTPS connection" title="HTTPS connection"><div class="shackle"></div><div class="lock-body"></div></div>"#
            } else {
                r#"<div class="insecure-dot" role="img" aria-label="Not HTTPS" title="Not HTTPS"></div>"#
            };
            let (url_class, url) = if state.url.is_empty() {
                ("url url-placeholder", "Search or enter address".to_string())
            } else {
                ("url", escape_html(state.url))
            };
            let caret = if state.focused {
                r#"<div class="caret"></div>"#
            } else {
                ""
            };
            format!(
                r#"<div class="{address_class}" id="address" role="textbox" aria-label="Search or enter address" aria-multiline="false" title="Enter a URL or search">{security}<div class="{url_class}">{url}</div>{caret}</div>"#
            )
        } else {
            format!(r#"<div class="tab-title">{title}</div>"#)
        };
        html.push_str(&format!(
            r#"<div class="{class}" id="tab-{index}" role="tab" aria-selected="{active}" title="{title}" style="left:{x:.3}px;width:{width:.3}px;">
              <div class="tab-shape" aria-hidden="true" style="clip-path:path('{path}');"></div>
              <div class="tab-content"><div class="favicon" aria-hidden="true">{letter}</div>{content}<div class="tab-close" id="tab-close-{index}" role="button" aria-label="Close {title}">&#215;</div></div>
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
    use blitz_dom::{DocumentConfig, LocalName, NodeId};
    use blitz_html::HtmlDocument;
    use blitz_traits::shell::Viewport;

    fn document(chrome: &ChromeState<'_>) -> HtmlDocument {
        let mut doc = HtmlDocument::from_html(
            &render(chrome),
            DocumentConfig {
                viewport: Some(Viewport::new(
                    chrome.surface_width as u32,
                    chrome.surface_height as u32,
                    1.0,
                    chrome.theme.color_scheme(),
                )),
                ..Default::default()
            },
        );
        doc.resolve(0.0);
        doc
    }

    fn element(doc: &HtmlDocument, id: &str) -> NodeId {
        doc.get_element_by_id(id)
            .unwrap_or_else(|| panic!("missing #{id}"))
    }

    fn rect(doc: &HtmlDocument, id: &str) -> [f32; 4] {
        let node = doc.get_node(element(doc, id)).unwrap();
        let position = node.absolute_position(0.0, 0.0);
        let size = node.final_layout().size;
        [position.x, position.y, size.width, size.height]
    }

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
    fn top_right_has_loading_feedback_without_browser_wordmarks() {
        let tabs = [];
        for theme in [Theme::Light, Theme::Dark] {
            for loading in [false, true] {
                let mut chrome = state(&tabs);
                chrome.theme = theme;
                chrome.loading = loading;
                let html = render(&chrome);
                assert!(!html.contains("Walkaway"));
                assert!(!html.contains("brand-word"));
                assert!(!html.contains("menu-caption"));
                let doc = document(&chrome);
                assert!(doc.get_element_by_id("page-load-status").is_some());
                assert!(doc.get_element_by_id("walkaway-throbber").is_some());
            }
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
        assert!(!html.contains("Location:"));
        assert!(!html.contains("chrome-location"));
        assert!(!html.contains("address-popup-anchor"));
        let doc = document(&chrome);
        assert_eq!(
            doc.query_selector("#tab-0 #address").unwrap(),
            Some(element(&doc, "address"))
        );
    }

    #[test]
    fn modern_tools_have_resolved_gradients_and_named_controls() {
        for theme in [Theme::Light, Theme::Dark] {
            let mut chrome = state(&[]);
            chrome.theme = theme;
            let html = render(&chrome);
            assert!(html.contains("data-chrome-theme=\"netscape-modern\""));
            assert!(!html.contains("Navigator &#183; Modern"));
            assert!(!html.contains("Classic"));
            assert!(!html.contains("@keyframes"));
            assert!(!html.contains("animation:"));
            let doc = document(&chrome);
            for selector in [
                "#chrome-navigation",
                "#btn-back .nav-disc",
                "#btn-reload .tool-face",
                "#walkaway-throbber",
                "#walkaway-throbber .compass-disc",
            ] {
                let node = doc.query_selector(selector).unwrap().unwrap();
                let image = doc.resolved_style_value(node, "background-image");
                assert!(image.contains("gradient("), "{theme:?} {selector}: {image}");
            }
            let back_disc = doc.query_selector("#btn-back .nav-disc").unwrap().unwrap();
            assert_eq!(
                doc.resolved_style_value(back_disc, "border-top-left-radius"),
                "50%"
            );
            for (id, label) in [
                ("btn-back", "Back"),
                ("btn-forward", "Forward"),
                ("btn-reload", "Reload page"),
                ("btn-home", "Home"),
                ("btn-debugger", "JavaScript debugger"),
            ] {
                let node = doc.get_node(element(&doc, id)).unwrap();
                assert_eq!(node.attr(LocalName::from("role")), Some("button"));
                assert_eq!(node.attr(LocalName::from("aria-label")), Some(label));
                assert!(!node.text_content().trim().is_empty());
            }
        }
    }

    #[test]
    fn blank_fallback_address_is_only_inside_the_active_tab() {
        let tabs = [
            TabInfo {
                title: "Inactive page",
                active: false,
            },
            TabInfo {
                title: "Start",
                active: true,
            },
        ];
        for focused in [false, true] {
            let mut chrome = state(&tabs);
            chrome.url = "";
            chrome.focused = focused;
            // A stale secure flag must not turn an empty internal page into an
            // HTTPS claim. The host supplies the same empty value to AppKit.
            let html = render(&chrome);
            assert_eq!(html.matches("id=\"address\"").count(), 1);
            assert!(!html.contains("aria-label=\"HTTPS connection\""));
            assert!(!html.contains("aria-label=\"Not HTTPS\""));
            assert!(!html.contains("chrome-location"));
            assert!(!html.contains("address-popup-anchor"));
            assert!(!html.contains("muse://"));
            assert!(!html.contains("<script"));
            let doc = document(&chrome);
            let address_id = element(&doc, "address");
            let address = doc.get_node(address_id).unwrap();
            assert_eq!(address.text_content(), "Search or enter address");
            assert_eq!(address.attr(LocalName::from("role")), Some("textbox"));
            assert_eq!(
                doc.query_selector("#tab-1 #address").unwrap(),
                Some(address_id)
            );
            assert!(doc.query_selector("#tab-0 #address").unwrap().is_none());
            let inactive_title = doc.query_selector("#tab-0 .tab-title").unwrap().unwrap();
            assert_eq!(
                doc.get_node(inactive_title).unwrap().text_content(),
                "Inactive page"
            );
            assert_eq!(
                doc.query_selector_all("#address .caret").unwrap().len(),
                usize::from(focused)
            );
            assert_eq!(
                doc.get_node(element(&doc, "page-load-status"))
                    .unwrap()
                    .text_content(),
                "Ready"
            );
        }
    }

    #[test]
    fn fallback_address_tracks_the_active_tab_without_another_row() {
        for active in 0..2 {
            let tabs = [
                TabInfo {
                    title: "First page",
                    active: active == 0,
                },
                TabInfo {
                    title: "Second page",
                    active: active == 1,
                },
            ];
            for width in [360, 560, 600, 601, 800, 1280] {
                let mut chrome = state(&tabs);
                chrome.surface_width = f64::from(width);
                chrome.url = if active == 0 {
                    "https://first.test/?a=1&b=2"
                } else {
                    "https://second.test/"
                };
                let html = render(&chrome);
                assert_eq!(html.matches("id=\"address\"").count(), 1);
                assert_eq!(html.matches(&escape_html(chrome.url)).count(), 1);
                assert!(!html.contains("chrome-location"));
                assert!(!html.contains("address-popup-anchor"));
                let doc = document(&chrome);
                let address_id = element(&doc, "address");
                assert_eq!(
                    doc.query_selector(&format!("#tab-{active} #address"))
                        .unwrap(),
                    Some(address_id)
                );
                assert_eq!(doc.get_node(address_id).unwrap().text_content(), chrome.url);
                let inactive = 1 - active;
                let title = doc
                    .query_selector(&format!("#tab-{inactive} .tab-title"))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    doc.get_node(title).unwrap().text_content(),
                    tabs[inactive].title
                );
                assert!(
                    doc.query_selector(&format!("#tab-{active} .tab-title"))
                        .unwrap()
                        .is_none()
                );
                let strip = doc.query_selector(".tabstrip").unwrap().unwrap();
                let strip = doc.get_node(strip).unwrap();
                assert_eq!(strip.absolute_position(0.0, 0.0).y, 74.0);
                assert_eq!(strip.final_layout().size.width, width as f32);
                assert_eq!(strip.final_layout().size.height, 38.0);
                let [x, y, w, h] = rect(&doc, "address");
                let [tab_x, tab_y, tab_w, tab_h] = rect(&doc, &format!("tab-{active}"));
                assert!(w > 0.0 && h > 0.0);
                assert!(x >= tab_x && x + w <= tab_x + tab_w);
                assert!(y >= tab_y && y + h <= tab_y + tab_h);
                assert!(y >= 74.0 && y + h <= CHROME_HEIGHT as f32);
            }
        }
    }

    #[test]
    fn loading_state_switches_one_action_and_indeterminate_feedback() {
        let mut chrome = state(&[]);
        for loading in [false, true, false] {
            chrome.loading = loading;
            let html = render(&chrome);
            assert_eq!(html.matches("id=\"btn-reload\"").count(), 1);
            assert!(!html.contains("id=\"btn-stop\""));
            assert!(!html.contains("aria-valuenow"));
            let doc = document(&chrome);
            let action = doc.get_node(element(&doc, "btn-reload")).unwrap();
            assert_eq!(
                action.text_content(),
                if loading { "Stop" } else { "Reload" }
            );
            assert_eq!(
                action.attr(LocalName::from("aria-label")),
                Some(if loading {
                    "Stop loading page"
                } else {
                    "Reload page"
                })
            );
            assert_eq!(
                doc.get_node(element(&doc, "page-load-status"))
                    .unwrap()
                    .text_content(),
                if loading { "Loading…" } else { "Done" }
            );
            assert_eq!(
                doc.query_selector_all(".compass-loading").unwrap().len(),
                usize::from(loading)
            );
            assert_eq!(
                doc.query_selector_all(".stop-glyph").unwrap().len(),
                usize::from(loading)
            );
            assert_eq!(
                doc.get_element_by_id("page-load-progress").is_some(),
                loading
            );
            if loading {
                let progress = doc.get_node(element(&doc, "page-load-progress")).unwrap();
                assert_eq!(progress.attr(LocalName::from("role")), Some("progressbar"));
                assert_eq!(
                    progress.attr(LocalName::from("aria-valuetext")),
                    Some("Loading…")
                );
                assert_eq!(rect(&doc, "page-load-progress"), [0.0, 71.0, 800.0, 3.0]);
            }
        }
    }

    #[test]
    fn three_chrome_rows_reserve_only_one_native_tab_slot() {
        for width in [360, 560, 600, 601, 800, 1280] {
            for theme in [Theme::Light, Theme::Dark] {
                let mut chrome = state(&[]);
                chrome.surface_width = f64::from(width);
                chrome.theme = theme;
                chrome.loading = true;
                chrome.focused = true;
                let html = render(&chrome);
                assert_eq!(html.matches("id=\"native-tab-slot\"").count(), 1);
                assert!(!html.contains("role=\"tablist\""));
                assert!(!html.contains("id=\"btn-newtab\""));
                assert!(!html.contains("id=\"tab-"));
                assert!(!html.contains("id=\"address\""));
                assert!(!html.contains("role=\"textbox\""));
                assert!(!html.contains("chrome-location"));
                assert!(!html.contains("location-row"));
                assert!(!html.contains("address-popup-anchor"));
                assert!(!html.contains(".bar "));
                assert!(!html.contains(chrome.url));
                let doc = document(&chrome);
                for (id, y, height) in [
                    ("chrome-menubar", 0.0, 24.0),
                    ("chrome-navigation", 24.0, 50.0),
                    ("native-tab-slot", 74.0, 38.0),
                ] {
                    assert_eq!(
                        rect(&doc, id),
                        [0.0, y, width as f32, height],
                        "{width} {theme:?} #{id}"
                    );
                }
                assert_eq!(
                    rect(&doc, "page-load-progress"),
                    [0.0, 71.0, width as f32, 3.0],
                    "loading stripe at toolbar bottom at {width}px {theme:?}"
                );
                assert!(doc.get_element_by_id("address").is_none());
                assert!(doc.query_selector(".tab-address").unwrap().is_none());
                let slot = doc.get_node(element(&doc, "native-tab-slot")).unwrap();
                assert!(slot.children.is_empty(), "no imitation below native tabs");
                for id in [
                    "btn-back",
                    "btn-forward",
                    "btn-reload",
                    "btn-home",
                    "btn-debugger",
                    "walkaway-throbber",
                    "page-load-status",
                ] {
                    let [x, y, w, h] = rect(&doc, id);
                    assert!(
                        w > 0.0 && h > 0.0 && x >= 0.0 && x + w <= width as f32,
                        "{width} {theme:?} #{id}: {x},{y} {w}x{h}"
                    );
                    assert!(
                        y >= 24.0 && y + h <= 74.0,
                        "#{id} must stay in navigation row"
                    );
                }
                for id in [
                    "btn-back",
                    "btn-forward",
                    "btn-reload",
                    "btn-home",
                    "btn-debugger",
                ] {
                    let [x, y, w, h] = rect(&doc, id);
                    let hit = doc
                        .hit(x + w / 2.0, y + h / 2.0)
                        .expect("visible navigation control");
                    assert_eq!(
                        crate::dom_ffi::bubble_target(&doc, hit.node_id),
                        element(&doc, id),
                        "{width} {theme:?} #{id}"
                    );
                }
            }
        }
        assert_eq!(CHROME_HEIGHT, 112.0);
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
        assert!(html.contains(&format!("top: {CHROME_HEIGHT}px;")));
        assert!(html.contains(&format!("top:{:.1}px;", CHROME_HEIGHT + 20.0)));
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
