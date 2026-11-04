//! Structural `FormRecipe` zero-LLM recall store (`HERMES_RECALL_PATH`).
//!
//! Computes deterministic structural signatures for form controls in a
//! `PageSnapshot`, records non-secret field bindings + blind vault secret
//! references (`origin`, `username`) when an approved `fill_form` executes,
//! and replays matched recipes in `< 1ms` with zero LLM calls.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use url::Url;

use crate::control;
use crate::observe::{ElementSnapshot, PageSnapshot};
use crate::page_endpoint::FormFieldCommand;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeField {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub label: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormRecipe {
    pub recipe_id: String,
    pub origin: String,
    pub form_signature: String,
    pub fields: Vec<RecipeField>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecallStore {
    pub recipes: Vec<FormRecipe>,
}

fn recall_path() -> PathBuf {
    std::env::var("HERMES_RECALL_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/hermes-recall.json"))
}

pub fn load_store() -> RecallStore {
    let path = recall_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return RecallStore::default();
    };
    serde_json::from_str::<RecallStore>(&raw).unwrap_or_default()
}

fn save_store(store: &RecallStore) -> Result<(), String> {
    let path = recall_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let serialized =
        serde_json::to_string_pretty(store).map_err(|e| format!("serialize recall: {e}"))?;
    let mut opts = OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&path)
        .map_err(|e| format!("open recall {}: {e}", path.display()))?;
    file.write_all(serialized.as_bytes())
        .map_err(|e| format!("write recall {}: {e}", path.display()))?;
    file.flush()
        .map_err(|e| format!("flush recall {}: {e}", path.display()))?;
    Ok(())
}

pub fn origin_of(url_str: &str) -> String {
    let Ok(parsed) = Url::parse(url_str) else {
        return url_str.to_string();
    };
    match (parsed.scheme(), parsed.host_str(), parsed.port()) {
        (scheme, Some(host), Some(port)) => format!("{scheme}://{host}:{port}"),
        (scheme, Some(host), None) => format!("{scheme}://{host}"),
        _ => url_str.to_string(),
    }
}

pub fn compute_form_signature(elements: &[ElementSnapshot]) -> Option<String> {
    let controls: Vec<&ElementSnapshot> = elements
        .iter()
        .filter(|e| matches!(e.tag.as_str(), "input" | "select" | "textarea"))
        .collect();
    if controls.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    for el in controls {
        hasher.update(el.tag.as_bytes());
        hasher.update([0]);
        hasher.update(el.input_type.as_deref().unwrap_or_default().as_bytes());
        hasher.update([0]);
        hasher.update(el.name.as_deref().unwrap_or_default().as_bytes());
        hasher.update([0]);
        hasher.update(el.dom_id.as_deref().unwrap_or_default().as_bytes());
        hasher.update([0]);
        hasher.update(el.role.as_bytes());
        hasher.update([0xff]);
    }
    let digest = format!("{:x}", hasher.finalize());
    Some(format!("sig_{}", &digest[..16]))
}

pub fn find_matching_recipe(url_str: &str, form_signature: Option<&str>) -> Option<FormRecipe> {
    let sig = form_signature?;
    let orig = origin_of(url_str);
    let store = load_store();
    store
        .recipes
        .iter()
        .find(|r| r.form_signature == sig && r.origin == orig)
        .or_else(|| store.recipes.iter().find(|r| r.form_signature == sig))
        .cloned()
}

pub fn get_recipe_by_id(recipe_id: &str) -> Option<FormRecipe> {
    let store = load_store();
    store
        .recipes
        .into_iter()
        .find(|r| r.recipe_id == recipe_id)
}

pub fn record_recipe(
    url_str: &str,
    form_signature: &str,
    fields: &[FormFieldCommand],
    snapshot: &PageSnapshot,
) {
    let mut recipe_fields = Vec::new();
    for f in fields {
        let Some(el) = snapshot.elements.iter().find(|e| {
            f.target_ref.as_deref() == Some(e.node_ref.as_str())
                || f.target_id.as_deref() == Some(e.id.as_str())
        }) else {
            continue;
        };
        let value = if f.action == "fill_secret" {
            None
        } else {
            f.value.clone()
        };
        recipe_fields.push(RecipeField {
            dom_id: el.dom_id.clone(),
            name: el.name.clone(),
            label: el.label.clone(),
            action: f.action.clone(),
            value,
            origin: f.origin.clone(),
            username: f.username.clone(),
        });
    }
    if recipe_fields.is_empty() {
        return;
    }
    let orig = origin_of(url_str);
    let mut store = load_store();
    if let Some(existing) = store
        .recipes
        .iter_mut()
        .find(|r| r.origin == orig && r.form_signature == form_signature)
    {
        existing.fields = recipe_fields;
        existing.updated_at = control::now_iso();
    } else {
        let recipe_id = format!("recipe_{}", store.recipes.len() + 1);
        store.recipes.push(FormRecipe {
            recipe_id,
            origin: orig,
            form_signature: form_signature.to_string(),
            fields: recipe_fields,
            updated_at: control::now_iso(),
        });
    }
    let _ = save_store(&store);
}
