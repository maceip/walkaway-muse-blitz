//! Origin and relying party identifier validation (WebAuthn §5.1 / §13.4).
//!
//! Ported from the iOS Equipped Browser
//! `validateOriginAndEffectiveRpId(origin:requestedRpId:topOrigin:crossOrigin:isCreate:)`
//! in `WebAuthnManager.swift`.
//!
//! The iOS implementation could only say yes or no to an RP ID, because it
//! never had to consult the network. The browser does: WebAuthn Level 3 §5.11.1
//! adds *Related Origin Requests*, which lets a relying party name an RP ID
//! that is not a suffix of the caller's host, provided the relying party
//! publishes the caller's origin in its own `/.well-known/webauthn` document.
//! That is a network fetch, so the validation is split in two:
//!
//!   * [`validate_origin_and_effective_rp_id`] is the cheap, synchronous
//!     half. It checks the scheme, the secure context, the RP ID's format,
//!     and whether the RP ID is the origin's host or a suffix of it. When the
//!     RP ID *is* a suffix, the answer is final and **no fetch happens**.
//!   * When the cheap half says the origin is not related to the RP ID by
//!     suffix, it returns [`OriginValidation::NeedsRelatedOrigins`] and the
//!     caller runs the related origins validation procedure in
//!     [`crate::webauthn::related_origins`], which fetches the relying
//!     party's well-known document over the network.
//!
//! The network fetch must not run on the UI thread, so the host resolves it
//! off the UI thread and settles the page's promise when the answer arrives.

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

/// What the cheap, synchronous half of the validation decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginValidation {
    /// The origin is related to the RP ID by the suffix rule, and the answer
    /// is final. No well-known fetch is needed.
    Suffix(ValidatedOrigin),
    /// The origin's host is neither the RP ID nor a subdomain of it. The
    /// related origins validation procedure must decide.
    NeedsRelatedOrigins {
        /// The validated origin, with the requested RP ID already resolved.
        validated: ValidatedOrigin,
        /// The origin the page was actually served from.
        caller_origin: String,
        /// The RP ID the relying party asked for.
        rp_id: String,
    },
}

/// Validates an origin and resolves the effective relying party identifier.
///
/// Mirrors the Swift behaviour exactly, including the rule that the origin
/// host must equal the RP ID or be a subdomain of it, and that a non-localhost
/// RP ID must contain a dot.
///
/// When the origin host is neither the RP ID nor a subdomain of it, this
/// returns [`OriginValidation::NeedsRelatedOrigins`] instead of an error:
/// the related origins validation procedure (WebAuthn §5.11.1) has to run
/// first, and that procedure needs the network. Use
/// [`validate_origin_and_effective_rp_id_async`] to get the final answer.
///
/// `caller_origin` is the origin the page was actually served from. It must
/// never be taken from the page's own request parameters.
pub fn validate_origin_and_effective_rp_id(
    origin: &str,
    requested_rp_id: Option<&str>,
    cross_origin: bool,
    is_create: bool,
) -> Result<OriginValidation, WebAuthnError> {
    let url = Url::parse(origin)
        .map_err(|_| WebAuthnError::Security(format!("Invalid origin URL: {origin}")))?;
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

    let is_localhost =
        host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".localhost");
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

    let mut normalized = format!("{scheme}://{host}");
    if let Some(port) = url.port() {
        let is_default = (scheme == "https" && port == 443) || (scheme == "http" && port == 80);
        if !is_default {
            normalized.push_str(&format!(":{port}"));
        }
    }

    let validated = ValidatedOrigin {
        effective_rp_id: rp_id.clone(),
        normalized_origin: normalized,
    };

    // The RP ID must be a registrable domain suffix of the origin's host, or
    // the origin's host itself. A non-localhost RP ID must be a registrable
    // domain, which means it must contain a dot: a single-label RP ID is
    // never registrable, and no relying party can be related to it either.
    let registrable = rp_id.contains('.') || rp_id == "localhost";
    if !registrable {
        return Err(WebAuthnError::Security(format!(
            "RP ID '{rp_id}' is not a valid registrable domain suffix for origin host '{host}'"
        )));
    }

    // The origin host must be the RP ID itself or a subdomain of it.
    if host != rp_id && !host.ends_with(&format!(".{rp_id}")) {
        // The origin host is not under the RP ID. The spec does not stop
        // here: it runs the related origins validation procedure, which
        // asks the relying party itself whether it is related to this
        // origin. That procedure needs the network, so it is left to the
        // async half.
        return Ok(OriginValidation::NeedsRelatedOrigins {
            validated,
            caller_origin: origin.to_string(),
            rp_id,
        });
    }

    Ok(OriginValidation::Suffix(validated))
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

    /// A validated origin, for the callers that only care about the answer
    /// and not about how it was reached.
    fn suffix(
        origin: &str,
        requested_rp_id: Option<&str>,
        cross_origin: bool,
        is_create: bool,
    ) -> ValidatedOrigin {
        match validate_origin_and_effective_rp_id(origin, requested_rp_id, cross_origin, is_create)
            .unwrap()
        {
            OriginValidation::Suffix(validated) => validated,
            OriginValidation::NeedsRelatedOrigins { .. } => {
                panic!("{origin} should have been decided by the suffix rule")
            }
        }
    }

    #[test]
    fn accepts_https_origins_and_defaults_the_rp_id_to_the_host() {
        let validated = suffix("https://example.com", None, false, false);
        assert_eq!(validated.effective_rp_id, "example.com");
        assert_eq!(validated.normalized_origin, "https://example.com");
    }

    #[test]
    fn accepts_a_subdomain_origin_with_an_explicit_rp_id() {
        let validated = suffix(
            "https://login.example.com",
            Some("example.com"),
            false,
            false,
        );
        assert_eq!(validated.effective_rp_id, "example.com");
        assert_eq!(validated.normalized_origin, "https://login.example.com");
    }

    #[test]
    fn a_non_suffix_rp_id_is_deferred_to_the_related_origins_procedure() {
        // The suffix rule does not decide this case. The related origins
        // procedure does, and it needs the network, so the synchronous half
        // defers rather than deciding.
        let outcome = validate_origin_and_effective_rp_id(
            "https://evil.com",
            Some("example.com"),
            false,
            false,
        )
        .unwrap();
        match outcome {
            OriginValidation::NeedsRelatedOrigins {
                validated,
                caller_origin,
                rp_id,
            } => {
                assert_eq!(validated.effective_rp_id, "example.com");
                assert_eq!(caller_origin, "https://evil.com");
                assert_eq!(rp_id, "example.com");
            }
            OriginValidation::Suffix(validated) => {
                panic!("expected a deferred decision, got {validated:?}")
            }
        }
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
        let err = validate_origin_and_effective_rp_id("http://example.com", None, false, false)
            .unwrap_err();
        assert!(err.detail().contains("secure context"));

        let ok = suffix("http://localhost:8080", None, false, false);
        assert_eq!(ok.effective_rp_id, "localhost");
        assert_eq!(ok.normalized_origin, "http://localhost:8080");
    }

    #[test]
    fn keeps_a_non_default_port_and_drops_a_default_port() {
        let kept = suffix("https://example.com:8443", None, false, false);
        assert_eq!(kept.normalized_origin, "https://example.com:8443");

        let dropped = suffix("https://example.com:443", None, false, false);
        assert_eq!(dropped.normalized_origin, "https://example.com");
    }

    #[test]
    fn rejects_an_rp_id_carrying_a_scheme_port_or_path() {
        for bad in ["https://example.com", "example.com:443", "example.com/path"] {
            let err =
                validate_origin_and_effective_rp_id("https://example.com", Some(bad), false, false)
                    .unwrap_err();
            assert_eq!(err.dom_exception_name(), "SecurityError", "accepted {bad}");
        }
    }

    #[test]
    fn rejects_cross_origin_creation() {
        let err = validate_origin_and_effective_rp_id("https://example.com", None, true, true)
            .unwrap_err();
        assert_eq!(err.dom_exception_name(), "NotAllowedError");
    }

    #[test]
    fn rejects_an_ipv6_host_as_a_relying_party_identifier() {
        // The RP ID must not contain a colon, so an IPv6 literal host cannot
        // serve as an RP ID. The iOS implementation rejects it the same way
        // (`rpId.contains(":")`), and WebAuthn §5.4 likewise requires the RP
        // ID to be a valid domain string.
        let err = validate_origin_and_effective_rp_id("http://[::1]:9000", None, false, false)
            .unwrap_err();
        assert_eq!(err.dom_exception_name(), "SecurityError");
        assert!(err.detail().contains("Invalid RP ID format"));
    }

    #[test]
    fn defers_an_ipv6_host_with_an_explicit_domain_rp_id() {
        // The origin host is not a subdomain of the RP ID, so the suffix rule
        // does not decide this case. The related origins procedure does, and
        // it needs the network, so the synchronous half defers.
        let outcome = validate_origin_and_effective_rp_id(
            "http://[::1]:9000",
            Some("example.com"),
            false,
            false,
        )
        .unwrap();
        match outcome {
            OriginValidation::NeedsRelatedOrigins { rp_id, .. } => {
                assert_eq!(rp_id, "example.com");
            }
            OriginValidation::Suffix(validated) => {
                panic!("the suffix rule should not have decided this: {validated:?}")
            }
        }
    }

    #[test]
    fn localhost_and_loopback_are_decided_by_the_suffix_rule() {
        // The RP-ID suffix check must not misfire for localhost and the
        // loopback addresses. `localhost` is a single-label RP ID but it is
        // explicitly registrable, and `127.0.0.1` is its own host.
        let ok = suffix("http://localhost:8080", Some("localhost"), false, false);
        assert_eq!(ok.effective_rp_id, "localhost");

        let loopback = suffix("http://127.0.0.1:8080", Some("127.0.0.1"), false, false);
        assert_eq!(loopback.effective_rp_id, "127.0.0.1");

        let sub = suffix("http://app.localhost:3000", Some("localhost"), false, false);
        assert_eq!(sub.effective_rp_id, "localhost");
    }

    // --- the related origins procedure -----------------------------------
    //
    // These tests run the real fetch against a real local HTTPS server. The
    // server is started for the test and the fetch is pointed at it, so the
    // path under test is the production one: a real socket, a real
    // handshake, a real status code and a real content type.

    use crate::webauthn::related_origins;
    use crate::webauthn::test_server::{TestResponse, TestServer};

    /// The related origins tests all share one HTTP client override, so they
    /// take turns rather than racing each other.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Runs the related origins procedure against a local server that
    /// answers with `response`.
    ///
    /// The server's host name is mapped onto its port, so the URL the fetch
    /// builds is exactly the one the spec asks for.
    fn related_origins_against(response: TestResponse) -> Result<bool, WebAuthnError> {
        // The client override is process-wide, so only one of these tests may
        // hold it at a time.
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = TestServer::start(response);
        related_origins::set_client_for_test(server.client());
        // The fetch needs a Tokio reactor. The browser's own runtime is
        // private, but `enter_runtime` enters it.
        let outcome = {
            let _runtime = crate::browser::enter_runtime();
            pollster::block_on(related_origins::validate(
                "https://caller.example",
                &server.rp_id(),
            ))
        };
        related_origins::reset_client_for_test();
        outcome
    }

    #[test]
    fn a_suffix_rp_id_is_decided_without_any_fetch() {
        // The suffix case must not touch the network at all. The server
        // records every path it is asked for, so a fetch would show up.
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = TestServer::start(TestResponse::json("{\"origins\":[]}"));
        related_origins::set_client_for_test(server.client());
        let outcome = validate_origin_and_effective_rp_id(
            "https://login.example.com",
            Some("example.com"),
            false,
            false,
        )
        .unwrap();
        related_origins::reset_client_for_test();

        match outcome {
            OriginValidation::Suffix(validated) => {
                assert_eq!(validated.effective_rp_id, "example.com");
            }
            OriginValidation::NeedsRelatedOrigins { .. } => {
                panic!("the suffix rule should have decided this")
            }
        }
        let requests = server.requests.lock().expect("the request log");
        assert!(
            requests.is_empty(),
            "the well-known document must not be fetched when the RP ID is a suffix: {requests:?}"
        );
    }

    #[test]
    fn accepts_an_origin_the_relying_party_lists() {
        // The relying party's own document names the caller's origin, so the
        // ceremony may go ahead.
        let outcome = related_origins_against(TestResponse::json(
            r#"{"origins": ["https://caller.example"]}"#,
        ));
        assert_eq!(outcome.expect("the document is well formed"), true);
    }

    #[test]
    fn refuses_an_origin_the_relying_party_does_not_list() {
        // The relying party's document is well formed but names a different
        // origin. The procedure answers false.
        let outcome = related_origins_against(TestResponse::json(
            r#"{"origins": ["https://someone-else.example"]}"#,
        ));
        assert_eq!(outcome.expect("the document is well formed"), false);
    }

    #[test]
    fn refuses_an_origin_when_the_document_has_no_origins_property() {
        // The `origins` property is missing, so the document is refused.
        let outcome = related_origins_against(TestResponse::json(r#"{"other": []}"#));
        let err = outcome.expect_err("a document with no origins is refused");
        assert_eq!(err.dom_exception_name(), "SecurityError");
        assert!(err.detail().contains("origins"), "{}", err.detail());
    }

    #[test]
    fn refuses_an_origin_when_the_document_is_not_valid_json() {
        // The body is not JSON at all.
        let outcome = related_origins_against(TestResponse::json("this is not json"));
        let err = outcome.expect_err("a body that is not JSON is refused");
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn refuses_an_origin_when_the_document_is_not_an_object() {
        // The body is valid JSON, but it is an array rather than an object.
        let outcome = related_origins_against(TestResponse::json(r#"["https://caller.example"]"#));
        let err = outcome.expect_err("a body that is not an object is refused");
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn refuses_an_origin_when_the_origins_property_is_not_an_array() {
        // The `origins` property is present but is not an array of strings.
        let outcome = related_origins_against(TestResponse::json(
            r#"{"origins": "https://caller.example"}"#,
        ));
        let err = outcome.expect_err("an origins property that is not an array is refused");
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn refuses_an_origin_when_the_origins_array_holds_a_non_string() {
        // The `origins` property is an array, but one of its members is not a
        // string, so it is not "an array of strings".
        let outcome = related_origins_against(TestResponse::json(
            r#"{"origins": ["https://caller.example", 7]}"#,
        ));
        let err = outcome.expect_err("an origins array with a non-string is refused");
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn refuses_an_origin_when_the_document_is_not_json_media() {
        // The status is 200, but the content type is not application/json.
        let outcome = related_origins_against(TestResponse::with(
            200,
            "text/html",
            r#"{"origins": ["https://caller.example"]}"#,
        ));
        let err = outcome.expect_err("a document that is not application/json is refused");
        assert_eq!(err.dom_exception_name(), "SecurityError");
        assert!(
            err.detail().contains("application/json"),
            "{}",
            err.detail()
        );
    }

    #[test]
    fn refuses_an_origin_when_the_status_is_not_200() {
        // The content type is right, but the status is not 200.
        let outcome = related_origins_against(TestResponse::with(
            404,
            "application/json",
            r#"{"origins": ["https://caller.example"]}"#,
        ));
        let err = outcome.expect_err("a document with a 404 status is refused");
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn follows_a_redirect_and_reads_the_status_after_it() {
        // The status that counts is the one after redirects. The server
        // redirects once and then answers 200, so the procedure accepts.
        let outcome = related_origins_against(
            TestResponse::json(r#"{"origins": ["https://caller.example"]}"#).redirecting(),
        );
        assert_eq!(outcome.expect("the document is well formed"), true);
    }

    #[test]
    fn honours_the_max_labels_cap() {
        // The procedure counts the distinct registrable origin labels it has
        // seen, and stops honouring new ones once the cap is reached.
        //
        // The caller's origin is listed after five other labels have been
        // seen, so the cap makes the procedure skip it and answer false.
        let mut origins: Vec<String> = Vec::new();
        for i in 0..related_origins::MAX_RELATED_ORIGIN_LABELS {
            origins.push(format!("https://other-{i}.example"));
        }
        origins.push("https://caller.example".to_string());
        let body = serde_json::json!({ "origins": origins }).to_string();

        let outcome = related_origins_against(TestResponse::json(body));
        assert_eq!(
            outcome.expect("the document is well formed"),
            false,
            "the label cap must stop the caller from being honoured"
        );
    }

    #[test]
    fn accepts_an_origin_listed_within_the_max_labels_cap() {
        // The caller's origin is listed before the cap is reached, so the
        // procedure accepts it.
        let mut origins: Vec<String> = Vec::new();
        for i in 0..(related_origins::MAX_RELATED_ORIGIN_LABELS - 1) {
            origins.push(format!("https://other-{i}.example"));
        }
        origins.push("https://caller.example".to_string());
        let body = serde_json::json!({ "origins": origins }).to_string();

        let outcome = related_origins_against(TestResponse::json(body));
        assert_eq!(outcome.expect("the document is well formed"), true);
    }

    #[test]
    fn skips_unparsable_origins_and_keeps_looking() {
        // An origin that is not a URL is skipped, and the rest of the list
        // is still consulted.
        let outcome = related_origins_against(TestResponse::json(
            r#"{"origins": ["not a url", "https://caller.example"]}"#,
        ));
        assert_eq!(outcome.expect("the document is well formed"), true);
    }

    #[test]
    fn compares_origins_on_scheme_host_and_port() {
        // The listed origin has to match the caller's origin on scheme, host
        // and port. A different port is a different origin.
        let outcome = related_origins_against(TestResponse::json(
            r#"{"origins": ["https://caller.example:8443"]}"#,
        ));
        assert_eq!(outcome.expect("the document is well formed"), false);

        let outcome = related_origins_against(TestResponse::json(
            r#"{"origins": ["http://caller.example"]}"#,
        ));
        assert_eq!(outcome.expect("the document is well formed"), false);
    }

    #[test]
    fn the_whole_chain_lets_a_related_origin_through() {
        // This is the whole path a page takes when the relying party's RP ID
        // is not a suffix of the origin's host:
        //
        //   1. The synchronous validation defers, because the origin is not
        //      under the RP ID.
        //   2. The related origins procedure fetches the relying party's own
        //      well-known document and finds the caller's origin in it.
        //   3. The broker accepts the ceremony, because the caller says the
        //      related origins check has been made.
        //
        // The third step is the one that matters for security: without the
        // related origins check, the broker refuses the origin outright.
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let origin = "https://caller.example";
        let rp_id = "rp.example";

        let server = TestServer::start_for_host(
            rp_id,
            TestResponse::json(r#"{"origins": ["https://caller.example"]}"#),
        );
        related_origins::set_client_for_test(server.client());

        // 1. The synchronous half defers.
        let outcome = validate_origin_and_effective_rp_id(origin, Some(rp_id), false, false)
            .expect("the origin is valid");
        match outcome {
            OriginValidation::NeedsRelatedOrigins { rp_id: deferred, .. } => {
                assert_eq!(deferred, rp_id);
            }
            OriginValidation::Suffix(validated) => {
                panic!("the suffix rule should not have decided this: {validated:?}")
            }
        }

        // 2. The related origins procedure accepts.
        let related = {
            let _runtime = crate::browser::enter_runtime();
            pollster::block_on(related_origins::validate(origin, rp_id))
        };
        assert_eq!(related.expect("the document is well formed"), true);
        related_origins::reset_client_for_test();

        // 3. The broker accepts the ceremony on the strength of that check.
        //    A registration is used, because an assertion needs a stored
        //    credential to find, and the point here is the origin check.
        use crate::credentials::{PasskeyBroker, RegistrationRequest};
        use crate::webauthn::types::{PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity};

        let mut broker = crate::credentials::LocalPasskeyBroker::new();
        let request = RegistrationRequest {
            task_id: "related-origins-test".to_string(),
            origin: origin.to_string(),
            rp: PublicKeyCredentialRpEntity {
                id: rp_id.to_string(),
                name: Some(rp_id.to_string()),
            },
            user: PublicKeyCredentialUserEntity {
                id: b"user".to_vec(),
                name: "user@example.com".to_string(),
                display_name: "user@example.com".to_string(),
            },
            challenge: b"challenge".to_vec(),
            pub_key_cred_params: Vec::new(),
            exclude_credentials: Vec::new(),
            authenticator_selection: None,
            attestation: None,
            extensions: None,
            cross_origin: false,
            related_origin_validated: true,
        };
        assert!(
            broker.begin_registration(request).is_ok(),
            "the broker must accept the ceremony once the related origins check has been made"
        );
    }

    #[test]
    fn the_broker_refuses_an_origin_without_the_related_origins_check() {
        // The other half of the security property: a caller that has not run
        // the related origins procedure cannot talk the broker into accepting
        // an origin the relying party has not claimed.
        use crate::credentials::{PasskeyBroker, RegistrationRequest};
        use crate::webauthn::types::{PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity};

        let mut broker = crate::credentials::LocalPasskeyBroker::new();
        let request = RegistrationRequest {
            task_id: "related-origins-test".to_string(),
            origin: "https://caller.example".to_string(),
            rp: PublicKeyCredentialRpEntity {
                id: "rp.example".to_string(),
                name: Some("rp.example".to_string()),
            },
            user: PublicKeyCredentialUserEntity {
                id: b"user".to_vec(),
                name: "user@example.com".to_string(),
                display_name: "user@example.com".to_string(),
            },
            challenge: b"challenge".to_vec(),
            pub_key_cred_params: Vec::new(),
            exclude_credentials: Vec::new(),
            authenticator_selection: None,
            attestation: None,
            extensions: None,
            cross_origin: false,
            related_origin_validated: false,
        };
        let error = broker
            .begin_registration(request)
            .expect_err("the broker must refuse an origin it has not checked");
        assert!(
            error.to_string().contains("not related"),
            "unexpected error: {error}"
        );
    }
}
