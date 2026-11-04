//! Typed page-endpoint commands — step 1 of the Ladybird-style split.
//!
//! Ladybird declares the browser↔WebContent protocol in `.ipc` files and
//! routes generated typed messages by `PageId`. Hermes keeps NDJSON at the
//! external agent/host edge, but page execution inside the host speaks this
//! typed command vocabulary instead of untyped JSON. When the page layer
//! moves to a child process, this enum is the wire schema for that boundary.

use serde_json::{Value, json};

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
            Self::Scroll { .. } => "scroll",
            Self::Navigate { .. } => "navigate",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Reload => "reload",
        }
    }

    pub fn target_id(&self) -> Option<&str> {
        match self {
            Self::Click { target_id, .. } if !target_id.is_empty() => Some(target_id),
            _ => None,
        }
    }

    pub fn target_ref(&self) -> Option<&str> {
        match self {
            Self::Click { target_ref, .. } => target_ref.as_deref(),
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
            Self::Scroll { dy } => json!({"kind": "scroll", "dy": dy}),
            Self::Navigate { url } => json!({"kind": "navigate", "url": url}),
            Self::Back => json!({"kind": "back"}),
            Self::Forward => json!({"kind": "forward"}),
            Self::Reload => json!({"kind": "reload"}),
        }
    }
}
