//! WebAuthn `clientDataJSON` construction.
//!
//! Ported from the iOS Equipped Browser `encodeClientDataJSON` helper in
//! `WebAuthnManager.swift`, including the field order it emits:
//! `type`, `challenge`, `origin`, `crossOrigin`, and the optional `topOrigin`.
//!
//! The bytes produced here are the bytes that get hashed into the signature,
//! so the producer and the relying party must agree on them exactly. The
//! relying party parses the JSON, so field order is not semantically
//! significant, but keeping the Swift order makes a byte-level diff against
//! the iOS implementation meaningful.

use crate::webauthn::b64::b64url_encode;

/// The `type` member of `clientDataJSON`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CeremonyType {
    /// `webauthn.create`.
    Create,
    /// `webauthn.get`.
    Get,
}

impl CeremonyType {
    /// The literal value written into the JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            CeremonyType::Create => "webauthn.create",
            CeremonyType::Get => "webauthn.get",
        }
    }
}

/// Builds the `clientDataJSON` bytes for a ceremony.
pub fn client_data_json(
    ceremony: CeremonyType,
    challenge: &[u8],
    origin: &str,
    cross_origin: bool,
    top_origin: Option<&str>,
) -> Vec<u8> {
    let mut parts: Vec<String> = Vec::with_capacity(5);
    parts.push(format!("\"type\":\"{}\"", escape_json(ceremony.as_str())));
    parts.push(format!(
        "\"challenge\":\"{}\"",
        b64url_encode(challenge)
    ));
    parts.push(format!("\"origin\":\"{}\"", escape_json(origin)));
    parts.push(format!("\"crossOrigin\":{cross_origin}"));
    if let Some(top) = top_origin {
        parts.push(format!("\"topOrigin\":\"{}\"", escape_json(top)));
    }
    format!("{{{}}}", parts.join(",")).into_bytes()
}

/// Escapes a string for embedding in a JSON string literal.
///
/// Matches the Swift helper (backslash and double quote) and additionally
/// escapes control characters, which the Swift helper left raw and which
/// would produce invalid JSON.
fn escape_json(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_the_same_field_order_as_the_ios_implementation() {
        let json = client_data_json(CeremonyType::Get, b"chal", "https://example.com", false, None);
        assert_eq!(
            String::from_utf8(json).unwrap(),
            r#"{"type":"webauthn.get","challenge":"Y2hhbA","origin":"https://example.com","crossOrigin":false}"#
        );
    }

    #[test]
    fn emits_create_with_top_origin_when_present() {
        let json = client_data_json(
            CeremonyType::Create,
            b"chal",
            "https://example.com",
            true,
            Some("https://top.example"),
        );
        let text = String::from_utf8(json).unwrap();
        assert!(text.starts_with(r#"{"type":"webauthn.create","challenge":"Y2hhbA""#));
        assert!(text.contains(r#""crossOrigin":true"#));
        assert!(text.contains(r#""topOrigin":"https://top.example""#));
        assert!(text.ends_with('}'));
    }

    #[test]
    fn escapes_quotes_and_backslashes() {
        let json = client_data_json(
            CeremonyType::Get,
            b"c",
            r#"https://example.com/"a\b"#,
            false,
            None,
        );
        let text = String::from_utf8(json).unwrap();
        assert!(text.contains(r#""origin":"https://example.com/\"a\\b""#));
    }
}
