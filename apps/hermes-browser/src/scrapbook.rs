//! IE5 Mac Internet Scrapbook: persistent offline DOM + style + image (`data:` URI)
//! archival system.
//!
//! Captures the active tab's live, post-JS-mutation Blitz `BaseDocument` into a
//! self-contained HTML archive with:
//! - Original URL, page title, and UTC capture timestamp metadata banner/attributes
//! - Serialized live DOM tree (preserving dynamic nodes, text, and attribute mutations)
//! - Resolved inline styles (`style` attribute enriched with Stylo computed properties)
//! - Decoded `<img>` raster buffers encoded to PNG and inlined as `data:image/png;base64,...`
//! - Resolved absolute `<a href="...">` URLs so hyperlinks remain navigable offline
//! - Persistent JSON disk storage (`HERMES_SCRAPBOOK_PATH` or `~/.local/share/hermes-blitz-browser/scrapbook.json`)

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use blitz_dom::BaseDocument;
use blitz_dom::node::{ImageData, NodeData, SpecialElementData};
use blitz_traits::node_id::NodeId;
use markup5ever::LocalName;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::control::b64_encode;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScrapbookEntry {
    pub id: String,
    pub title: String,
    pub url: String,
    pub timestamp: String,
    pub is_folder: bool,
    pub html_snapshot: String,
}

pub fn scrapbook_path() -> PathBuf {
    if let Ok(custom) = std::env::var("HERMES_SCRAPBOOK_PATH") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/share/hermes-blitz-browser/scrapbook.json")
}

pub fn load_entries() -> Vec<ScrapbookEntry> {
    let path = scrapbook_path();
    let Ok(bytes) = fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_entries(entries: &[ScrapbookEntry]) -> Result<(), String> {
    let path = scrapbook_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create scrapbook dir: {e}"))?;
    }
    let json =
        serde_json::to_vec_pretty(entries).map_err(|e| format!("serialize scrapbook: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("write scrapbook {}: {e}", path.display()))
}

pub fn now_timestamp_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format as YYYY-MM-DD HH:MM:SS UTC using civil-from-days algorithm.
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02} UTC")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Serialize the live `BaseDocument` into a self-contained offline HTML snapshot.
pub fn capture_document_snapshot(
    doc: &BaseDocument,
    url_str: &str,
    title: &str,
    timestamp: &str,
) -> String {
    let base_url = Url::parse(url_str).ok();
    let mut out = String::from("<!DOCTYPE html>\n");
    let root_id = doc
        .try_root_element()
        .map(|n| n.id)
        .unwrap_or_else(|| doc.root_node().id);
    serialize_snapshot_node(doc, root_id, &base_url, url_str, title, timestamp, &mut out);
    out
}

fn serialize_snapshot_node(
    doc: &BaseDocument,
    node_id: NodeId,
    base_url: &Option<Url>,
    url_str: &str,
    title: &str,
    timestamp: &str,
    out: &mut String,
) {
    let Some(node) = doc.get_node(node_id) else {
        return;
    };
    match &node.data {
        NodeData::Document(_) | NodeData::AnonymousBlock(_) => {
            for &child in &node.children {
                serialize_snapshot_node(doc, child, base_url, url_str, title, timestamp, out);
            }
        }
        NodeData::Text(t) => {
            escape_text(&t.content, out);
        }
        NodeData::Comment { .. } => {}
        NodeData::Element(el) => {
            let tag: &str = el.name.local.as_ref();
            // Do not serialize <script> tags into offline snapshots so that already-mutated
            // DOM state is preserved as-is without re-executing initial page scripts.
            if tag.eq_ignore_ascii_case("script") {
                return;
            }
            out.push('<');
            out.push_str(tag);

            if tag.eq_ignore_ascii_case("html") {
                out.push_str(&format!(
                    r#" data-scrapbook-url="{}" data-scrapbook-title="{}" data-scrapbook-timestamp="{}""#,
                    escape_attr(url_str),
                    escape_attr(title),
                    escape_attr(timestamp),
                ));
            }

            let mut existing_style = String::new();
            let mut wrote_src = false;

            // Check if this is an <img> with decoded raster pixel data in Blitz.
            let raster_data_uri = if tag.eq_ignore_ascii_case("img") {
                encode_img_raster_as_data_uri(&el.special_data)
            } else {
                None
            };

            for attr in el.attrs.iter() {
                let name: &str = attr.name.local.as_ref();
                if name.eq_ignore_ascii_case("data-scrapbook-url")
                    || name.eq_ignore_ascii_case("data-scrapbook-title")
                    || name.eq_ignore_ascii_case("data-scrapbook-timestamp")
                {
                    continue;
                }
                if name.eq_ignore_ascii_case("style") {
                    existing_style = attr.value.clone();
                    continue;
                }
                if tag.eq_ignore_ascii_case("img") && name.eq_ignore_ascii_case("src") {
                    wrote_src = true;
                    if let Some(ref data_uri) = raster_data_uri {
                        out.push_str(&format!(r#" src="{}""#, data_uri));
                        continue;
                    }
                    let resolved = resolve_attr_url(base_url, &attr.value);
                    out.push_str(&format!(r#" src="{}""#, escape_attr(&resolved)));
                    continue;
                }
                if tag.eq_ignore_ascii_case("a") && name.eq_ignore_ascii_case("href") {
                    let resolved = resolve_attr_url(base_url, &attr.value);
                    out.push_str(&format!(r#" href="{}""#, escape_attr(&resolved)));
                    continue;
                }
                out.push(' ');
                out.push_str(name);
                out.push_str("=\"");
                out.push_str(&escape_attr(&attr.value));
                out.push('"');
            }

            if tag.eq_ignore_ascii_case("img") && !wrote_src {
                if let Some(ref data_uri) = raster_data_uri {
                    out.push_str(&format!(r#" src="{}""#, data_uri));
                }
            }

            // Preserve form control live state on <input>.
            if tag.eq_ignore_ascii_case("input") {
                if let Some(text_data) = el.text_input_data() {
                    let live_val = text_data.editor.text().to_string();
                    if !live_val.is_empty() && el.attr(LocalName::from("value")).is_none() {
                        out.push_str(&format!(r#" value="{}""#, escape_attr(&live_val)));
                    }
                }
            }

            // Capture key resolved inline styles so visual appearance survives offline.
            let inline_style = build_inline_style(doc, node_id, tag, &existing_style);
            if !inline_style.is_empty() {
                out.push_str(&format!(r#" style="{}""#, escape_attr(&inline_style)));
            }

            out.push('>');

            let is_void = matches!(
                tag.to_ascii_lowercase().as_str(),
                "area"
                    | "base"
                    | "br"
                    | "col"
                    | "embed"
                    | "hr"
                    | "img"
                    | "input"
                    | "link"
                    | "meta"
                    | "param"
                    | "source"
                    | "track"
                    | "wbr"
            );
            if !is_void {
                for &child in &node.children {
                    serialize_snapshot_node(doc, child, base_url, url_str, title, timestamp, out);
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
    }
}

fn build_inline_style(
    doc: &BaseDocument,
    node_id: NodeId,
    tag: &str,
    existing_style: &str,
) -> String {
    let mut parts = Vec::new();
    let trimmed = existing_style.trim().trim_end_matches(';');
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }
    if matches!(
        tag.to_ascii_lowercase().as_str(),
        "html" | "head" | "title" | "meta" | "link" | "style"
    ) {
        return parts.join("; ");
    }
    for prop in ["background-color", "color"] {
        if existing_style.to_ascii_lowercase().contains(prop) {
            continue;
        }
        let val = doc.resolved_style_value(node_id, prop);
        let v = val.trim();
        if !v.is_empty()
            && v != "rgba(0, 0, 0, 0)"
            && v != "transparent"
            && v != "initial"
            && v != "inherit"
        {
            parts.push(format!("{prop}: {v}"));
        }
    }
    parts.join("; ")
}

fn encode_img_raster_as_data_uri(special: &SpecialElementData) -> Option<String> {
    let SpecialElementData::Image(img_box) = special else {
        return None;
    };
    let ImageData::Raster(raster) = img_box.as_ref() else {
        return None;
    };
    if raster.width == 0 || raster.height == 0 {
        return None;
    }
    let expected_len = (raster.width as usize)
        .checked_mul(raster.height as usize)?
        .checked_mul(4)?;
    let raw_bytes = raster.data.data();
    if raw_bytes.len() < expected_len {
        return None;
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, raster.width, raster.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&raw_bytes[..expected_len]).ok()?;
    }
    Some(format!("data:image/png;base64,{}", b64_encode(&png_bytes)))
}

fn resolve_attr_url(base_url: &Option<Url>, raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("data:")
        || trimmed.starts_with("javascript:")
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with('#')
    {
        return trimmed.to_string();
    }
    if let Some(base) = base_url {
        if let Ok(joined) = base.join(trimmed) {
            return joined.to_string();
        }
    }
    trimmed.to_string()
}

fn escape_text(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

fn escape_attr(s: &str) -> String {
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
