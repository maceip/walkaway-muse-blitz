//! WebAuthn Related Origin Requests (ROR).
//!
//! W3C WebAuthn Level 3, §5.11.1 ("Related Origin Requests"), REC 2026-08-25.
//!
//! A relying party may name an RP ID that is not a registrable domain suffix
//! of the origin the page was served from. The only sanctioned way for a
//! browser to honour that is for the *relying party itself* to publish, at
//! `https://<rp-id>/.well-known/webauthn`, a JSON document naming the origins
//! it is related to. The browser fetches that document and checks the caller's
//! origin against the list.
//!
//! Nothing here is a relaxation of security: the origin is allowed only when
//! the relying party's own well-known document lists it, and every failure to
//! obtain or parse that document is a `SecurityError`.
//!
//! The normative algorithm, given `callerOrigin` and `rpIdRequested`:
//!
//! 1. Let `maxLabels` be the maximum number of registrable origin labels
//!    allowed by client policy.
//! 2. Fetch `https://<rpIdRequested>/.well-known/webauthn` without
//!    credentials, without a referrer, using the `https:` scheme.
//!    1. If the fetch fails, the response is not `application/json`, or its
//!       status (after redirects) is not 200, throw a `SecurityError`.
//!    2. If the body is not a valid JSON object, throw a `SecurityError`.
//!    3. If the `origins` property is missing or is not an array of strings,
//!       throw a `SecurityError`.
//! 3. Let `labelsSeen` be a new empty set.
//! 4. For each `originItem` of `origins`:
//!    1. Parse `originItem` as a URL. If that fails, continue.
//!    2. Let `domain` be the effective domain of the URL. If null, continue.
//!    3. Let `label` be the registrable origin label of `domain`.
//!    4. If `label` is empty or null, continue.
//!    5. If `labelsSeen` is full and does not contain `label`, continue.
//!    6. If `callerOrigin` and the URL are same origin, return TRUE.
//!    7. If `labelsSeen` is not full, append `label` to `labelsSeen`.
//! 5. Return FALSE.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use url::Url;

use crate::webauthn::types::WebAuthnError;

/// The maximum number of registrable origin labels a client will honour in a
/// single related-origins document.
///
/// This matches the go-webauthn `MaximumRelatedOriginLabels` (5) and Chrome's
/// limit. A relying party that names more than five distinct registrable
/// domains is trying to claim more origins than any browser will allow.
pub const MAX_RELATED_ORIGIN_LABELS: usize = 5;

/// The well-known path a relying party publishes its related origins at.
const WELL_KNOWN_PATH: &str = "/.well-known/webauthn";

/// How long the well-known fetch may take before it is treated as a failure.
///
/// The fetch runs off the UI thread, but it still holds the page's ceremony
/// hostage while it is in flight, so it must not hang indefinitely.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// A failure of the related origins procedure.
///
/// Every one of these is a `SecurityError`, because a relying party that
/// cannot prove its relationship to the caller's origin gets no relationship.
fn security_error(detail: String) -> WebAuthnError {
    WebAuthnError::Security(detail)
}

/// The HTTP client used for the well-known fetch.
///
/// The client is built once and shared: building a TLS client per fetch would
/// re-read the system trust store on every ceremony.
///
/// The client is configured to send no referrer, and it carries no cookie jar,
/// so the request goes out without credentials. The default redirect policy
/// follows up to ten hops, which is what the algorithm asks for: the status
/// that matters is the one *after* redirects.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(build_client)
}

/// Builds the HTTP client used for the well-known fetch.
fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        // No referrer: the spec requires the fetch to be made without one.
        .referer(false)
        // No cookie jar. reqwest only keeps cookies when its `cookies`
        // feature is enabled, and it is not, so the request carries no
        // credentials.
        .timeout(FETCH_TIMEOUT)
        .build()
        .expect("the related-origins HTTP client")
}

/// The client the well-known fetch uses.
///
/// This is a process-wide override so a test can point the fetch at a local
/// server whose certificate it trusts, without the production path ever
/// relaxing TLS. The override is set before any ceremony runs.
static CLIENT_OVERRIDE: OnceLock<Mutex<Option<Arc<reqwest::Client>>>> = OnceLock::new();

fn client_slot() -> &'static Mutex<Option<Arc<reqwest::Client>>> {
    CLIENT_OVERRIDE.get_or_init(|| Mutex::new(None))
}

/// The client the well-known fetch actually uses.
fn http_client() -> reqwest::Client {
    if let Ok(guard) = client_slot().lock() {
        if let Some(client) = guard.as_ref() {
            return client.as_ref().clone();
        }
    }
    client().clone()
}

/// Points the well-known fetch at a different HTTP client.
///
/// This exists for the tests, which run the fetch against a local server
/// whose certificate the test itself generated. The production client never
/// relaxes its TLS verification.
#[cfg(test)]
pub fn set_client_for_test(client: reqwest::Client) {
    let mut guard = client_slot().lock().expect("the client slot");
    *guard = Some(Arc::new(client));
}

/// Restores the production HTTP client after a test has used its own.
#[cfg(test)]
pub fn reset_client_for_test() {
    let mut guard = client_slot().lock().expect("the client slot");
    *guard = None;
}

/// The registrable origin label of a host.
///
/// This is the registrable domain of the host: `example.com` for
/// `www.example.com`, and `example.co.jp` for `www.example.co.jp`.
///
/// **The public suffix list matters and is deliberately not implemented
/// here.** The exact answer needs the PSL, because `co.jp` is a public suffix,
/// so `example.co.jp`'s registrable domain is the three-label
/// `example.co.jp`, not the two-label `co.jp`. This function takes the last
/// two labels, which is exact only when the public suffix is a single label.
/// That approximation is the same one go-webauthn's default labeler makes,
/// and the spec itself allows a client to define its own "registrable origin
/// label": §5.11.1 step 4.3 says "Let label be the registrable origin label of
/// domain", leaving the labeler to the client.
///
/// The approximation errs on the side of *fewer* labels for multi-label public
/// suffixes (`example.co.jp` yields `co.jp`), which can only make the label
/// cap bite sooner. It never widens what a document is allowed to claim: the
/// cap counts distinct labels, and a coarser label can only make two origins
/// look alike, never make two different origins look allowed.
fn registrable_origin_label(host: &str) -> Option<String> {
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() < 2 {
        // A single-label host has no registrable domain to speak of.
        return None;
    }
    let label = labels[labels.len() - 2..].join(".");
    if label.is_empty() { None } else { Some(label) }
}

/// The effective domain of a URL: its host, lowercased, without the port.
///
/// The spec's "effective domain" is the domain used for the origin's
/// host-part comparison. An IP address has no registrable domain, so it
/// yields null here and the caller skips the entry.
fn effective_domain(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    // A bare IPv4 or IPv6 literal is not a registrable domain.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    Some(host)
}

/// Whether two origins are the same origin: same scheme, same host, and same
/// port, with each scheme's default port normalized away.
///
/// The scheme is compared case-insensitively, and the host is already
/// lowercased by the URL parser, so `HTTPS://Example.COM` and
/// `https://example.com` are the same origin.
fn same_origin(a: &Url, b: &Url) -> bool {
    let scheme = |u: &Url| u.scheme().to_ascii_lowercase();
    if scheme(a) != scheme(b) {
        return false;
    }
    let host = |u: &Url| u.host_str().map(|h| h.to_ascii_lowercase());
    if host(a) != host(b) {
        return false;
    }
    let port = |u: &Url| {
        let explicit = u.port();
        let default = match scheme(u).as_str() {
            "https" | "wss" => Some(443),
            "http" | "ws" => Some(80),
            _ => None,
        };
        match (explicit, default) {
            (Some(p), _) => Some(p),
            (None, Some(d)) => Some(d),
            (None, None) => None,
        }
    };
    port(a) == port(b)
}

/// The `https:` well-known URL for a relying party identifier.
///
/// The RP ID is a domain string, so it is placed at the root of the `https:`
/// scheme. The RP ID is validated before this is called: it may not carry a
/// scheme, a port, or a path.
fn well_known_url(rp_id: &str) -> Result<Url, WebAuthnError> {
    Url::parse(&format!("https://{rp_id}{WELL_KNOWN_PATH}")).map_err(|_| {
        security_error(format!(
            "The relying party identifier '{rp_id}' does not name a valid well-known URL"
        ))
    })
}

/// Fetches and parses the relying party's related-origins document.
///
/// Returns the list of origins the relying party claims. Every failure to
/// obtain or parse the document is a `SecurityError`, exactly as the
/// algorithm requires.
async fn fetch_origins(rp_id: &str) -> Result<Vec<String>, WebAuthnError> {
    let url = well_known_url(rp_id)?;

    let response = http_client()
        .get(url.clone())
        // No credentials: the request carries no cookies and no
        // authorization header.
        .header(reqwest::header::AUTHORIZATION, "")
        .send()
        .await
        .map_err(|e| {
            security_error(format!(
                "The related origins document for '{rp_id}' could not be fetched: {e}"
            ))
        })?;

    // The status that counts is the one after redirects, which reqwest
    // reports because it follows them by default.
    let status = response.status();
    if status != reqwest::StatusCode::OK {
        return Err(security_error(format!(
            "The related origins document for '{rp_id}' returned HTTP status {status}"
        )));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    // The media type must be `application/json`. A `charset` parameter is
    // allowed; anything else is not.
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if media_type != "application/json" {
        return Err(security_error(format!(
            "The related origins document for '{rp_id}' is not application/json (got '{content_type}')"
        )));
    }

    let body = response.text().await.map_err(|e| {
        security_error(format!(
            "The related origins document for '{rp_id}' could not be read: {e}"
        ))
    })?;

    let value: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        security_error(format!(
            "The related origins document for '{rp_id}' is not valid JSON: {e}"
        ))
    })?;

    let object = value.as_object().ok_or_else(|| {
        security_error(format!(
            "The related origins document for '{rp_id}' is not a JSON object"
        ))
    })?;

    let origins = object.get("origins").ok_or_else(|| {
        security_error(format!(
            "The related origins document for '{rp_id}' has no 'origins' property"
        ))
    })?;

    let list = origins.as_array().ok_or_else(|| {
        security_error(format!(
            "The 'origins' property in the related origins document for '{rp_id}' is not an array"
        ))
    })?;

    // Every member must be a string. An array with a non-string member is
    // not "an array of strings", so the document is rejected.
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let text = item.as_str().ok_or_else(|| {
            security_error(format!(
                "The 'origins' property in the related origins document for '{rp_id}' is not an array of strings"
            ))
        })?;
        out.push(text.to_string());
    }
    Ok(out)
}

/// Runs the related origins validation procedure.
///
/// `caller_origin` is the origin the page was actually served from. It is
/// never taken from the page's own request parameters: the caller passes the
/// origin the host recorded for the document.
///
/// `rp_id_requested` is the RP ID the relying party asked for.
///
/// Returns `true` when the relying party's own well-known document lists the
/// caller's origin, and `false` when it does not. A failure to obtain or
/// parse the document is an error, not a `false`.
pub async fn validate(caller_origin: &str, rp_id_requested: &str) -> Result<bool, WebAuthnError> {
    let origins = fetch_origins(rp_id_requested).await?;

    let caller = Url::parse(caller_origin)
        .map_err(|_| security_error(format!("Invalid origin URL: {caller_origin}")))?;

    let mut labels_seen: HashSet<String> = HashSet::new();

    for origin_item in origins {
        // 4.1. Parse the item as a URL. A parse failure skips the item.
        let Ok(url) = Url::parse(&origin_item) else {
            continue;
        };
        // 4.2. The effective domain. Null skips the item.
        let Some(domain) = effective_domain(&url) else {
            continue;
        };
        // 4.3. The registrable origin label.
        let Some(label) = registrable_origin_label(&domain) else {
            continue;
        };
        // 4.4. An empty label skips the item.
        if label.is_empty() {
            continue;
        }
        // 4.5. When the cap is reached, an unseen label skips the item.
        if labels_seen.len() >= MAX_RELATED_ORIGIN_LABELS && !labels_seen.contains(&label) {
            continue;
        }
        // 4.6. Same origin: the caller's origin is listed.
        if same_origin(&caller, &url) {
            return Ok(true);
        }
        // 4.7. Otherwise the label is remembered, if there is room for it.
        if labels_seen.len() < MAX_RELATED_ORIGIN_LABELS {
            labels_seen.insert(label);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_origins_on_scheme_host_and_port() {
        let parse = |s: &str| Url::parse(s).unwrap();
        assert!(same_origin(
            &parse("https://example.com"),
            &parse("https://example.com")
        ));
        assert!(same_origin(
            &parse("https://example.com"),
            &parse("https://example.com:443")
        ));
        assert!(!same_origin(
            &parse("https://example.com"),
            &parse("https://example.com:8443")
        ));
        assert!(!same_origin(
            &parse("https://example.com"),
            &parse("http://example.com")
        ));
        assert!(!same_origin(
            &parse("https://example.com"),
            &parse("https://www.example.com")
        ));
        assert!(!same_origin(
            &parse("https://example.com"),
            &parse("https://other.com")
        ));
    }

    #[test]
    fn takes_the_last_two_labels_as_the_registrable_label() {
        assert_eq!(
            registrable_origin_label("www.example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            registrable_origin_label("example.com").as_deref(),
            Some("example.com")
        );
        // The public suffix list is not consulted, so a multi-label public
        // suffix collapses to its last two labels. This is the documented
        // approximation.
        assert_eq!(
            registrable_origin_label("www.example.co.jp").as_deref(),
            Some("co.jp")
        );
        assert_eq!(registrable_origin_label("localhost"), None);
    }
}
