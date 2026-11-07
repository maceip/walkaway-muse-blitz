//! Navigation state: URL parsing, history, and the fetch that backs the
//! address bar.
//!
//! Fetching happens on a worker thread; the event loop stays responsive and
//! receives the result as a user event. Nothing here is simulated — a
//! navigation either produces real HTML from the network or a real error.

use std::sync::Arc;
use url::Url;

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

    let looks_like_host = !trimmed.contains(char::is_whitespace)
        && (trimmed.contains('.') || trimmed.starts_with("localhost"));

    if looks_like_host {
        let candidate = format!("https://{trimmed}");
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
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Enter the runtime context so document construction may spawn fetch tasks.
pub fn enter_runtime() -> tokio::runtime::EnterGuard<'static> {
    runtime().enter()
}

/// Blocking fetch used by the worker thread and by `--url` renders.
pub fn fetch(provider: &SharedProvider, url: &Url) -> Result<String, String> {
    let request = blitz_traits::net::Request::get(url.clone());
    match runtime().block_on(provider.fetch_async(request)) {
        Ok((_content_type, bytes)) => {
            if bytes.is_empty() {
                return Err("empty response".to_string());
            }
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// Blocking HTTP form submission (`GET` or `POST` `application/x-www-form-urlencoded`).
pub fn submit_form(
    provider: &SharedProvider,
    url: &Url,
    method: &str,
    entries: &[(String, String)],
) -> Result<(Url, String), String> {
    let mut target_url = url.clone();
    let request = if method.eq_ignore_ascii_case("POST") {
        let mut req = blitz_traits::net::Request::get(target_url.clone());
        req.method = blitz_traits::net::Method::POST;
        req.content_type = Some("application/x-www-form-urlencoded".to_string());
        let form_data = blitz_traits::net::FormData(
            entries
                .iter()
                .map(|(k, v)| blitz_traits::net::Entry {
                    name: k.clone(),
                    value: blitz_traits::net::EntryValue::String(v.clone()),
                })
                .collect(),
        );
        req.body = blitz_traits::net::Body::Form(form_data);
        req
    } else {
        {
            let mut qp = target_url.query_pairs_mut();
            for (k, v) in entries {
                qp.append_pair(k, v);
            }
        }
        blitz_traits::net::Request::get(target_url.clone())
    };
    match runtime().block_on(provider.fetch_async(request)) {
        Ok((final_url_str, bytes)) => {
            if bytes.is_empty() {
                return Err("empty response".to_string());
            }
            let final_url = Url::parse(&final_url_str).unwrap_or(target_url);
            let raw_body = String::from_utf8_lossy(&bytes).into_owned();
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
        Err(e) => Err(format!("{e}")),
    }
}
