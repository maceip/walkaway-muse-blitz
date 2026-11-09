use crate::browser::{
    Browser, is_blocked_tracker_url, is_local_or_private_url, normalize_input,
};
use url::Url;

#[test]
fn internal_start_url_is_not_presented_in_address_field() {
    let mut browser = Browser::new();
    browser.commit(Url::parse("muse://start").unwrap(), String::new());
    assert_eq!(browser.address_display(), "");
    assert_eq!(browser.display_url(), "muse://start");
    browser.commit(
        Url::parse("https://example.com/path").unwrap(),
        String::new(),
    );
    assert_eq!(browser.address_display(), "https://example.com/path");
}

#[test]
fn stop_rejects_pending_navigation_generation() {
    let mut browser = Browser::new();
    browser.commit(Url::parse("https://example.com/").unwrap(), "old".into());
    browser.begin_navigation("https://example.org/").unwrap();
    let in_flight = browser.generation;
    browser.stop_loading();
    assert_ne!(browser.generation, in_flight);
    assert!(!browser.loading);
    assert_eq!(browser.address_display(), "https://example.com/");
}

#[test]
fn cancelled_fetch_does_not_start_network_work() {
    let provider = std::sync::Arc::new(blitz_net::Provider::new(None));
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    let start = std::time::Instant::now();
    let result = crate::browser::fetch_cancellable(
        &provider,
        &Url::parse("http://127.0.0.1:1/").unwrap(),
        &cancelled,
    );
    assert_eq!(result.unwrap_err(), "navigation cancelled");
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn standard_local_hosts_and_searches_normalize() {
    assert_eq!(
        normalize_input("localhost:8795/form").unwrap().as_str(),
        "http://localhost:8795/form"
    );
    assert_eq!(
        normalize_input("127.0.0.1:8795/form").unwrap().as_str(),
        "http://127.0.0.1:8795/form"
    );
    assert_eq!(
        normalize_input("[::1]:8795/form").unwrap().as_str(),
        "http://[::1]:8795/form"
    );
    assert_eq!(
        normalize_input("example.com").unwrap().as_str(),
        "https://example.com/"
    );
    assert_eq!(
        normalize_input("localhost.evil.test").unwrap().scheme(),
        "https"
    );
    assert_eq!(
        normalize_input("127.example.com").unwrap().scheme(),
        "https"
    );
    assert_eq!(
        normalize_input("a normal search").unwrap().host_str(),
        Some("duckduckgo.com")
    );
}

#[test]
fn tracker_filter_and_ssrf_detection_work() {
    assert!(is_blocked_tracker_url(
        &Url::parse("https://stats.g.doubleclick.net/r/collect").unwrap()
    ));
    assert!(!is_blocked_tracker_url(
        &Url::parse("https://www.zappos.com/shoes").unwrap()
    ));
    assert!(is_local_or_private_url(
        &Url::parse("http://127.0.0.1:8888/admin").unwrap()
    ));
    assert!(is_local_or_private_url(
        &Url::parse("http://169.254.169.254/latest/meta-data/").unwrap()
    ));
    assert!(is_local_or_private_url(
        &Url::parse("http://localhost:3000/").unwrap()
    ));
    assert!(!is_local_or_private_url(
        &Url::parse("https://www.expedia.com/Flights").unwrap()
    ));
}
