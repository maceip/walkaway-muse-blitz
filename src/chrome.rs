//! The browser chrome, rendered by the same HTML/CSS engine as the page.
//!
//! Structure is taken from the Meta Muse browser, in both its mobile shell and
//! its docked desktop panel: tab strip above, navigation row below, filled
//! address pill, neutral palette, padlock for https, hamburger menu.
//!
//! Palettes are sampled from screenshots rather than guessed. Light: strip
//! `#E4E4E4`, active tab `#FAFAFA`, nav `#FAFAFA`, pill `#F3F3F3`, ink
//! `#1C1C1E`. Dark: strip `#1C1C1E`, nav `#2C2C2E`, pill `#3A3A3C`, ink
//! `#F2F2F7`, hairline `#3A3A3C`.
//!
//! One deliberate difference from the reference: the screenshots show the
//! active tab and the toolbar tinted to the *page's* colour (light, because the
//! page in that shot was a white JSON view). That works when the page is light,
//! but produces an unreadable toolbar over a dark page, so the chrome here uses
//! a fixed palette per theme and lets the page supply its own colours.
//!
//! Icons are drawn from borders and bars, not glyphs. A probe of the fallback
//! chain (examples/glyphs.rs) showed `←`/`→` resolve but every refresh glyph
//! (U+21BB, U+27F3, U+21BA, U+27F2) renders as tofu, so back/forward are
//! rotated border chevrons, reload is a gapped ring plus a border triangle, and
//! the menu is three bars.

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
    tab_active: &'static str,
    nav: &'static str,
    hairline: &'static str,
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
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            strip: "#E4E4E4",
            tab_active: "#FAFAFA",
            nav: "#FAFAFA",
            hairline: "#D8D8D8",
            pill: "#F3F3F3",
            pill_focus: "#FFFFFF",
            ink: "#1C1C1E",
            muted: "#BFC1C6",
            hover: "#EDEDED",
            menu_bg: "#FFFFFF",
            separator: "#E4E4E4",
            favicon_bg: "#D4D6DB",
            favicon_ink: "#4A4D52",
            accent: "#1B74E4",
        },
        Theme::Dark => Palette {
            strip: "#1C1C1E",
            tab_active: "#2C2C2E",
            nav: "#2C2C2E",
            hairline: "#3A3A3C",
            pill: "#3A3A3C",
            pill_focus: "#1C1C1E",
            ink: "#F2F2F7",
            muted: "#636366",
            hover: "#3A3A3C",
            menu_bg: "#2C2C2E",
            separator: "#3A3A3C",
            favicon_bg: "#48484A",
            favicon_ink: "#C7C7CC",
            accent: "#0A84FF",
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
    /// Height of the whole surface in CSS pixels. The chrome body spans it (and
    /// stays transparent below the nav row) so the menu is not clipped.
    pub surface_height: f64,
    pub theme: Theme,
    pub supervision: Option<SupervisionInfo<'a>>,
}

/// Tab strip (38) + navigation row (50).
pub const CHROME_HEIGHT: f64 = 88.0;

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
    font-family: "Optimistic VF", Montserrat, "Helvetica Neue", Helvetica, Arial, sans-serif;
    position: relative;
  }}

  /* ---- tab strip ---- */
  .tabstrip {{
    height: 38px;
    background-color: {strip};
    display: flex;
    flex-direction: row;
    align-items: flex-end;
    padding: 0 10px;
    gap: 6px;
  }}
  .tab {{
    height: 30px;
    max-width: 240px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border-radius: 10px 10px 0 0;
  }}
  .tab-active {{ background-color: {tab_active}; }}
  .favicon {{
    width: 15px; height: 15px;
    border-radius: 4px;
    background-color: {favicon_bg};
    color: {favicon_ink};
    font-size: 9px;
    text-align: center;
  }}
  .tab-title {{ font-size: 13px; color: {ink}; }}
  .tab-close {{ font-size: 15px; color: {muted}; padding: 0 2px; }}
  .newtab {{
    width: 28px; height: 28px;
    font-size: 18px;
    color: {ink};
    text-align: center;
    margin-bottom: 2px;
  }}

  /* ---- navigation row ---- */
  .nav {{
    height: 50px;
    background-color: {nav};
    border-bottom: 1px solid {hairline};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 10px;
    gap: 6px;
  }}
  .btn {{
    width: 32px; height: 32px;
    border-radius: 16px;
    display: flex; align-items: center; justify-content: center;
  }}
  .btn:hover {{ background-color: {hover}; }}

  .chev {{
    width: 9px; height: 9px;
    border-left: 2px solid {ink};
    border-bottom: 2px solid {ink};
  }}
  .chev-l {{ transform: rotate(45deg); margin-left: 3px; }}
  .chev-r {{ transform: rotate(225deg); margin-right: 3px; }}
  .btn-off .chev {{ border-left-color: {muted}; border-bottom-color: {muted}; }}
  .btn-off:hover {{ background-color: {nav}; }}

  .reload {{ display: flex; flex-direction: row; align-items: flex-start; }}
  .ring {{
    width: 15px; height: 15px;
    border-radius: 8px;
    border: 2px solid {ink};
    border-top-color: {nav};
  }}
  .btn:hover .ring {{ border-top-color: {hover}; }}
  .arrow {{
    width: 0; height: 0;
    border-top: 4px solid transparent;
    border-bottom: 4px solid transparent;
    border-left: 6px solid {ink};
    margin-top: -6px;
    margin-left: -5px;
  }}

  .bar {{
    flex: 1;
    height: 34px;
    background-color: {pill};
    border: 1px solid {pill};
    border-radius: 17px;
    display: flex; flex-direction: row; align-items: center;
    padding: 0 12px;
    gap: 8px;
  }}
  .bar-focused {{ background-color: {pill_focus}; border: 1px solid {accent}; }}
  .url {{ font-size: 14px; color: {ink}; }}
  .caret {{ width: 2px; height: 16px; background-color: {accent}; }}

  /* padlock, drawn from a shackle + body */
  .lock {{ display: flex; flex-direction: column; align-items: center; }}
  .shackle {{
    width: 6px; height: 5px;
    border: 1.5px solid {favicon_ink};
    border-bottom-width: 0;
    border-radius: 3px 3px 0 0;
  }}
  .lock-body {{ width: 9px; height: 6px; border-radius: 2px; background-color: {favicon_ink}; }}
  .insecure-dot {{ width: 7px; height: 7px; border-radius: 4px; background-color: {muted}; }}

  /* menu */
  .menu-icon {{ display: flex; flex-direction: column; gap: 3px; }}
  .bar-line {{ width: 15px; height: 2px; background-color: {ink}; }}

  .menu {{
    position: absolute;
    right: 10px;
    top: 92px;
    width: 190px;
    background-color: {menu_bg};
    border: 1px solid {hairline};
    border-radius: 12px;
    padding: 6px;
  }}
  .menu-item {{ font-size: 13px; color: {ink}; padding: 8px 10px; border-radius: 8px; }}
  .menu-item:hover {{ background-color: {hover}; }}
  .menu-sep {{ height: 1px; background-color: {separator}; margin: 5px 4px; }}

  /* supervision bar: overlays the top of the page on the session tab */
  .supbar {{
    position: absolute;
    left: 0; right: 0; top: 88px;
    height: 46px;
    background-color: {menu_bg};
    border-bottom: 1px solid {hairline};
    display: flex; flex-direction: row; align-items: center;
    padding: 0 12px;
    gap: 8px;
  }}
  .sup-title {{ font-size: 13px; color: {ink}; }}
  .sup-state {{ font-size: 11px; color: {accent}; }}
  .sup-detail {{ font-size: 12px; color: {favicon_ink}; flex: 1; }}
  .sup-btn {{
    font-size: 12px; color: {ink};
    background-color: {pill};
    border-radius: 13px;
    padding: 5px 12px;
  }}
  .sup-btn:hover {{ background-color: {hover}; }}
  .sup-approve {{ background-color: {accent}; color: #FFFFFF; }}
  .sup-stop {{ color: #D70015; }}

  .progress {{
    position: absolute;
    left: 0; right: 0; bottom: 0;
    height: 2px;
    background-color: {accent};
  }}
</style>
</head>
<body>
  <div class="tabstrip">
    {tabs}
    <div class="newtab" id="btn-newtab">+</div>
  </div>

  <div class="nav">
    <div class="{back_class}" id="btn-back"><div class="chev chev-l"></div></div>
    <div class="{forward_class}" id="btn-forward"><div class="chev chev-r"></div></div>
    <div class="btn" id="btn-reload"><div class="reload"><div class="ring"></div><div class="arrow"></div></div></div>
    <div class="{bar_class}" id="address">
      {security}
      <div class="url">{url}</div>
      {caret}
    </div>
    <div class="btn" id="btn-menu">
      <div class="menu-icon"><div class="bar-line"></div><div class="bar-line"></div><div class="bar-line"></div></div>
    </div>
  </div>
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
        menu = menu,
        supbar = supbar,
        progress = progress,
    )
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
            _ => out.push(ch),
        }
    }
    out
}
