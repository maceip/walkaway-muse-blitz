//! Navigation state: URL parsing, history, and the fetch that backs the
//! address bar.
//!
//! Fetching happens on a worker thread; the event loop stays responsive and
//! receives the result as a user event. Nothing here is simulated — a
//! navigation either produces real HTML from the network or a real error.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub host_only: bool,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub expires_unix: Option<i64>,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn cookie_jar_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("HERMES_COOKIE_JAR_PATH") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return Some(std::path::PathBuf::from(trimmed));
        }
    }
    let home = std::env::var("HOME").ok()?;
    let dir = std::path::PathBuf::from(home).join(".walkaway-hermes");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("cookies.json"))
}

fn cookie_store() -> &'static Mutex<Vec<CookieEntry>> {
    static JAR: OnceLock<Mutex<Vec<CookieEntry>>> = OnceLock::new();
    JAR.get_or_init(|| {
        let initial = cookie_jar_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Vec<CookieEntry>>(&s).ok())
            .unwrap_or_default();
        Mutex::new(initial)
    })
}

fn persist_cookie_jar(entries: &[CookieEntry]) {
    if let Some(path) = cookie_jar_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Only persist cookies that haven't expired; session cookies are kept in the file
        // so warm/restarted browser sessions within a campaign retain login state.
        let now = now_unix();
        let active: Vec<&CookieEntry> = entries
            .iter()
            .filter(|c| c.expires_unix.is_none_or(|exp| exp > now))
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&active) {
            let _ = std::fs::write(path, json);
        }
    }
}

pub fn clear_cookies() {
    if let Ok(mut jar) = cookie_store().lock() {
        jar.clear();
    }
    if let Ok(p) = std::env::var("HERMES_COOKIE_JAR_PATH") {
        if !p.trim().is_empty() {
            let _ = std::fs::remove_file(p.trim());
        }
    }
}

fn default_cookie_path(url_path: &str) -> String {
    if !url_path.starts_with('/') || url_path == "/" {
        return "/".to_string();
    }
    match url_path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(idx) => url_path[..idx].to_string(),
    }
}

fn domain_matches(request_host: &str, cookie_domain: &str, host_only: bool) -> bool {
    let req = request_host.to_ascii_lowercase();
    let dom = cookie_domain.to_ascii_lowercase();
    if req == dom {
        return true;
    }
    if host_only {
        return false;
    }
    req.ends_with(&format!(".{dom}"))
}

fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    let req = if request_path.is_empty() { "/" } else { request_path };
    if req == cookie_path {
        return true;
    }
    if req.starts_with(cookie_path) {
        if cookie_path.ends_with('/') {
            return true;
        }
        if req.as_bytes().get(cookie_path.len()) == Some(&b'/') {
            return true;
        }
    }
    false
}

/// Parse and store a `Set-Cookie` header (or `document.cookie = ...` assignment when `from_js` is true).
pub fn store_set_cookie(request_url: &Url, raw_cookie: &str, from_js: bool) {
    let Some(req_host) = request_url.host_str() else {
        return;
    };
    let req_host_lc = req_host.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    let mut parts = raw_cookie.split(';');
    let Some(name_val) = parts.next() else {
        return;
    };
    let Some((raw_name, raw_val)) = name_val.split_once('=') else {
        return;
    };
    let name = raw_name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let value = raw_val.trim().to_string();

    let mut domain_attr: Option<String> = None;
    let mut path_attr: Option<String> = None;
    let mut secure = false;
    let mut http_only = false;
    let mut same_site: Option<String> = None;
    let mut max_age: Option<i64> = None;
    let mut expires_gone = false;

    for attr in parts {
        let attr = attr.trim();
        if attr.is_empty() {
            continue;
        }
        if let Some((k, v)) = attr.split_once('=') {
            let k_lc = k.trim().to_ascii_lowercase();
            let v_trim = v.trim();
            match k_lc.as_str() {
                "domain" => {
                    let clean = v_trim.trim_start_matches('.').to_ascii_lowercase();
                    if !clean.is_empty() {
                        domain_attr = Some(clean);
                    }
                }
                "path" => {
                    if v_trim.starts_with('/') {
                        path_attr = Some(v_trim.to_string());
                    }
                }
                "max-age" => {
                    if let Ok(secs) = v_trim.parse::<i64>() {
                        max_age = Some(secs);
                    }
                }
                "expires" => {
                    // Treat explicit Epoch/1970 expires as deletion when Max-Age is absent.
                    if v_trim.contains("1970") || v_trim.contains(" 00:00:00") && v_trim.contains("19") {
                        expires_gone = true;
                    }
                }
                "samesite" => {
                    same_site = Some(v_trim.to_ascii_lowercase());
                }
                _ => {}
            }
        } else {
            match attr.to_ascii_lowercase().as_str() {
                "secure" => secure = true,
                "httponly" => http_only = true,
                _ => {}
            }
        }
    }

    if from_js && http_only {
        // Non-HTTP APIs (document.cookie) may not set HttpOnly cookies.
        return;
    }

    let (domain, host_only) = match domain_attr {
        Some(d) => {
            if !domain_matches(&req_host_lc, &d, false) {
                // Reject Set-Cookie for a foreign domain.
                return;
            }
            (d, false)
        }
        None => (req_host_lc.clone(), true),
    };
    let path = path_attr.unwrap_or_else(|| default_cookie_path(request_url.path()));
    let now = now_unix();
    let expires_unix = if let Some(ma) = max_age {
        Some(now + ma)
    } else if expires_gone {
        Some(0)
    } else {
        None
    };

    let Ok(mut jar) = cookie_store().lock() else {
        return;
    };
    if from_js {
        // Non-HTTP APIs cannot overwrite an existing HttpOnly cookie.
        if jar
            .iter()
            .any(|c| c.name == name && c.domain == domain && c.path == path && c.http_only)
        {
            return;
        }
    }
    jar.retain(|c| !(c.name == name && c.domain == domain && c.path == path));
    if expires_unix.is_none_or(|exp| exp > now) {
        jar.push(CookieEntry {
            name,
            value,
            domain,
            host_only,
            path,
            secure,
            http_only,
            same_site,
            expires_unix,
        });
    }
    persist_cookie_jar(&jar);
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_matches(|c| c == '[' || c == ']');
    h.eq_ignore_ascii_case("localhost")
        || h == "::1"
        || h.parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// Format matching cookies as `"k1=v1; k2=v2"` for an outgoing HTTP request (`for_js = false`)
/// or for `document.cookie` (`for_js = true`, which omits `HttpOnly` cookies).
pub fn cookie_header_for_url(url: &Url, for_js: bool) -> String {
    let Some(host) = url.host_str() else {
        return String::new();
    };
    let host_lc = host.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    let is_secure_transport = url.scheme() == "https" || is_loopback_host(host);
    let now = now_unix();
    let Ok(mut jar) = cookie_store().lock() else {
        return String::new();
    };
    let before_len = jar.len();
    jar.retain(|c| c.expires_unix.is_none_or(|exp| exp > now));
    if jar.len() != before_len {
        persist_cookie_jar(&jar);
    }
    let mut matched: Vec<&CookieEntry> = jar
        .iter()
        .filter(|c| {
            if for_js && c.http_only {
                return false;
            }
            if c.secure && !is_secure_transport {
                return false;
            }
            domain_matches(&host_lc, &c.domain, c.host_only) && path_matches(url.path(), &c.path)
        })
        .collect();
    // RFC 6265 §5.4: cookies with longer paths are listed before shorter paths.
    matched.sort_by(|a, b| b.path.len().cmp(&a.path.len()));
    matched
        .into_iter()
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

pub struct HistoryEntry {
    pub url: Url,
    pub html: String,
}

pub struct Browser {
    pub history: Vec<HistoryEntry>,
    /// Index into `history` of the document currently displayed. Only
    /// meaningful when `history` is non-empty.
    pub index: usize,
    /// Text shown in the address bar. While the bar has focus this is the
    /// user's edit buffer, not the committed URL.
    pub address: String,
    pub focused: bool,
    /// When true, the first character typed or pasted into the focused address
    /// bar replaces the entire buffer (like clicking an address bar or pressing
    /// Cmd+L / Cmd+A in a desktop browser).
    pub address_select_all: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// Monotonic counter used to discard results from superseded navigations.
    pub generation: u64,
}

impl Browser {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            index: 0,
            address: String::new(),
            focused: false,
            address_select_all: false,
            loading: false,
            error: None,
            generation: 0,
        }
    }

    pub fn focus_address_bar(&mut self) {
        let url = self.display_url();
        self.focused = true;
        self.address = url;
        self.address_select_all = true;
    }

    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty() && self.index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        !self.history.is_empty() && self.index + 1 < self.history.len()
    }

    pub fn current(&self) -> Option<&HistoryEntry> {
        self.history.get(self.index)
    }

    /// URL text for display. Falls back to the edit buffer when the user is
    /// typing, and to a placeholder on a fresh start page.
    pub fn display_url(&self) -> String {
        if self.focused {
            return self.address.clone();
        }
        self.current()
            .map(|e| e.url.to_string())
            .unwrap_or_else(|| self.address.clone())
    }

    #[allow(dead_code)]
    pub fn address_display(&self) -> String {
        let url = self.display_url();
        if url.starts_with("muse://") {
            String::new()
        } else {
            url
        }
    }

    pub fn stop_loading(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        self.error = None;
        self.sync_address();
    }

    /// Begin a navigation: validates the input, records the generation, and
    /// returns the URL to fetch. The caller performs the fetch off-thread.
    pub fn begin_navigation(&mut self, input: &str) -> Result<Url, String> {
        let url = normalize_input(input)?;
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.error = None;
        self.focused = false;
        self.address_select_all = false;
        self.address = url.to_string();
        Ok(url)
    }

    /// Commit a successful fetch. Truncates any forward history, as a browser
    /// does when you navigate from a page you had gone back to.
    pub fn commit(&mut self, url: Url, html: String) {
        self.history.truncate(self.index + 1);
        // Replacing the entry when reloading rather than appending a duplicate.
        let is_reload = self
            .history
            .get(self.index)
            .map(|e| e.url == url)
            .unwrap_or(false);
        if is_reload {
            self.history[self.index] = HistoryEntry { url, html };
        } else {
            self.history.push(HistoryEntry { url, html });
            self.index = self.history.len() - 1;
        }
        self.loading = false;
        self.error = None;
        self.address_select_all = false;
        self.address = self
            .history
            .get(self.index)
            .map(|e| e.url.to_string())
            .unwrap_or_default();
    }

    pub fn fail(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub fn go_back(&mut self) -> bool {
        if !self.can_go_back() {
            return false;
        }
        self.index -= 1;
        self.sync_address();
        true
    }

    pub fn go_forward(&mut self) -> bool {
        if !self.can_go_forward() {
            return false;
        }
        self.index += 1;
        self.sync_address();
        true
    }

    pub fn sync_address(&mut self) {
        self.address_select_all = false;
        self.address = self
            .history
            .get(self.index)
            .map(|e| e.url.to_string())
            .unwrap_or_default();
    }
}

/// Turn address-bar text into a URL.
///
/// Accepted forms: absolute URLs, bare hosts (`example.com`, `localhost:8080`),
/// and anything else as a DuckDuckGo search — the same disambiguation a browser
/// performs. Returns an error for input that can be neither.
pub fn normalize_input(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty address".to_string());
    }

    if trimmed.contains("://") {
        return Url::parse(trimmed).map_err(|e| format!("invalid URL: {e}"));
    }

    let authority = trimmed.split('/').next().unwrap_or(trimmed);
    let looks_like_host = !trimmed.contains(char::is_whitespace)
        && (authority.contains('.')
            || authority.starts_with("localhost")
            || authority.starts_with('['));

    if looks_like_host {
        let secure_candidate = Url::parse(&format!("https://{trimmed}"));
        let loopback = secure_candidate
            .as_ref()
            .ok()
            .and_then(Url::host_str)
            .is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host.eq_ignore_ascii_case("[::1]")
                    || host.eq_ignore_ascii_case("::1")
                    || host
                        .parse::<std::net::Ipv4Addr>()
                        .is_ok_and(|address| address.is_loopback())
            });
        let candidate = format!("{}://{trimmed}", if loopback { "http" } else { "https" });
        if let Ok(url) = Url::parse(&candidate) {
            if url.host().is_some() {
                return Ok(url);
            }
        }
    }

    let mut search = Url::parse("https://duckduckgo.com/html/").expect("static url");
    search.query_pairs_mut().append_pair("q", trimmed);
    Ok(search)
}

/// Returns true if `url` targets a known third-party ad/tracker/beacon domain
/// that wastes network worker threads and adds layout jitter without contributing
/// to document structure.
pub fn is_blocked_tracker_url(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host_lc = host.to_ascii_lowercase();
    const BLOCKED_SUFFIXES: &[&str] = &[
        "doubleclick.net",
        "googlesyndication.com",
        "google-analytics.com",
        "googletagmanager.com",
        "googletagservices.com",
        "connect.facebook.net",
        "scorecardresearch.com",
        "adnxs.com",
        "criteo.com",
        "taboola.com",
        "outbrain.com",
        "hotjar.com",
        "clarity.ms",
        "quantserve.com",
        "rubiconproject.com",
    ];
    BLOCKED_SUFFIXES
        .iter()
        .any(|suffix| host_lc == *suffix || host_lc.ends_with(&format!(".{suffix}")))
}

/// Returns true if `url` resolves to a loopback, RFC1918 private, link-local,
/// or cloud-metadata IP/host (used when `policy.allow_local_network == false`).
pub fn is_local_or_private_url(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host_lc = host.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    if host_lc == "localhost"
        || host_lc.ends_with(".localhost")
        || host_lc.ends_with(".local")
        || host_lc == "::1"
    {
        return true;
    }
    if let Ok(ipv4) = host_lc.parse::<std::net::Ipv4Addr>() {
        return ipv4.is_loopback()
            || ipv4.is_private()
            || ipv4.is_link_local()
            || ipv4.is_unspecified()
            || ipv4.octets()[0] == 100 && (ipv4.octets()[1] & 0b1100_0000) == 64;
    }
    if let Ok(ipv6) = host_lc.parse::<std::net::Ipv6Addr>() {
        return ipv6.is_loopback()
            || ipv6.is_unspecified()
            || (ipv6.segments()[0] & 0xfe00) == 0xfc00
            || (ipv6.segments()[0] & 0xffc0) == 0xfe80;
    }
    false
}

/// Resolve a link target found inside a page against that page's URL.
pub fn resolve_link(base: &Url, href: &str) -> Result<Url, String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return Err("anchor-only link".to_string());
    }
    base.join(href).map_err(|e| format!("bad link: {e}"))
}

pub type SharedProvider = Arc<blitz_net::Provider>;

/// reqwest needs a Tokio reactor, and blitz-net's subresource loader calls
/// `tokio::spawn` from whichever thread parses the document. A multi-threaded
/// runtime with worker threads is therefore required: tasks spawned during
/// parsing must keep running after the constructor returns, and their
/// completions arrive through the NetWaker.
///
/// Scaled to 8+ worker threads so concurrent multi-tab agent sessions
/// (e.g., LinkedIn + Expedia + Zappos loading HTML, CSS, and images in parallel)
/// never serialize behind a 2-thread bottleneck.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().clamp(8, 16))
            .unwrap_or(8);
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(threads)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Enter the runtime context so document construction may spawn fetch tasks.
pub fn enter_runtime() -> tokio::runtime::EnterGuard<'static> {
    runtime().enter()
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Walkaway-Hermes-Blitz/0.1")
            .build()
            .expect("browser http client")
    })
}

#[derive(Clone, Debug)]
pub struct HttpExchange {
    pub final_url: Url,
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

async fn execute_http_async(
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    body: Option<Vec<u8>>,
) -> Result<HttpExchange, String> {
    if is_blocked_tracker_url(url) {
        return Err("blocked tracker url".to_string());
    }
    let mut cur_url = url.clone();
    let mut cur_method = method.to_ascii_uppercase();
    let mut cur_body = body;
    let mut cur_headers: Vec<(String, String)> = headers.to_vec();

    for _hop in 0..=10 {
        let req_method = reqwest::Method::from_bytes(cur_method.as_bytes())
            .unwrap_or(reqwest::Method::GET);
        let mut builder = http_client().request(req_method, cur_url.clone());

        let cookie_hdr = cookie_header_for_url(&cur_url, false);
        if !cookie_hdr.is_empty() {
            builder = builder.header(reqwest::header::COOKIE, cookie_hdr);
        }
        for (k, v) in &cur_headers {
            if !k.eq_ignore_ascii_case("cookie") {
                builder = builder.header(k.as_str(), v.as_str());
            }
        }
        if let Some(ref payload) = cur_body {
            builder = builder.body(payload.clone());
        }

        let resp = builder.send().await.map_err(|e| format!("{e}"))?;
        let status = resp.status();

        for val in resp.headers().get_all(reqwest::header::SET_COOKIE).iter() {
            if let Ok(s) = val.to_str() {
                store_set_cookie(&cur_url, s, false);
            }
        }

        if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
            if let Some(loc) = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            {
                let next_url = cur_url
                    .join(loc.trim())
                    .map_err(|e| format!("bad redirect location: {e}"))?;
                if matches!(status.as_u16(), 301 | 302 | 303) && cur_method == "POST" {
                    cur_method = "GET".to_string();
                    cur_body = None;
                    cur_headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));
                }
                cur_url = next_url;
                continue;
            }
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let mut resp_headers = Vec::new();
        for (k, v) in resp.headers().iter() {
            if let Ok(vs) = v.to_str() {
                resp_headers.push((k.as_str().to_string(), vs.to_string()));
            }
        }
        let bytes = resp.bytes().await.map_err(|e| format!("{e}"))?.to_vec();
        return Ok(HttpExchange {
            final_url: cur_url,
            status: status.as_u16(),
            content_type,
            headers: resp_headers,
            body: bytes,
        });
    }
    Err("too many redirects".to_string())
}

/// Blocking HTTP request with RFC 6265 cookie jar and 10-hop redirect handling.
pub fn execute_http(
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    body: Option<Vec<u8>>,
) -> Result<HttpExchange, String> {
    runtime().block_on(execute_http_async(method, url, headers, body))
}

/// Blocking fetch that returns both the final post-redirect `Url` and the response HTML.
pub fn fetch_with_final_url(provider: &SharedProvider, url: &Url) -> Result<(Url, String), String> {
    if is_blocked_tracker_url(url) {
        return Err("blocked tracker url".to_string());
    }
    if matches!(url.scheme(), "http" | "https") {
        let exchange = execute_http("GET", url, &[], None)?;
        if exchange.body.is_empty() {
            return Err("empty response".to_string());
        }
        return Ok((
            exchange.final_url,
            String::from_utf8_lossy(&exchange.body).into_owned(),
        ));
    }
    let request = blitz_traits::net::Request::get(url.clone());
    match runtime().block_on(provider.fetch_async(request)) {
        Ok((_content_type, bytes)) => {
            if bytes.is_empty() {
                return Err("empty response".to_string());
            }
            Ok((url.clone(), String::from_utf8_lossy(&bytes).into_owned()))
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// Blocking fetch used by the worker thread and by `--url` renders.
pub fn fetch(provider: &SharedProvider, url: &Url) -> Result<String, String> {
    fetch_with_final_url(provider, url).map(|(_, body)| body)
}

#[allow(dead_code)]
pub fn fetch_cancellable(
    provider: &SharedProvider,
    url: &Url,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;
    if cancelled.load(Ordering::Acquire) {
        return Err("navigation cancelled".into());
    }
    if is_blocked_tracker_url(url) {
        return Err("blocked tracker url".into());
    }
    let use_http = matches!(url.scheme(), "http" | "https");
    runtime().block_on(async {
        let started = std::time::Instant::now();
        let max_wait = std::time::Duration::from_secs(60);
        if use_http {
            let fut = execute_http_async("GET", url, &[], None);
            tokio::pin!(fut);
            loop {
                if cancelled.load(Ordering::Acquire) {
                    return Err("navigation cancelled".into());
                }
                if started.elapsed() >= max_wait {
                    return Err("navigation timed out after 60 seconds".into());
                }
                if let Ok(result) =
                    tokio::time::timeout(std::time::Duration::from_millis(20), &mut fut).await
                {
                    return result.and_then(|ex| {
                        if ex.body.is_empty() {
                            Err("empty response".into())
                        } else {
                            Ok(String::from_utf8_lossy(&ex.body).into_owned())
                        }
                    });
                }
            }
        } else {
            let request = blitz_traits::net::Request::get(url.clone());
            let fetch = provider.fetch_async(request);
            tokio::pin!(fetch);
            loop {
                if cancelled.load(Ordering::Acquire) {
                    return Err("navigation cancelled".into());
                }
                if started.elapsed() >= max_wait {
                    return Err("navigation timed out after 60 seconds".into());
                }
                if let Ok(result) =
                    tokio::time::timeout(std::time::Duration::from_millis(20), &mut fetch).await
                {
                    return result
                        .map_err(|error| error.to_string())
                        .and_then(|(_, bytes)| {
                            if bytes.is_empty() {
                                Err("empty response".into())
                            } else {
                                Ok(String::from_utf8_lossy(&bytes).into_owned())
                            }
                        });
                }
            }
        }
    })
}

/// Blocking HTTP form submission (`GET` or `POST` `application/x-www-form-urlencoded`).
pub fn submit_form(
    _provider: &SharedProvider,
    url: &Url,
    method: &str,
    entries: &[(String, String)],
) -> Result<(Url, String), String> {
    let mut target_url = url.clone();
    let (req_method, headers, body) = if method.eq_ignore_ascii_case("POST") {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in entries {
            serializer.append_pair(k, v);
        }
        let encoded = serializer.finish().into_bytes();
        (
            "POST",
            vec![(
                "content-type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            )],
            Some(encoded),
        )
    } else {
        {
            let mut qp = target_url.query_pairs_mut();
            for (k, v) in entries {
                qp.append_pair(k, v);
            }
        }
        ("GET", Vec::new(), None)
    };
    let exchange = execute_http(req_method, &target_url, &headers, body)?;
    if exchange.body.is_empty() {
        return Err("empty response".to_string());
    }
    let final_url = exchange.final_url;
    let raw_body = String::from_utf8_lossy(&exchange.body).into_owned();
    let trimmed = raw_body.trim_start();
    let html_body = if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && !trimmed.to_ascii_lowercase().contains("<html")
    {
        let escaped = raw_body
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!(
            "<!doctype html><html><head><title>Submitted — {final_url}</title></head>\
             <body><h1>Form Submitted ({final_url})</h1>\
             <pre id=\"submitted-json\">{escaped}</pre></body></html>"
        )
    } else {
        raw_body
    };
    Ok((final_url, html_body))
}
