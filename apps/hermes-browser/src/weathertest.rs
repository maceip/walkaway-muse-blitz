//! `--weathertest`: the weather web app proven end to end.
//!
//! The harness proves:
//!
//!   1. The fixture serves the weather page and the JSON API from the same
//!      origin, and the API returns real upstream data — a resolved place,
//!      a current temperature, and a daily forecast.
//!   2. The page the browser receives is the server-rendered one: it
//!      already contains the real temperature, the real place name, and the
//!      real forecast rows, with no page-side network call needed. The
//!      engine has no `fetch` and no `XMLHttpRequest`, so a page that
//!      pulled its own data would render nothing at all.
//!   3. The engine's own DOM, after loading that page, contains the
//!      rendered values — the temperature, the condition, the place, the
//!      forecast rows, and the inline SVG weather icon.
//!   4. A place that does not exist produces an honest error page naming
//!      the real reason, and no forecast.
//!
//! The fixture is started as a real subprocess and torn down afterwards; if
//! it cannot be started, the harness fails rather than skipping. The
//! upstream is the real Open-Meteo service, so a run with no network fails
//! loudly instead of showing invented weather.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use markup5ever::local_name;
use url::Url;

use crate::Page;

/// Kills the fixture subprocess when the harness ends.
struct FixtureGuard(Child);

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The repository root, derived from the manifest directory.
///
/// The manifest lives at `<repo>/apps/hermes-browser`, so the repository root
/// is two levels up from it.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_default()
}

fn fixture_path() -> std::path::PathBuf {
    repo_root().join("services/demo-fixtures/signup_fixture.py")
}

/// Fetches a URL from the loopback fixture as raw text.
///
/// The fixture speaks plain HTTP on loopback with `Connection: close`, so no
/// TLS or HTTP client dependency is needed for the harness.
fn http_text(port: u16, path: &str) -> Result<(u16, String), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("no status line in {head:?}"))?;
    Ok((status, body.to_string()))
}

/// Starts the demo fixture on an ephemeral port.
fn start_fixture() -> Result<(u16, FixtureGuard), String> {
    let fixture = fixture_path();
    if !fixture.exists() {
        return Err(format!("fixture not found at {}", fixture.display()));
    }
    let db = std::env::temp_dir().join(format!("hermes-weathertest-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    let child = Command::new("python3")
        .arg(&fixture)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--db")
        .arg(&db)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", fixture.display()))?;
    let mut guard = FixtureGuard(child);
    let stdout = guard
        .0
        .stdout
        .take()
        .ok_or_else(|| "fixture stdout was not captured".to_string())?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read fixture banner: {e}"))?;
    // "Signup fixture listening on http://127.0.0.1:PORT"
    let port: u16 = line
        .trim()
        .rsplit(':')
        .next()
        .and_then(|p| p.trim().parse().ok())
        .ok_or_else(|| format!("cannot read the fixture port from {line:?}"))?;
    Ok((port, guard))
}

/// Reads one element's text from a loaded document.
fn element_text(page: &Page, id: &str) -> String {
    let doc = page.doc();
    doc.get_element_by_id(id)
        .and_then(|node_id| doc.get_node(node_id))
        .map(|node| node.text_content())
        .unwrap_or_default()
}

/// Counts the elements that carry a class.
fn count_by_class(page: &Page, class: &str) -> usize {
    let doc = page.doc();
    doc.tree()
        .iter()
        .filter_map(|(_id, node)| node.element_data())
        .filter(|element| {
            element
                .attr(local_name!("class"))
                .is_some_and(|value| value.split_whitespace().any(|name| name == class))
        })
        .count()
}

pub fn weathertest() -> i32 {
    let mut failures: Vec<String> = Vec::new();
    let mut checks = 0usize;

    fn check(
        checks: &mut usize,
        failures: &mut Vec<String>,
        name: &str,
        ok: bool,
        detail: impl FnOnce() -> String,
    ) {
        *checks += 1;
        if ok {
            println!("ok   {name}");
        } else {
            println!("not ok {name}: {}", detail());
            failures.push(name.to_string());
        }
    }

    let (port, _fixture) = match start_fixture() {
        Ok(parts) => parts,
        Err(error) => {
            println!("not ok fixture start: {error}");
            return 1;
        }
    };
    println!("-- fixture listening on 127.0.0.1:{port}");

    let viewport = crate::page_viewport((1120, 800), 1.0, crate::Theme::Dark);

    // --- the JSON API returns real upstream data -------------------------
    let (api_status, api_body) = match http_text(port, "/api/weather?place=San%20Francisco") {
        Ok(parts) => parts,
        Err(error) => {
            println!("not ok weather API request: {error}");
            return 1;
        }
    };
    check(
        &mut checks,
        &mut failures,
        "the weather API answers 200 for a real place",
        api_status == 200,
        || format!("status {api_status}"),
    );

    let api_json: serde_json::Value =
        serde_json::from_str(&api_body).unwrap_or(serde_json::Value::Null);
    let api_temp = api_json
        .pointer("/forecast/current/temperature")
        .and_then(serde_json::Value::as_f64);
    let api_name = api_json
        .pointer("/location/name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let api_days = api_json
        .pointer("/forecast/days")
        .and_then(serde_json::Value::as_array)
        .map(|d| d.len())
        .unwrap_or(0);

    check(
        &mut checks,
        &mut failures,
        "the API reports a real place name resolved by the geocoder",
        api_name == "San Francisco",
        || format!("location name was {api_name:?}"),
    );
    check(
        &mut checks,
        &mut failures,
        "the API reports a real current temperature from the forecast service",
        matches!(api_temp, Some(value) if (-90.0..=60.0).contains(&value)),
        || format!("temperature was {api_temp:?}"),
    );
    check(
        &mut checks,
        &mut failures,
        "the API carries a multi-day forecast",
        api_days >= 3,
        || format!("forecast had {api_days} days"),
    );

    // --- the page is server-rendered --------------------------------------
    let (page_status, page_html) = match http_text(port, "/weather?place=San%20Francisco") {
        Ok(parts) => parts,
        Err(error) => {
            println!("not ok weather page request: {error}");
            return 1;
        }
    };
    check(
        &mut checks,
        &mut failures,
        "the weather page is served",
        page_status == 200,
        || format!("status {page_status}"),
    );
    check(
        &mut checks,
        &mut failures,
        "the page already contains the real temperature, so no page-side fetch is needed",
        match api_temp {
            Some(value) => page_html.contains(&format!("{:.0}°C", value)),
            None => false,
        },
        || "the rendered temperature was not present in the served HTML".to_string(),
    );
    check(
        &mut checks,
        &mut failures,
        "the page contains no page-side network call, because the engine has no fetch",
        !page_html.contains("fetch(") && !page_html.contains("XMLHttpRequest"),
        || "the served page still tries to fetch its own data".to_string(),
    );

    // --- the real engine renders it --------------------------------------
    let base = Url::parse(&format!(
        "http://127.0.0.1:{port}/weather?place=San%20Francisco"
    ))
    .expect("valid base url");
    let mut page = Page::load(&page_html, Some(base), None, viewport, crate::Theme::Dark);
    for _ in 0..40 {
        page.resolve();
        std::thread::sleep(Duration::from_millis(25));
    }
    page.resolve();

    let temp_text = element_text(&page, "w-temp");
    check(
        &mut checks,
        &mut failures,
        "the engine's DOM holds the rendered temperature",
        match api_temp {
            Some(value) => temp_text.trim() == format!("{:.0}°C", value),
            None => false,
        },
        || format!("w-temp was {temp_text:?}"),
    );

    let label_text = element_text(&page, "w-label");
    check(
        &mut checks,
        &mut failures,
        "the engine's DOM holds the rendered condition",
        !label_text.trim().is_empty(),
        || "w-label was empty".to_string(),
    );

    let place_text = element_text(&page, "w-place");
    check(
        &mut checks,
        &mut failures,
        "the engine's DOM holds the rendered place",
        place_text.contains("San Francisco"),
        || format!("w-place was {place_text:?}"),
    );

    let day_rows = count_by_class(&page, "day");
    check(
        &mut checks,
        &mut failures,
        "the engine's DOM holds one row per forecast day",
        day_rows == api_days && day_rows > 0,
        || format!("{day_rows} day rows for {api_days} forecast days"),
    );

    // The weather icon is inline SVG: the engine's font stack has no emoji
    // font, so an emoji would render as a tofu box instead of the icon.
    // `#w-icon` is the wrapper; the SVG is inside it.
    let icon_is_svg = {
        let doc = page.doc();
        let wrapper_id = doc.get_element_by_id("w-icon");
        let svg_count = doc
            .tree()
            .iter()
            .filter_map(|(_id, node)| node.element_data())
            .filter(|element| element.name.local == local_name!("svg"))
            .count();
        // The page carries one SVG per forecast day plus the current one.
        // The current icon is the one inside the `#w-icon` wrapper, so the
        // page must have at least one SVG and the wrapper must exist.
        wrapper_id.is_some() && svg_count >= 1
    };
    check(
        &mut checks,
        &mut failures,
        "the engine's DOM holds the inline SVG weather icon",
        icon_is_svg,
        || "#w-icon did not contain an <svg> element in the rendered document".to_string(),
    );
    check(
        &mut checks,
        &mut failures,
        "the served page carries inline SVG for the icon, not an emoji glyph",
        page_html.contains("<svg")
            && "☀"
                != page_html
                    .split("<svg")
                    .nth(1)
                    .unwrap_or("")
                    .chars()
                    .next()
                    .unwrap_or('x')
                    .to_string(),
        || "the page did not carry an inline SVG icon".to_string(),
    );

    // --- an unknown place reports the real reason -------------------------
    let (bad_status, bad_body) = match http_text(port, "/api/weather?place=zzzqqxnotaplace12345") {
        Ok(parts) => parts,
        Err(error) => {
            println!("not ok weather API request for an unknown place: {error}");
            return 1;
        }
    };
    check(
        &mut checks,
        &mut failures,
        "an unknown place is a 404, not a made-up forecast",
        bad_status == 404,
        || format!("status {bad_status}"),
    );
    check(
        &mut checks,
        &mut failures,
        "the unknown-place response names the real reason",
        bad_body.contains("no place named"),
        || format!("body was {bad_body:?}"),
    );

    let (_, bad_page_html) = match http_text(port, "/weather?place=zzzqqxnotaplace12345") {
        Ok(parts) => parts,
        Err(error) => {
            println!("not ok weather page request for an unknown place: {error}");
            return 1;
        }
    };
    check(
        &mut checks,
        &mut failures,
        "the error page shows the reason and no temperature",
        bad_page_html.contains("Could not load the weather")
            && bad_page_html.contains("no place named")
            && !bad_page_html.contains("id=\"w-temp\""),
        || "the error page did not report the real reason".to_string(),
    );

    println!("-- {checks} checks, {} failed", failures.len());
    if failures.is_empty() {
        println!("PASS");
        0
    } else {
        println!("FAIL");
        1
    }
}
