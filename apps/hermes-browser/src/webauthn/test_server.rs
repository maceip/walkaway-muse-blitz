//! A local HTTPS server for the related-origins tests.
//!
//! The related origins procedure fetches the relying party's well-known
//! document over `https:`. The tests need a real server that speaks TLS and
//! serves real HTTP responses, so that the fetch path they exercise is the
//! production one: a real socket, a real handshake, a real status code and
//! a real content type.
//!
//! The server listens on an ephemeral port and the test's HTTP client maps
//! the relying party's host name onto it, so the URL the fetch builds is
//! exactly the one the spec asks for: `https://<rp-id>/.well-known/webauthn`.
//!
//! Nothing here is a mock of the fetch. The server is a server.

#![cfg(test)]

use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use openssl::pkey::PKey;
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::X509;

/// The host name the test server's certificate is issued for, and the host
/// name the relying party identifier uses.
///
/// The tests address the server as `localhost`, so the certificate has to
/// carry that name, or the TLS handshake fails the hostname check.
pub const TEST_HOST: &str = "localhost";

/// What the server answers with.
#[derive(Clone, Debug)]
pub struct TestResponse {
    /// The HTTP status line's code.
    pub status: u16,
    /// The value of the `Content-Type` header.
    pub content_type: String,
    /// The response body.
    pub body: String,
    /// When true, the server answers with a redirect before the real
    /// response, so the fetch has to follow it before it sees the final
    /// status.
    pub redirect_first: bool,
}

impl TestResponse {
    /// A `200 application/json` response with the given body.
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "application/json".to_string(),
            body: body.into(),
            redirect_first: false,
        }
    }

    /// A response with an explicit status and content type.
    pub fn with(status: u16, content_type: &str, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: content_type.to_string(),
            body: body.into(),
            redirect_first: false,
        }
    }

    /// Makes the server answer with a redirect before the real response, so
    /// the fetch has to follow it to see the final status.
    pub fn redirecting(mut self) -> Self {
        self.redirect_first = true;
        self
    }
}

/// A running local HTTPS server.
pub struct TestServer {
    /// The address the server is listening on.
    pub addr: SocketAddr,
    /// The host name the server answers to, and the one the relying party
    /// identifier uses.
    pub host: String,
    /// The certificate the server presents, in DER form.
    pub cert_der: Vec<u8>,
    /// The paths the server has been asked for, in order.
    pub requests: Arc<Mutex<Vec<String>>>,
    /// Set when the server should stop accepting.
    pub stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    /// Starts a server that always answers with `response`.
    pub fn start(response: TestResponse) -> Self {
        let responder = response.clone();
        Self::start_with(move |_path| responder.clone())
    }

    /// Starts a server that answers under `host` rather than `localhost`.
    ///
    /// The host name is the one the relying party identifier uses, so the
    /// well-known URL the procedure builds is the one the test expects.
    pub fn start_for_host(host: &str, response: TestResponse) -> Self {
        let responder = response.clone();
        Self::start_with_host(host, move |_path| responder.clone())
    }

    /// Starts a server that answers with whatever `responder` returns for the
    /// requested path.
    pub fn start_with<F>(responder: F) -> Self
    where
        F: Fn(&str) -> TestResponse + Send + Sync + 'static,
    {
        Self::start_with_host(TEST_HOST, responder)
    }

    /// Starts a server that answers under `host` and answers with whatever
    /// `responder` returns for the requested path.
    pub fn start_with_host<F>(host: &str, responder: F) -> Self
    where
        F: Fn(&str) -> TestResponse + Send + Sync + 'static,
    {
        let host = host.to_string();
        let (cert, key) = self_signed_cert(&host);
        let cert_der = cert.to_der().expect("the certificate in DER form");

        let mut builder =
            SslAcceptor::mozilla_intermediate(SslMethod::tls()).expect("the TLS acceptor");
        builder.set_certificate(&cert).expect("the certificate");
        builder.set_private_key(&key).expect("the private key");
        let acceptor = Arc::new(builder.build());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("the test listener");
        let addr = listener.local_addr().expect("the test address");
        listener
            .set_nonblocking(true)
            .expect("the test listener is non-blocking");

        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_requests = requests.clone();
        let thread_stop = stop.clone();
        let responder = Arc::new(responder);
        let handle = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let acceptor = acceptor.clone();
                        let requests = thread_requests.clone();
                        let responder = responder.clone();
                        std::thread::spawn(move || {
                            let _ = serve_one(stream, acceptor, responder, requests);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        TestServer {
            addr,
            host,
            cert_der,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    /// The RP ID the tests use for this server.
    ///
    /// The RP ID is a domain, so it carries no port: the well-known URL the
    /// procedure fetches is `https://localhost/.well-known/webauthn`, and the
    /// test's client maps that host name onto this server's port.
    pub fn rp_id(&self) -> String {
        self.host.clone()
    }

    /// An HTTP client that trusts this server's certificate and maps the
    /// server's host name onto this server's port.
    pub fn client(&self) -> reqwest::Client {
        let certificate = reqwest::Certificate::from_der(&self.cert_der)
            .expect("the certificate for the test client");
        reqwest::Client::builder()
            .add_root_certificate(certificate)
            .resolve(&self.host, self.addr)
            // No referrer, and no cookie jar: the spec requires the fetch to
            // be made without credentials and without a referrer.
            .referer(false)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("the test client")
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Serves one connection.
fn serve_one<F>(
    stream: std::net::TcpStream,
    acceptor: Arc<SslAcceptor>,
    responder: Arc<F>,
    requests: Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()>
where
    F: Fn(&str) -> TestResponse,
{
    // The accepted socket inherits the listener's non-blocking mode, and the
    // TLS handshake needs a blocking socket.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    let tls = match acceptor.accept(stream) {
        Ok(tls) => tls,
        Err(error) => {
            eprintln!("[test-server] the TLS handshake failed: {error}");
            return Err(std::io::Error::other(error));
        }
    };
    let mut reader = BufReader::new(tls);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    if let Ok(mut guard) = requests.lock() {
        guard.push(path.clone());
    }

    // Drain the headers so the client sees a complete exchange.
    let mut header = String::new();
    loop {
        header.clear();
        let read = reader.read_line(&mut header)?;
        if read == 0 || header.trim().is_empty() {
            break;
        }
    }

    let response = responder(&path);
    if response.redirect_first && !path.contains("redirected=1") {
        // A redirect to the same path with a marker, so the fetch has to
        // follow it before it sees the real response. The marker keeps the
        // exchange from looping.
        let body = "redirecting";
        let header = format!(
            "HTTP/1.1 302 Found\r\nLocation: /.well-known/webauthn?redirected=1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        reader.get_mut().write_all(header.as_bytes())?;
        reader.get_mut().write_all(body.as_bytes())?;
        reader.get_mut().flush()?;
        return Ok(());
    }

    let body = response.body;
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        body.len()
    );
    reader.get_mut().write_all(header.as_bytes())?;
    reader.get_mut().write_all(body.as_bytes())?;
    reader.get_mut().flush()?;
    Ok(())
}

/// The reason phrase for a status code.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        301 => "Moved Permanently",
        302 => "Found",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

/// Builds a self-signed certificate for `localhost` and its key.
fn self_signed_cert(host: &str) -> (X509, PKey<openssl::pkey::Private>) {
    let rsa = openssl::rsa::Rsa::generate(2048).expect("the test key");
    let key = PKey::from_rsa(rsa).expect("the test key");

    let mut name = openssl::x509::X509NameBuilder::new().expect("the name builder");
    name.append_entry_by_text("CN", host)
        .expect("the common name");
    let name = name.build();

    let mut builder = X509::builder().expect("the certificate builder");
    builder.set_version(2).expect("the version");
    builder.set_subject_name(&name).expect("the subject");
    builder.set_issuer_name(&name).expect("the issuer");
    builder.set_pubkey(&key).expect("the public key");
    builder
        .set_not_before(&openssl::asn1::Asn1Time::days_from_now(0).expect("the not-before"))
        .expect("the not-before");
    builder
        .set_not_after(&openssl::asn1::Asn1Time::days_from_now(1).expect("the not-after"))
        .expect("the not-after");
    // The certificate is its own trust anchor, so it carries the basic
    // constraints extension that says it may sign certificates.
    let basic_constraints = openssl::x509::extension::BasicConstraints::new()
        .critical()
        .ca()
        .build()
        .expect("the basic constraints");
    builder
        .append_extension(basic_constraints)
        .expect("the basic constraints");
    // The certificate must be usable for TLS server authentication, or the
    // client's extended key usage check rejects it.
    let extended_key_usage = openssl::x509::extension::ExtendedKeyUsage::new()
        .critical()
        .server_auth()
        .build()
        .expect("the extended key usage");
    builder
        .append_extension(extended_key_usage)
        .expect("the extended key usage");
    // The server certificate names the host the client connects to, so the
    // client's hostname check passes against the real certificate.
    let context = builder.x509v3_context(None, None);
    let subject_alternative_name = openssl::x509::extension::SubjectAlternativeName::new()
        .dns(host)
        .build(&context)
        .expect("the subject alternative name");
    builder
        .append_extension(subject_alternative_name)
        .expect("the subject alternative name");
    builder
        .sign(&key, openssl::hash::MessageDigest::sha256())
        .expect("the signature");
    let cert = builder.build();
    (cert, key)
}
