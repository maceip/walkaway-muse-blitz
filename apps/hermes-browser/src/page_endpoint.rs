//! Typed page-endpoint commands — step 1 of the Ladybird-style split.
//!
//! Ladybird declares the browser↔WebContent protocol in `.ipc` files and
//! routes generated typed messages by `PageId`. Hermes keeps NDJSON at the
//! external agent/host edge, but page execution inside the host speaks this
//! typed command vocabulary instead of untyped JSON. When the page layer
//! moves to a child process, this enum is the wire schema for that boundary.

use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct FormFieldCommand {
    pub target_id: Option<String>,
    pub target_ref: Option<String>,
    pub action: String,
    pub value: Option<String>,
    pub origin: Option<String>,
    pub username: Option<String>,
    pub account_id: Option<String>,
}

impl FormFieldCommand {
    pub fn from_value(val: &Value) -> Option<Self> {
        let action = val.get("action").and_then(Value::as_str)?.to_string();
        let target_id = val
            .get("target_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let target_ref = val
            .get("target_ref")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if target_id.is_none() && target_ref.is_none() {
            return None;
        }
        let value = val.get("value").and_then(Value::as_str).map(str::to_string);
        let origin = val
            .get("origin")
            .and_then(Value::as_str)
            .map(str::to_string);
        let username = val
            .get("username")
            .and_then(Value::as_str)
            .map(str::to_string);
        let account_id = val
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string);

        match action.as_str() {
            "type" | "select" | "check" => {
                if value.is_none() {
                    return None;
                }
            }
            "fill_secret" => {
                if value.is_some() || origin.is_none() || username.is_none() {
                    return None;
                }
            }
            _ => return None,
        }

        Some(Self {
            target_id,
            target_ref,
            action,
            value,
            origin,
            username,
            account_id,
        })
    }

    pub fn to_value(&self) -> Value {
        if self.action == "fill_secret" {
            json!({
                "target_id": self.target_id,
                "target_ref": self.target_ref,
                "action": "fill_secret",
                "origin": self.origin,
                "username": self.username,
                "account_id": self.account_id,
            })
        } else {
            json!({
                "target_id": self.target_id,
                "target_ref": self.target_ref,
                "action": self.action,
                "value": self.value,
            })
        }
    }
}

/// One command the trusted host may send to a page instance. A command is
/// only constructed from an operation that proposal validation accepted, and
/// only dispatched after a human decision bound to tab, page, generation,
/// and fingerprint.
#[derive(Clone, Debug, PartialEq)]
pub enum PageCommand {
    Click {
        target_id: String,
        target_ref: Option<String>,
    },
    Type {
        target_id: String,
        target_ref: Option<String>,
        text: String,
    },
    Select {
        target_id: String,
        target_ref: Option<String>,
        value: String,
    },
    FillForm {
        fields: Vec<FormFieldCommand>,
    },
    ReplayRecipe {
        recipe_id: String,
    },
    Submit {
        target_id: String,
        target_ref: Option<String>,
    },
    Scroll {
        dy: f64,
    },
    Navigate {
        url: String,
    },
    Back,
    Forward,
    Reload,
}

impl PageCommand {
    pub fn from_operation(operation: &Value) -> Option<Self> {
        match operation.get("kind").and_then(Value::as_str)? {
            "click" => Some(Self::Click {
                target_id: operation
                    .get("target_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                target_ref: operation
                    .get("target_ref")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
            "type" => Some(Self::Type {
                target_id: operation
                    .get("target_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                target_ref: operation
                    .get("target_ref")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                text: operation
                    .get("text")
                    .or_else(|| operation.get("value"))
                    .and_then(Value::as_str)?
                    .to_string(),
            }),
            "select" => Some(Self::Select {
                target_id: operation
                    .get("target_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                target_ref: operation
                    .get("target_ref")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                value: operation.get("value").and_then(Value::as_str)?.to_string(),
            }),
            "fill_form" => {
                let arr = operation.get("fields").and_then(Value::as_array)?;
                if arr.is_empty() {
                    return None;
                }
                let mut fields = Vec::with_capacity(arr.len());
                for item in arr {
                    fields.push(FormFieldCommand::from_value(item)?);
                }
                Some(Self::FillForm { fields })
            }
            "replay_recipe" => Some(Self::ReplayRecipe {
                recipe_id: operation
                    .get("recipe_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }),
            "submit" => Some(Self::Submit {
                target_id: operation
                    .get("target_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                target_ref: operation
                    .get("target_ref")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
            "scroll" => Some(Self::Scroll {
                dy: operation
                    .get("dy")
                    .and_then(Value::as_f64)
                    .filter(|dy| dy.is_finite())?
                    .clamp(-10_000.0, 10_000.0),
            }),
            "navigate" => Some(Self::Navigate {
                url: operation.get("url").and_then(Value::as_str)?.to_string(),
            }),
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "reload" => Some(Self::Reload),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Click { .. } => "click",
            Self::Type { .. } => "type",
            Self::Select { .. } => "select",
            Self::FillForm { .. } => "fill_form",
            Self::ReplayRecipe { .. } => "replay_recipe",
            Self::Submit { .. } => "submit",
            Self::Scroll { .. } => "scroll",
            Self::Navigate { .. } => "navigate",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Reload => "reload",
        }
    }

    pub fn target_id(&self) -> Option<&str> {
        match self {
            Self::Click { target_id, .. }
            | Self::Type { target_id, .. }
            | Self::Select { target_id, .. }
            | Self::Submit { target_id, .. }
                if !target_id.is_empty() =>
            {
                Some(target_id)
            }
            _ => None,
        }
    }

    pub fn target_ref(&self) -> Option<&str> {
        match self {
            Self::Click { target_ref, .. }
            | Self::Type { target_ref, .. }
            | Self::Select { target_ref, .. }
            | Self::Submit { target_ref, .. } => target_ref.as_deref(),
            _ => None,
        }
    }

    /// Serialized form for protocol events and audit records. Execution never
    /// parses this back; the typed value is the authority.
    pub fn to_operation(&self) -> Value {
        match self {
            Self::Click {
                target_id,
                target_ref,
            } => json!({"kind": "click", "target_id": target_id, "target_ref": target_ref}),
            Self::Type {
                target_id,
                target_ref,
                text,
            } => json!({"kind": "type", "target_id": target_id, "target_ref": target_ref, "text": text}),
            Self::Select {
                target_id,
                target_ref,
                value,
            } => json!({"kind": "select", "target_id": target_id, "target_ref": target_ref, "value": value}),
            Self::FillForm { fields } => json!({
                "kind": "fill_form",
                "fields": fields.iter().map(FormFieldCommand::to_value).collect::<Vec<_>>(),
            }),
            Self::ReplayRecipe { recipe_id } => json!({
                "kind": "replay_recipe",
                "recipe_id": recipe_id,
            }),
            Self::Submit {
                target_id,
                target_ref,
            } => json!({"kind": "submit", "target_id": target_id, "target_ref": target_ref}),
            Self::Scroll { dy } => json!({"kind": "scroll", "dy": dy}),
            Self::Navigate { url } => json!({"kind": "navigate", "url": url}),
            Self::Back => json!({"kind": "back"}),
            Self::Forward => json!({"kind": "forward"}),
            Self::Reload => json!({"kind": "reload"}),
        }
    }
}
