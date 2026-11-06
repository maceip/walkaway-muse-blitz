//! The browser chrome, rendered by the same HTML/CSS engine as the page.
//!
//! Structure combines the clean top tab strip + navigation bar with the IE5 Mac
//! Collapsible Explorer Bar:
//! - Left vertical Explorer Bar rail (`28px` wide) with `#btn-collapse-toolbar`
//!   and vertical tabs (`#exp-favorites`, `#exp-history`, `#exp-search`,
//!   `#exp-scrapbook`, `#exp-pageholder`).
//! - Collapsible Toolbar Mode (`toolbar_collapsed`): hides the top tab strip and
//!   address bar so the web viewport expands to `y = 0`, placing compact
//!   `#btn-back`, `#btn-forward`, `#btn-stop`, and `#btn-reload` controls at the
//!   top of the vertical Explorer Bar rail.
//! - Explorer Bar Drawer (`260px` wide) supporting Page Holder (`#ph-add`,
//!   `#ph-links`, Rendered Page View & Links List View), Internet Scrapbook
//!   (`#sb-add`, `#sb-delete`, `#sb-organize`, `#sb-entry-N`), Favorites,
//!   History, and Search.

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
    rail_bg: &'static str,
    drawer_bg: &'static str,
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
            rail_bg: "#EAECEF",
            drawer_bg: "#F7F8FA",
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
            rail_bg: "#242426",
            drawer_bg: "#1F1F21",
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

/// Tab strip (38) + navigation row (50).
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

    let security = if state.secure {
        r#"<div class="lock"><div class="shackle"></div><div class="lock-body"></div></div>"#
    } else {
        r#"<div class="insecure-dot"></div>"#
    };

    let top_bars = if state.toolbar_collapsed {
        String::new()
    } else {
        format!(
            r#"<div class="tabstrip">
                 {tabs}
                 <div class="newtab" id="btn-newtab">+</div>
               </div>
               <div class="nav">
                 <div class="{back_class}" id="btn-back"><div class="chev chev-l"></div></div>
                 <div class="{forward_class}" id="btn-forward"><div class="chev chev-r"></div></div>
                 <div class="{stop_class}" id="btn-stop"><div class="stop-icon">&#215;</div></div>
                 <div class="btn" id="btn-reload"><div class="reload"><div class="ring"></div><div class="arrow"></div></div></div>
                 <div class="{bar_class}" id="address">
                   {security}
                   <div class="url">{url}</div>
                   {caret}
                 </div>
                 <div class="btn" id="btn-menu">
                   <div class="menu-icon"><div class="bar-line"></div><div class="bar-line"></div><div class="bar-line"></div></div>
                 </div>
               </div>"#,
            tabs = tabs,
            back_class = back_class,
            forward_class = forward_class,
            stop_class = stop_class,
            bar_class = bar_class,
            security = security,
            url = escape_html(state.url),
            caret = caret,
        )
    };

    // Compact navigation buttons rendered inside the left Explorer Bar rail when the top toolbar is collapsed.
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
            r#"<div class="{b_cls}" id="btn-back">&lt;</div>
               <div class="{f_cls}" id="btn-forward">&gt;</div>
               <div class="{s_cls}" id="btn-stop">&#215;</div>
               <div class="rail-icon" id="btn-reload">&#8635;</div>"#,
        )
    } else {
        String::new()
    };

    let collapse_glyph = if state.toolbar_collapsed {
        "&#9662;"
    } else {
        "&#9652;"
    };
    let fav_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Favorites));
    let hist_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::History));
    let srch_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Search));
    let scrp_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::Scrapbook));
    let ph_cls = exp_tab_class(state.explorer_tab == Some(ExplorerTab::PageHolder));

    let explorer_rail = format!(
        r#"<div class="exp-rail" id="exp-rail" style="top:{top_chrome_h:.1}px;">
             <div class="rail-collapse" id="btn-collapse-toolbar">{collapse_glyph}</div>
             {rail_compact_nav}
             <div class="{fav_cls}" id="exp-favorites">Fav</div>
             <div class="{hist_cls}" id="exp-history">Hist</div>
             <div class="{srch_cls}" id="exp-search">Srch</div>
             <div class="{scrp_cls}" id="exp-scrapbook">Scrp</div>
             <div class="{ph_cls}" id="exp-pageholder">Hold</div>
           </div>"#,
    );

    let explorer_drawer = match state.explorer_tab {
        None => String::new(),
        Some(ExplorerTab::PageHolder) => {
            let links_btn_cls = if state.page_holder.links_mode {
                "drawer-btn drawer-btn-active"
            } else {
                "drawer-btn"
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
                            r#"<div class="drawer-item" id="ph-link-{i}">
                                 <div class="drawer-item-title">{label}</div>
                                 <div class="drawer-item-sub">{url}</div>
                               </div>"#,
                            i = i,
                            label = escape_html(link.label),
                            url = escape_html(link.url),
                        ));
                    }
                }
                format!(r#"<div class="drawer-body" id="ph-links-list">{rows}</div>"#)
            } else if state.page_holder.has_doc {
                // Transparent body so the live `page_holder_doc` painted in its clipped Vello layer underneath shows through.
                r#"<div class="drawer-body-transparent" id="ph-canvas"></div>"#.to_string()
            } else {
                r#"<div class="drawer-body"><div class="drawer-empty" id="ph-empty">Click "Add" to hold the current page for side-by-side browsing and link retargeting.</div></div>"#.to_string()
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
                       <div class="drawer-title">Page Holder</div>
                       <div class="drawer-btn" id="ph-add">Add</div>
                       <div class="{links_btn_cls}" id="ph-links">Links</div>
                     </div>
                     {body_html}
                   </div>"#,
            )
        }
        Some(ExplorerTab::Scrapbook) => {
            let mut rows = String::new();
            if state.scrapbook_items.is_empty() {
                rows.push_str(
                    r#"<div class="drawer-empty" id="sb-empty">Scrapbook is empty. Click "Add" to archive an offline snapshot of the active page.</div>"#,
                );
            } else {
                for (i, item) in state.scrapbook_items.iter().enumerate() {
                    let cls = match (item.is_folder, item.selected) {
                        (true, true) => "drawer-item drawer-folder drawer-item-selected",
                        (true, false) => "drawer-item drawer-folder",
                        (false, true) => "drawer-item drawer-item-selected",
                        (false, false) => "drawer-item",
                    };
                    if item.is_folder {
                        rows.push_str(&format!(
                            r#"<div class="{cls}" id="sb-entry-{i}" data-sb-id="{id}">
                                 <div class="drawer-item-title">&#128193; {title}</div>
                                 <div class="drawer-item-sub">{ts}</div>
                               </div>"#,
                            cls = cls,
                            i = i,
                            id = escape_html(item.id),
                            title = escape_html(item.title),
                            ts = escape_html(item.timestamp),
                        ));
                    } else {
                        rows.push_str(&format!(
                            r#"<div class="{cls}" id="sb-entry-{i}" data-sb-id="{id}">
                                 <div class="drawer-item-title">{title}</div>
                                 <div class="drawer-item-sub">{url}</div>
                                 <div class="drawer-item-meta">{ts}</div>
                               </div>"#,
                            cls = cls,
                            i = i,
                            id = escape_html(item.id),
                            title = escape_html(item.title),
                            url = escape_html(item.url),
                            ts = escape_html(item.timestamp),
                        ));
                    }
                }
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="sb-header">
                       <div class="drawer-title">Scrapbook</div>
                       <div class="drawer-btn" id="sb-add">Add</div>
                       <div class="drawer-btn" id="sb-organize">Organize</div>
                       <div class="drawer-btn drawer-btn-danger" id="sb-delete">Delete</div>
                     </div>
                     <div class="drawer-body" id="sb-list">{rows}</div>
                   </div>"#,
            )
        }
        Some(ExplorerTab::Favorites) => {
            let mut rows = String::new();
            for (i, fav) in state.favorites.iter().enumerate() {
                rows.push_str(&format!(
                    r#"<div class="drawer-item" id="fav-item-{i}">
                         <div class="drawer-item-title">{title}</div>
                         <div class="drawer-item-sub">{url}</div>
                       </div>"#,
                    i = i,
                    title = escape_html(fav.title),
                    url = escape_html(fav.url),
                ));
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="fav-header">
                       <div class="drawer-title">Favorites</div>
                       <div class="drawer-btn" id="fav-add">Add</div>
                     </div>
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
                        r#"<div class="drawer-item" id="hist-item-{i}">
                             <div class="drawer-item-title">{title}</div>
                             <div class="drawer-item-sub">{url}</div>
                           </div>"#,
                        i = i,
                        title = escape_html(entry.title),
                        url = escape_html(entry.url),
                    ));
                }
            }
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="hist-header">
                       <div class="drawer-title">History</div>
                     </div>
                     <div class="drawer-body" id="hist-list">{rows}</div>
                   </div>"#,
            )
        }
        Some(ExplorerTab::Search) => {
            format!(
                r#"<div class="exp-drawer" id="exp-drawer" style="top:{top_chrome_h:.1}px;">
                     <div class="drawer-header" id="search-header">
                       <div class="drawer-title">Search Assistant</div>
                       <div class="drawer-btn" id="exp-search-focus">Address</div>
                     </div>
                     <div class="drawer-body" id="search-list">
                       <div class="drawer-item" id="search-item-0">
                         <div class="drawer-item-title">DuckDuckGo Lite</div>
                         <div class="drawer-item-sub">https://lite.duckduckgo.com/lite/</div>
                       </div>
                       <div class="drawer-item" id="search-item-1">
                         <div class="drawer-item-title">Wikipedia</div>
                         <div class="drawer-item-sub">https://en.wikipedia.org/</div>
                       </div>
                       <div class="drawer-item" id="search-item-2">
                         <div class="drawer-item-title">AOL Portal</div>
                         <div class="drawer-item-sub">https://aol.com/</div>
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
  .stop-icon {{ font-size: 16px; color: {ink}; font-weight: 600; }}
  .btn-off .stop-icon {{ color: {muted}; }}

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

  /* ---- IE5 Mac Left Vertical Explorer Bar Rail ---- */
  .exp-rail {{
    position: absolute;
    left: 0;
    bottom: 0;
    width: 28px;
    background-color: {rail_bg};
    border-right: 1px solid {hairline};
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 4px 0;
    gap: 4px;
  }}
  .rail-collapse {{
    width: 22px;
    height: 20px;
    border-radius: 4px;
    background-color: {pill};
    border: 1px solid {hairline};
    font-size: 11px;
    color: {ink};
    display: flex;
    align-items: center;
    justify-content: center;
  }}
  .rail-icon {{
    width: 22px;
    height: 22px;
    border-radius: 4px;
    background-color: {tab_active};
    border: 1px solid {hairline};
    font-size: 11px;
    font-weight: 700;
    color: {ink};
    display: flex;
    align-items: center;
    justify-content: center;
  }}
  .rail-icon-off {{
    color: {muted};
    background-color: {rail_bg};
  }}
  .exp-tab {{
    width: 24px;
    height: 46px;
    border-radius: 5px;
    background-color: {pill};
    border: 1px solid {hairline};
    font-size: 9px;
    font-weight: 600;
    color: {ink};
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
  }}
  .exp-tab-active {{
    background-color: {accent};
    border-color: {accent};
    color: #FFFFFF;
  }}

  /* ---- IE5 Mac Explorer Bar Drawer ---- */
  .exp-drawer {{
    position: absolute;
    left: 28px;
    bottom: 0;
    width: 260px;
    background-color: {drawer_bg};
    border-right: 1px solid {hairline};
    display: flex;
    flex-direction: column;
  }}
  .exp-drawer-transparent {{
    background-color: transparent;
  }}
  .drawer-header {{
    height: 38px;
    background-color: {nav};
    border-bottom: 1px solid {hairline};
    display: flex;
    flex-direction: row;
    align-items: center;
    padding: 0 8px;
    gap: 5px;
  }}
  .drawer-title {{
    font-size: 12px;
    font-weight: 700;
    color: {ink};
    flex: 1;
  }}
  .drawer-btn {{
    font-size: 11px;
    font-weight: 600;
    color: {ink};
    background-color: {pill};
    border: 1px solid {hairline};
    border-radius: 5px;
    padding: 3px 7px;
  }}
  .drawer-btn-active {{
    background-color: {accent};
    border-color: {accent};
    color: #FFFFFF;
  }}
  .drawer-btn-danger {{
    color: #D70015;
  }}
  .drawer-body {{
    flex: 1;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    overflow: hidden;
  }}
  .drawer-body-transparent {{
    flex: 1;
    background-color: transparent;
  }}
  .drawer-empty {{
    font-size: 12px;
    color: {favicon_ink};
    padding: 12px 8px;
    line-height: 1.4;
  }}
  .drawer-item {{
    background-color: {menu_bg};
    border: 1px solid {hairline};
    border-radius: 6px;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }}
  .drawer-folder {{
    background-color: {strip};
  }}
  .drawer-item-selected {{
    border: 1.5px solid {accent};
  }}
  .drawer-item-title {{
    font-size: 12px;
    font-weight: 600;
    color: {ink};
  }}
  .drawer-item-sub {{
    font-size: 10px;
    color: {accent};
  }}
  .drawer-item-meta {{
    font-size: 9px;
    color: {favicon_ink};
  }}

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

  /* supervision bar: docked in the right side of the top tab strip so it never covers page content */
  .supbar {{
    position: absolute;
    right: 10px; top: 3px;
    height: 32px;
    background-color: {menu_bg};
    border: 1px solid {hairline};
    border-radius: 8px;
    display: flex; flex-direction: row; align-items: center;
    padding: 0 10px;
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
        rail_bg = p.rail_bg,
        drawer_bg = p.drawer_bg,
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
