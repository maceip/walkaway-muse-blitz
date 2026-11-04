//! Native page observation for the supervised browser control API.
//!
//! This module reads Blitz's resolved DOM/layout directly. It does not inject
//! JavaScript, expose raw Blitz node IDs over the wire, or execute actions.
//! Snapshot-local `el_N` IDs are the only element handles in serialized data.

use std::collections::HashMap;

use blitz_dom::node::{Node, NodeData};
use blitz_html::HtmlDocument;
use blitz_traits::node_id::NodeId;
use markup5ever::local_name;
use serde::Serialize;
use sha2::{Digest, Sha256};
use url::Url;

#[derive(Clone, Debug)]
pub struct SnapshotMetadata {
    pub snapshot_id: u64,
    pub tab_id: u64,
    /// Instance of the page hosted in the tab. Changes when the document's
    /// page object is replaced, even if the tab and session survive.
    pub page_id: u64,
    pub document_generation: u64,
    pub url: String,
    pub title: String,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub scale: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageSnapshot {
    pub snapshot_id: u64,
    pub tab_id: u64,
    pub page_id: u64,
    pub document_generation: u64,
    pub fingerprint: String,
    pub url: String,
    pub title: String,
    pub viewport: ViewportSnapshot,
    pub scroll: ScrollSnapshot,
    pub text: String,
    pub elements: Vec<ElementSnapshot>,
    pub focused_element_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_recipe_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ViewportSnapshot {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ScrollSnapshot {
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ElementSnapshot {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    /// Stable reference for this node within its page instance, in the
    /// WebDriver sense: the same node keeps the same reference across
    /// snapshots until the document is replaced or the node is removed.
    pub node_ref: String,
    #[serde(skip)]
    pub node_id: NodeId,
    pub kind: String,
    pub role: String,
    pub tag: String,
    pub label: String,
    pub bounds: Bounds,
    pub enabled: bool,
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub value_state: ValueState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueState {
    Empty,
    Present,
    Sensitive,
    Unavailable,
}

fn attr<'a>(node: &'a Node, name: markup5ever::LocalName) -> Option<&'a str> {
    node.data.attr(name)
}

fn normalized(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn is_sensitive(tag: &str, input_type: &str, node: &Node, label: &str) -> bool {
    if input_type == "password" {
        return true;
    }
    let autocomplete = attr(node, local_name!("autocomplete")).unwrap_or_default();
    if autocomplete == "one-time-code"
        || autocomplete.starts_with("cc-")
        || autocomplete.contains("password")
    {
        return true;
    }
    if tag != "input" && tag != "textarea" && tag != "select" {
        return false;
    }
    let hint = format!(
        "{} {} {}",
        attr(node, local_name!("name")).unwrap_or_default(),
        attr(node, local_name!("id")).unwrap_or_default(),
        label
    )
    .to_ascii_lowercase();
    hint.contains("password")
        || hint.contains("passcode")
        || hint.contains("one-time code")
        || hint.contains("credit card")
        || hint.contains("card number")
        || hint.contains("cvv")
}

fn label_for(
    doc: &HtmlDocument,
    node: &Node,
    tag: &str,
    labels_by_for: &HashMap<String, String>,
) -> String {
    if let Some(value) = attr(node, local_name!("aria-label")) {
        return normalized(value, 160);
    }
    if let Some(id) = attr(node, local_name!("id")) {
        if let Some(label) = labels_by_for.get(id) {
            return label.clone();
        }
    }

    let text = normalized(&node.text_content(), 160);
    if !text.is_empty() {
        return text;
    }
    for name in [
        local_name!("placeholder"),
        local_name!("title"),
        local_name!("alt"),
    ] {
        if let Some(value) = attr(node, name) {
            let value = normalized(value, 160);
            if !value.is_empty() {
                return value;
            }
        }
    }
    if tag == "input" {
        if let Some(value) = attr(node, local_name!("value")) {
            return normalized(value, 160);
        }
    }
    if tag == "a" {
        if let Some(href) = attr(node, local_name!("href")) {
            return normalized(href, 160);
        }
    }
    let _ = doc;
    tag.to_string()
}

fn classify(tag: &str, input_type: &str, node: &Node) -> Option<(&'static str, String)> {
    let role_attr = attr(node, local_name!("role"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let interactive_role = matches!(
        role_attr.as_str(),
        "button" | "link" | "checkbox" | "radio" | "tab" | "menuitem" | "option"
    );

    match tag {
        "a" if attr(node, local_name!("href")).is_some() => Some(("click", "link".into())),
        "button" => Some(("click", "button".into())),
        "textarea" => Some(("type", "textbox".into())),
        "select" => Some(("select", "combobox".into())),
        "option" => Some(("select", "option".into())),
        "label" if attr(node, local_name!("for")).is_some() => Some(("click", "generic".into())),
        "input" => match input_type {
            "hidden" => None,
            "checkbox" => Some(("click", "checkbox".into())),
            "radio" => Some(("click", "radio".into())),
            "submit" | "button" | "reset" | "image" | "file" => Some(("click", "button".into())),
            "search" => Some(("type", "searchbox".into())),
            _ => Some(("type", "textbox".into())),
        },
        _ if interactive_role => Some(("click", role_attr)),
        _ if node.is_focussable() => Some(("click", "generic".into())),
        _ => None,
    }
}

fn is_pointer_target(doc: &HtmlDocument, node_id: NodeId) -> bool {
    doc.resolved_style_value(node_id, "cursor") == "pointer"
}

fn current_value(
    doc: &HtmlDocument,
    node: &Node,
    tag: &str,
    sensitive: bool,
) -> (Option<String>, ValueState, Option<bool>) {
    if sensitive {
        let present = node
            .element_data()
            .and_then(|data| data.text_input_data())
            .map(|data| !data.editor.text().to_string().is_empty())
            .or_else(|| attr(node, local_name!("value")).map(|v| !v.is_empty()))
            .unwrap_or(false);
        return (
            None,
            if present {
                ValueState::Sensitive
            } else {
                ValueState::Empty
            },
            None,
        );
    }

    let selected = if tag == "option" {
        Some(attr(node, local_name!("selected")).is_some())
    } else {
        None
    };

    let value = if let Some(data) = node.element_data().and_then(|data| data.text_input_data()) {
        let editor_text = data.editor.text().to_string();
        if !editor_text.is_empty() {
            Some(editor_text)
        } else {
            attr(node, local_name!("value")).map(str::to_string)
        }
    } else if tag == "select" {
        node.children.iter().copied().find_map(|child_id| {
            let child = doc.get_node(child_id)?;
            let child_data = child.element_data()?;
            if child_data.name.local.to_string() != "option"
                || child_data.attr(local_name!("selected")).is_none()
            {
                return None;
            }
            child_data
                .attr(local_name!("value"))
                .map(str::to_string)
                .or_else(|| Some(normalized(&child.text_content(), 512)))
        })
    } else if tag == "textarea" {
        Some(node.text_content())
    } else if matches!(tag, "input" | "option") {
        attr(node, local_name!("value")).map(str::to_string)
    } else {
        None
    };

    match value {
        Some(value) if !value.is_empty() => (
            Some(value.chars().take(512).collect()),
            ValueState::Present,
            selected,
        ),
        Some(_) => (Some(String::new()), ValueState::Empty, selected),
        None => (None, ValueState::Unavailable, selected),
    }
}

fn push_field(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn fingerprint(metadata: &SnapshotMetadata, elements: &[ElementSnapshot]) -> String {
    let mut index = Sha256::new();
    for element in elements {
        push_field(&mut index, &element.id);
        push_field(&mut index, &element.role);
        push_field(&mut index, &element.label);
        push_field(
            &mut index,
            &format!(
                "{:.2},{:.2},{:.2},{:.2}",
                element.bounds.x, element.bounds.y, element.bounds.width, element.bounds.height
            ),
        );
        push_field(&mut index, &element.kind);
        push_field(&mut index, if element.enabled { "1" } else { "0" });
        push_field(&mut index, &format!("{:?}", element.checked));
        push_field(&mut index, &format!("{:?}", element.selected));
        push_field(&mut index, &format!("{:?}", element.value_state));
        if let Some(val) = &element.value {
            push_field(&mut index, val);
        }
    }
    let index_digest = index.finalize();

    let mut outer = Sha256::new();
    outer.update(metadata.url.as_bytes());
    outer.update([0]);
    outer.update(metadata.document_generation.to_string().as_bytes());
    outer.update([0]);
    outer.update(index_digest);
    format!("{:x}", outer.finalize())
}

fn collect_visible_text(doc: &HtmlDocument) -> String {
    let mut pieces = Vec::new();
    let mut stack = vec![(doc.root_node().id, false)];
    while let Some((node_id, skip)) = stack.pop() {
        let Some(node) = doc.get_node(node_id) else {
            continue;
        };
        let tag = node
            .element_data()
            .map(|data| data.name.local.to_string().to_ascii_lowercase())
            .unwrap_or_default();
        let skip = skip || matches!(tag.as_str(), "script" | "style" | "noscript" | "template");
        if !skip {
            if let NodeData::Text(text) = &node.data {
                let text = normalized(&text.content, 6000);
                if !text.is_empty() {
                    pieces.push(text);
                }
            }
        }
        for child in node.children.iter().rev() {
            stack.push((*child, skip));
        }
    }
    normalized(&pieces.join(" "), 6000)
}

pub fn observe(doc: &HtmlDocument, metadata: SnapshotMetadata) -> PageSnapshot {
    let mut labels_by_for = HashMap::new();
    let mut stack = vec![doc.root_node().id];
    while let Some(node_id) = stack.pop() {
        let Some(node) = doc.get_node(node_id) else {
            continue;
        };
        if let Some(data) = node.element_data() {
            if data.name.local.to_string() == "label" {
                if let Some(target) = data.attr(local_name!("for")) {
                    labels_by_for.insert(target.to_string(), normalized(&node.text_content(), 160));
                }
            }
        }
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
    }

    let base_url = Url::parse(&metadata.url).ok();
    let mut elements = Vec::new();
    let mut stack = vec![doc.root_node().id];
    while let Some(node_id) = stack.pop() {
        let Some(node) = doc.get_node(node_id) else {
            continue;
        };
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
        let Some(data) = node.element_data() else {
            continue;
        };
        let tag = data.name.local.to_string().to_ascii_lowercase();
        let input_type = attr(node, local_name!("type"))
            .unwrap_or("text")
            .to_ascii_lowercase();
        let mut classified = classify(&tag, &input_type, node);
        if classified.is_none() && is_pointer_target(doc, node_id) {
            classified = Some(("click", "generic".to_string()));
        }
        let Some((kind, role)) = classified else {
            continue;
        };
        let Some(rect) = doc.get_client_bounding_rect(node_id) else {
            continue;
        };
        if rect.width <= 0.0 || rect.height <= 0.0 {
            continue;
        }
        let visible = rect.x < metadata.viewport_width as f64
            && rect.y < metadata.viewport_height as f64
            && rect.x + rect.width > 0.0
            && rect.y + rect.height > 0.0;
        if !visible {
            continue;
        }
        let label = label_for(doc, node, &tag, &labels_by_for);
        let sensitive = is_sensitive(&tag, &input_type, node, &label);
        let (value, value_state, selected) = current_value(doc, node, &tag, sensitive);
        let enabled = attr(node, local_name!("disabled")).is_none()
            && attr(node, local_name!("aria-disabled")) != Some("true");
        let href = attr(node, local_name!("href")).and_then(|href| {
            base_url
                .as_ref()
                .and_then(|base| base.join(href).ok())
                .map(|url| url.to_string())
                .or_else(|| Some(href.to_string()))
        });
        let dom_id = attr(node, local_name!("id"))
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let name = attr(node, local_name!("name"))
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let input_type_opt = if tag == "input" {
            Some(input_type.clone())
        } else {
            None
        };
        let checked = data
            .checkbox_input_checked()
            .or_else(|| (tag == "input" && matches!(input_type.as_str(), "checkbox" | "radio")).then(|| attr(node, local_name!("checked")).is_some()));
        elements.push(ElementSnapshot {
            id: format!("el_{}", elements.len() + 1),
            dom_id,
            name,
            input_type: input_type_opt,
            node_ref: format!("n{}_{}", metadata.page_id, node_id.as_u64()),
            node_id,
            kind: kind.to_string(),
            role,
            tag,
            label,
            bounds: Bounds {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            },
            enabled,
            visible,
            value,
            value_state,
            checked,
            selected,
            href,
        });
        if elements.len() >= 512 {
            break;
        }
    }

    let focused_node = doc.get_focussed_node_id();
    let focused_element_id = focused_node.and_then(|focused| {
        elements
            .iter()
            .find(|element| element.node_id == focused)
            .map(|element| element.id.clone())
    });
    let scroll = doc.viewport_scroll();
    let root = doc.root_node();
    let height = (root.final_layout().size.height as f64)
        .max(root.scrollable_overflow().y1)
        .max(metadata.viewport_height as f64);
    let fingerprint = fingerprint(&metadata, &elements);
    let form_signature = crate::recall::compute_form_signature(&elements);
    let matched_recipe_id =
        crate::recall::find_matching_recipe(&metadata.url, form_signature.as_deref())
            .map(|r| r.recipe_id);

    PageSnapshot {
        snapshot_id: metadata.snapshot_id,
        tab_id: metadata.tab_id,
        page_id: metadata.page_id,
        document_generation: metadata.document_generation,
        fingerprint,
        url: metadata.url,
        title: metadata.title,
        viewport: ViewportSnapshot {
            width: metadata.viewport_width,
            height: metadata.viewport_height,
            scale: metadata.scale,
        },
        scroll: ScrollSnapshot {
            x: scroll.x,
            y: scroll.y,
            height,
        },
        text: collect_visible_text(doc),
        elements,
        focused_element_id,
        form_signature,
        matched_recipe_id,
    }
}
