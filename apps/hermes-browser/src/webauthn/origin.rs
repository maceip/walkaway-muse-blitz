//! Origin and relying party identifier validation (WebAuthn §5.1 / §13.4).
//!
//! Ported from the iOS Equipped Browser
//! `validateOriginAndEffectiveRpId(origin:requestedRpId:topOrigin:crossOrigin:isCreate:)`
//! in `WebAuthnManager.swift`.

use url::{Host, Url};

use crate::webauthn::types::WebAuthnError;

/// The validated origin and effective relying party identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedOrigin {
    /// The effective relying party identifier (lowercased).
    pub effective_rp_id: String,
    /// The normalized origin, with a non-default port retained.
    pub normalized_origin: String,
}

/// Validates an origin and resolves the effective relying party identifier.
///
/// Mirrors the Swift behaviour exactly, including the rule that the origin
/// host must equal the RP ID or be a subdomain of it, and that a non-localhost
/// RP ID must contain a dot.
pub fn validate_origin_and_effective_rp_id(
    origin: &str,
    requested_rp_id: Option<&str>,
    cross_origin: bool,
    is_create: bool,
) -> Result<ValidatedOrigin, WebAuthnError> {
    let url = Url::parse(origin).map_err(|_| {
        WebAuthnError::Security(format!("Invalid origin URL: {origin}"))
    })?;
    let scheme = url.scheme().to_ascii_lowercase();
    let host = match url.host() {
        Some(Host::Domain(domain)) => domain.to_ascii_lowercase(),
        Some(Host::Ipv4(addr)) => addr.to_string(),
        Some(Host::Ipv6(addr)) => addr.to_string(),
        None => {
            return Err(WebAuthnError::Security(format!(
                "Invalid origin URL: {origin}"
            )));
        }
    };
    if host.is_empty() {
        return Err(WebAuthnError::Security(format!(
            "Invalid origin URL: {origin}"
        )));
    }

    let is_localhost = host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.ends_with(".localhost");
    if scheme != "https" && !(scheme == "http" && is_localhost) {
        return Err(WebAuthnError::Security(format!(
            "WebAuthn requires a secure context (HTTPS or localhost), got {origin}"
        )));
    }

    if cross_origin && is_create {
        return Err(WebAuthnError::NotAllowed(
            "PublicKeyCredential creation is not permitted in cross-origin iframes without explicit policy"
                .to_string(),
        ));
    }

    let rp_id = match requested_rp_id {
        Some(requested) if !requested.is_empty() => requested.to_ascii_lowercase(),
        _ => host.clone(),
    };

    // The RP ID must not carry a scheme, port, or path.
    if rp_id.contains("://") || rp_id.contains('/') || rp_id.contains(':') {
        return Err(WebAuthnError::Security(format!(
            "Invalid RP ID format: {rp_id}"
        )));
    }

    // The origin host must be the RP ID itself or a subdomain of it, and a
    // non-localhost RP ID must be a registrable domain (it must contain a dot).
    if host != rp_id {
        let suffix_ok = host.ends_with(&format!(".{rp_id}"));
        let registrable = rp_id.contains('.') || rp_id == "localhost";
        if !suffix_ok || !registrable {
            return Err(WebAuthnError::Security(format!(
                "RP ID '{rp_id}' is not a valid registrable domain suffix for origin host '{host}'"
            )));
        }
    }

    let mut normalized = format!("{scheme}://{host}");
    if let Some(port) = url.port() {
        let is_default = (scheme == "https" && port == 443) || (scheme == "http" && port == 80);
        if !is_default {
            normalized.push_str(&format!(":{port}"));
        }
    }

    Ok(ValidatedOrigin {
        effective_rp_id: rp_id,
        normalized_origin: normalized,
    })
}

/// Returns the registrable domain suffix of a host, used to scope
/// "always allow" decisions to a site rather than a single page.
pub fn registrable_suffix(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() <= 2 {
        return host.to_string();
    }
    // A conservative two-label public suffix approximation. The iOS
    // implementation scoped passkeys by relying party identifier, which is
    // already the registrable domain the relying party chose; this helper is
    // only used to describe the scope to the human.
    labels[labels.len() - 2..].join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_origins_and_defaults_the_rp_id_to_the_host() {
        let validated =
            validate_origin_and_effective_rp_id("https://example.com", None, false, false).unwrap();
        assert_eq!(validated.effective_rp_id, "example.com");
        assert_eq!(validated.normalized_origin, "https://example.com");
    }

    #[test]
    fn accepts_a_subdomain_origin_with_an_explicit_rp_id() {
        let validated =
            validate_origin_and_effective_rp_id("https://login.example.com", Some("example.com"), false, false)
                .unwrap();
        assert_eq!(validated.effective_rp_id, "example.com");
        assert_eq!(validated.normalized_origin, "https://login.example.com");
    }

    #[test]
    fn rejects_an_rp_id_that_is_not_a_suffix_of_the_host() {
        let err =
            validate_origin_and_effective_rp_id("https://evil.com", Some("example.com"), false, false)
                .unwrap_err();
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn rejects_a_single_label_rp_id_for_a_public_host() {
        let err =
            validate_origin_and_effective_rp_id("https://example.com", Some("com"), false, false)
                .unwrap_err();
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn rejects_plain_http_except_for_localhost() {
        let err =
            validate_origin_and_effective_rp_id("http://example.com", None, false, false).unwrap_err();
        assert!(err.detail().contains("secure context"));

        let ok =
            validate_origin_and_effective_rp_id("http://localhost:8080", None, false, false)
                .unwrap();
        assert_eq!(ok.effective_rp_id, "localhost");
        assert_eq!(ok.normalized_origin, "http://localhost:8080");
    }

    #[test]
    fn keeps_a_non_default_port_and_drops_a_default_port() {
        let kept =
            validate_origin_and_effective_rp_id("https://example.com:8443", None, false, false)
                .unwrap();
        assert_eq!(kept.normalized_origin, "https://example.com:8443");

        let dropped =
            validate_origin_and_effective_rp_id("https://example.com:443", None, false, false)
                .unwrap();
        assert_eq!(dropped.normalized_origin, "https://example.com");
    }

    #[test]
    fn rejects_an_rp_id_carrying_a_scheme_port_or_path() {
        for bad in ["https://example.com", "example.com:443", "example.com/path"] {
            let err = validate_origin_and_effective_rp_id("https://example.com", Some(bad), false, false)
                .unwrap_err();
            assert_eq!(err.dom_exception_name(), "SecurityError", "accepted {bad}");
        }
    }

    #[test]
    fn rejects_cross_origin_creation() {
        let err =
            validate_origin_and_effective_rp_id("https://example.com", None, true, true).unwrap_err();
        assert_eq!(err.dom_exception_name(), "NotAllowedError");
    }

    #[test]
    fn rejects_an_ipv6_host_as_a_relying_party_identifier() {
        // The RP ID must not contain a colon, so an IPv6 literal host cannot
        // serve as an RP ID. The iOS implementation rejects it the same way
        // (`rpId.contains(":")`), and WebAuthn §5.4 likewise requires the RP
        // ID to be a valid domain string.
        let err =
            validate_origin_and_effective_rp_id("http://[::1]:9000", None, false, false).unwrap_err();
        assert_eq!(err.dom_exception_name(), "SecurityError");
        assert!(err.detail().contains("Invalid RP ID format"));
    }

    #[test]
    fn rejects_an_ipv6_host_with_an_explicit_domain_rp_id() {
        // The origin host still has to be a subdomain of the RP ID.
        let err = validate_origin_and_effective_rp_id(
            "http://[::1]:9000",
            Some("example.com"),
            false,
            false,
        )
        .unwrap_err();
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }
}
