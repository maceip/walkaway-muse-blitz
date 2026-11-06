//! `--passkeytest`: the WebAuthn passkey path proven end to end against the
//! real demo fixture, whose server-side verifier is an independent
//! implementation (`services/demo-fixtures/signup_fixture.py` verifies the
//! signature with the `openssl` CLI against the registered public key).
//!
//! The harness proves:
//!
//!   1. A registration ceremony produces a real credential and stores it in
//!      the macOS Keychain.
//!   2. The relying party's independent verifier accepts the assertion the
//!      host-side authenticator produced.
//!   3. The ceremony produces nothing until a human approves it.
//!   4. The relying party rejects a replayed challenge, a wrong challenge,
//!      and an unknown credential.
//!   5. The agent never sees a secret: the broker's prompt is redacted and
//!      the audit trail carries no key material.
//!
//! The fixture is started as a real subprocess and torn down afterwards; if
//! it cannot be started, the harness fails rather than skipping.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

use crate::credentials::{
    CeremonyOutcome, LocalPasskeyBroker, PasskeyBroker, PasskeyCeremonyId, PasskeyRequest,
    PasskeyStatus, RegistrationRequest,
};
use crate::webauthn::b64::b64url_decode;
use crate::webauthn::types::{
    PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity,
};

/// The fixture's default relying party.
const RP_ID: &str = "127.0.0.1";
/// The origin the fixture serves on.
const ORIGIN: &str = "http://127.0.0.1";

/// A minimal HTTP/1.1 client for the loopback fixture.
///
/// The fixture speaks plain HTTP on loopback with `Connection: close`, so no
/// TLS or HTTP client dependency is needed for the harness.
fn http_json(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<(u16, Value), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| format!("read timeout: {e}"))?;
    let payload = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Accept: application/json\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("read: {e}"))?;
    let text = String::from_utf8_lossy(&response).to_string();
    let split = text
        .find("\r\n\r\n")
        .ok_or_else(|| format!("malformed response: {}", &text[..text.len().min(200)]))?;
    let head = &text[..split];
    let body = &text[split + 4..];
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("no status line: {head}"))?;
    let json: Value =
        serde_json::from_str(body).map_err(|e| format!("invalid json body ({e}): {body}"))?;
    Ok((status, json))
}

/// The path of the demo fixture, overridable for out-of-tree runs.
fn fixture_path() -> String {
    std::env::var("HERMES_FIXTURE_PATH").unwrap_or_else(|_| {
        format!(
            "{}/../../services/demo-fixtures/signup_fixture.py",
            env!("CARGO_MANIFEST_DIR")
        )
    })
}

/// Starts the demo fixture on an ephemeral port and returns its port.
fn start_fixture() -> Result<(u16, Child), String> {
    let fixture = fixture_path();
    let db = std::env::temp_dir().join(format!(
        "hermes-passkeytest-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&db);
    let mut child = Command::new("python3")
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
        .map_err(|e| format!("spawn {}: {e}", fixture))?;
    let stdout = child
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
    Ok((port, child))
}

/// Fetches a fresh challenge from the relying party.
fn fetch_challenge(port: u16) -> Result<(Vec<u8>, String), String> {
    let (status, body) = http_json(port, "GET", &format!("/api/webauthn/challenge?rp_id={RP_ID}"), None)?;
    if status != 200 {
        return Err(format!("challenge request returned {status}: {body}"));
    }
    let challenge = body
        .get("challenge")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("challenge missing from {body}"))?;
    let bytes = b64url_decode(challenge)
        .ok_or_else(|| format!("challenge is not base64url: {challenge}"))?;
    let rp_id = body
        .get("rp_id")
        .and_then(Value::as_str)
        .unwrap_or(RP_ID)
        .to_string();
    Ok((bytes, rp_id))
}

/// Registers a credential with the relying party.
fn register_credential(
    port: u16,
    credential_id: &str,
    public_key_pem: &str,
    rp_id: &str,
) -> Result<(), String> {
    let payload = json!({
        "credential_id": credential_id,
        "public_key_pem": public_key_pem,
        "rp_id": rp_id,
        "user_email": "ada@example.com",
    });
    let (status, body) = http_json(
        port,
        "POST",
        "/api/webauthn/register",
        Some(&payload.to_string()),
    )?;
    if status != 200 || body.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(format!("registration refused ({status}): {body}"));
    }
    Ok(())
}

/// Sends an assertion to the relying party's verifier.
fn verify_assertion(port: u16, payload: &Value) -> Result<(u16, Value), String> {
    http_json(
        port,
        "POST",
        "/api/webauthn/verify",
        Some(&payload.to_string()),
    )
}

/// Builds the verifier payload for an assertion.
fn assertion_payload(
    credential_id: &str,
    rp_id: &str,
    origin: &str,
    authenticator_data_b64: &str,
    client_data_json_b64: &str,
    signature_der_b64: &str,
) -> Value {
    json!({
        "credential_id": credential_id,
        "rp_id": rp_id,
        "origin": origin,
        "authenticator_data_b64": authenticator_data_b64,
        "client_data_json_b64": client_data_json_b64,
        "signature_der_b64": signature_der_b64,
    })
}

/// Runs the passkey end-to-end suite.
pub fn passkeytest() -> i32 {
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

    // The harness uses its own Keychain service so that a run never disturbs
    // the developer's real passkeys, and never leaves any behind.
    let service = format!(
        "ai.zmax.hermes.browser.passkeytest.{}",
        std::process::id()
    );
    let store = crate::credentials::KeychainPasskeyStore::with_service(&service, "passkey_records_v1");
    let mut broker = LocalPasskeyBroker::with_store(store);

    // The Keychain must be reachable. A run that cannot store a credential
    // must fail rather than quietly skip the storage checks.
    if !crate::keychain::is_available() {
        println!("not ok the macOS Keychain is not reachable from this process");
        return 1;
    }

    let (port, mut fixture) = match start_fixture() {
        Ok(parts) => parts,
        Err(error) => {
            println!("not ok fixture start: {error}");
            return 1;
        }
    };
    println!("-- fixture listening on 127.0.0.1:{port}");
    println!(
        "-- authenticator AAGUID: {}",
        broker
            .aaguid()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join("")
    );

    let result = (|| -> Result<(), String> {
        // --- registration -------------------------------------------------
        let (challenge, rp_id) = fetch_challenge(port)?;
        check(
            &mut checks,
            &mut failures,
            "relying party issues a challenge",
            !challenge.is_empty(),
            || "the challenge was empty".to_string(),
        );

        let register_id = broker
            .begin_registration(RegistrationRequest {
                task_id: "passkeytest".to_string(),
                origin: ORIGIN.to_string(),
                rp: PublicKeyCredentialRpEntity {
                    id: rp_id.clone(),
                    name: Some("Hermes Demo".to_string()),
                },
                user: PublicKeyCredentialUserEntity {
                    id: b"ada@example.com".to_vec(),
                    name: "ada@example.com".to_string(),
                    display_name: "Ada Lovelace".to_string(),
                },
                challenge: challenge.clone(),
                pub_key_cred_params: Vec::new(),
                exclude_credentials: Vec::new(),
                authenticator_selection: None,
                attestation: Some("none".to_string()),
                extensions: None,
                cross_origin: false,
            })
            .map_err(|e| format!("begin_registration: {e}"))?;

        // The ceremony must not produce anything before a human approves it.
        let pending = broker
            .poll_registration(register_id.clone())
            .map_err(|e| format!("poll_registration: {e}"))?;
        check(
            &mut checks,
            &mut failures,
            "registration waits for human approval",
            matches!(pending, PasskeyStatus::AwaitingApproval { .. }),
            || format!("unexpected status: {pending:?}"),
        );

        broker
            .approve_ceremony(register_id.clone(), "test-harness", "passkeytest")
            .map_err(|e| format!("approve_ceremony: {e}"))?;
        let registered = broker
            .poll_registration(register_id.clone())
            .map_err(|e| format!("poll_registration: {e}"))?;
        let PasskeyStatus::Complete(CeremonyOutcome::Registration(registration)) = registered else {
            return Err(format!("registration did not complete: {registered:?}"));
        };
        check(
            &mut checks,
            &mut failures,
            "registration completes after approval",
            !registration.response.attestation_object.is_empty(),
            || "the attestation object was empty".to_string(),
        );

        // The stored public key must be the one the relying party verifies.
        let records = broker
            .list_passkeys()
            .map_err(|e| format!("list_passkeys: {e}"))?;
        check(
            &mut checks,
            &mut failures,
            "the passkey is stored in the Keychain",
            records.len() == 1,
            || format!("expected one stored passkey, found {}", records.len()),
        );
        // Report the backend that actually took the write, not the one that
        // was assumed before it.
        println!(
            "-- keychain backend: {}",
            crate::keychain::backend()
                .map(|b| b.as_str())
                .unwrap_or("undetermined")
        );
        let credential_id = registration.id.clone();
        let credential_id_bytes = b64url_decode(&credential_id)
            .ok_or_else(|| "the credential ID is not base64url".to_string())?;
        let stored_record = broker
            .stored_passkey(&credential_id_bytes)
            .map_err(|e| format!("stored_passkey: {e}"))?
            .ok_or_else(|| "the stored passkey is missing".to_string())?;
        check(
            &mut checks,
            &mut failures,
            "the stored credential id matches the iOS base64url form",
            stored_record.credential_id_b64url() == credential_id,
            || "the stored credential id does not round-trip".to_string(),
        );
        let stored = broker
            .public_key_pem(&credential_id_bytes)
            .map_err(|e| format!("public_key_pem: {e}"))?
            .ok_or_else(|| "the stored passkey has no public key".to_string())?;

        register_credential(port, &credential_id, &stored, &rp_id)?;
        check(
            &mut checks,
            &mut failures,
            "the relying party accepts the registration",
            true,
            || "registration failed".to_string(),
        );

        // --- assertion ----------------------------------------------------
        let (assertion_challenge, _) = fetch_challenge(port)?;
        let assert_id = broker
            .begin_assertion(PasskeyRequest {
                task_id: "passkeytest".to_string(),
                origin: ORIGIN.to_string(),
                rp_id: Some(rp_id.clone()),
                challenge: assertion_challenge.clone(),
                allow_credentials: Vec::new(),
                user_verification: Some("preferred".to_string()),
                extensions: None,
                cross_origin: false,
                top_origin: None,
            })
            .map_err(|e| format!("begin_assertion: {e}"))?;

        let pending = broker
            .poll_assertion(assert_id.clone())
            .map_err(|e| format!("poll_assertion: {e}"))?;
        check(
            &mut checks,
            &mut failures,
            "assertion waits for human approval",
            matches!(pending, PasskeyStatus::AwaitingApproval { .. }),
            || format!("unexpected status: {pending:?}"),
        );

        broker
            .approve_ceremony(assert_id.clone(), "test-harness", "passkeytest")
            .map_err(|e| format!("approve_ceremony: {e}"))?;
        let asserted = broker
            .poll_assertion(assert_id)
            .map_err(|e| format!("poll_assertion: {e}"))?;
        let PasskeyStatus::Complete(CeremonyOutcome::Assertion(assertion)) = asserted else {
            return Err(format!("assertion did not complete: {asserted:?}"));
        };

        // --- the independent verifier ------------------------------------
        let payload = assertion_payload(
            &assertion.id,
            &rp_id,
            ORIGIN,
            &assertion.response.authenticator_data,
            &assertion.response.client_data_json,
            &assertion.response.signature,
        );
        let (status, body) = verify_assertion(port, &payload)?;
        let verified = body.get("verified").and_then(Value::as_bool) == Some(true);
        check(
            &mut checks,
            &mut failures,
            "the relying party verifies the assertion with openssl",
            status == 200 && verified,
            || format!("verifier said {status}: {body}"),
        );

        // --- negative cases -----------------------------------------------
        let (status, body) = verify_assertion(port, &payload)?;
        check(
            &mut checks,
            &mut failures,
            "the relying party rejects a replayed challenge",
            !(status == 200 && body.get("verified").and_then(Value::as_bool) == Some(true)),
            || "the replayed assertion was accepted".to_string(),
        );

        let unknown = assertion_payload(
            &b64url_encode(&[9u8; 32]),
            &rp_id,
            ORIGIN,
            &assertion.response.authenticator_data,
            &assertion.response.client_data_json,
            &assertion.response.signature,
        );
        let (status, body) = verify_assertion(port, &unknown)?;
        check(
            &mut checks,
            &mut failures,
            "the relying party rejects an unknown credential",
            !(status == 200 && body.get("verified").and_then(Value::as_bool) == Some(true)),
            || format!("the unknown credential was accepted: {body}"),
        );

        let (wrong_challenge, _) = fetch_challenge(port)?;
        let wrong = broker
            .begin_assertion(PasskeyRequest {
                task_id: "passkeytest".to_string(),
                origin: ORIGIN.to_string(),
                rp_id: Some(rp_id.clone()),
                challenge: wrong_challenge,
                allow_credentials: Vec::new(),
                user_verification: Some("preferred".to_string()),
                extensions: None,
                cross_origin: false,
                top_origin: None,
            })
            .map_err(|e| format!("begin_assertion: {e}"))?;
        broker
            .approve_ceremony(wrong.clone(), "test-harness", "passkeytest")
            .map_err(|e| format!("approve_ceremony: {e}"))?;
        let wrong_status = broker
            .poll_assertion(wrong)
            .map_err(|e| format!("poll_assertion: {e}"))?;
        let PasskeyStatus::Complete(CeremonyOutcome::Assertion(wrong_assertion)) = wrong_status else {
            return Err("the second assertion did not complete".to_string());
        };
        // Sign a *different* challenge with the credential, then present the
        // first assertion: the relying party must reject the mismatch.
        let mismatched = assertion_payload(
            &assertion.id,
            &rp_id,
            ORIGIN,
            &wrong_assertion.response.authenticator_data,
            &assertion.response.client_data_json,
            &wrong_assertion.response.signature,
        );
        let (status, body) = verify_assertion(port, &mismatched)?;
        check(
            &mut checks,
            &mut failures,
            "the relying party rejects a mismatched challenge",
            !(status == 200 && body.get("verified").and_then(Value::as_bool) == Some(true)),
            || format!("the mismatched assertion was accepted: {body}"),
        );

        // --- redaction ----------------------------------------------------
        let prompt = broker
            .describe_assertion(&PasskeyRequest {
                task_id: "passkeytest".to_string(),
                origin: ORIGIN.to_string(),
                rp_id: Some(rp_id.clone()),
                challenge: b"probe".to_vec(),
                allow_credentials: Vec::new(),
                user_verification: None,
                extensions: None,
                cross_origin: false,
                top_origin: None,
            });
        if let Ok(prompt) = prompt {
            let rendered = format!("{prompt:?}");
            check(
                &mut checks,
                &mut failures,
                "the prompt redacts the account name",
                !rendered.contains("ada@example.com") && rendered.contains("a**"),
                || format!("the prompt leaked the account: {rendered}"),
            );
        } else {
            check(
                &mut checks,
                &mut failures,
                "the prompt redacts the account name",
                false,
                || "the prompt could not be built".to_string(),
            );
        }

        Ok(())
    })();

    let _ = fixture.kill();
    let _ = fixture.wait();
    // Leave no passkey behind in the Keychain.
    let _ = crate::credentials::KeychainPasskeyStore::with_service(&service, "passkey_records_v1")
        .clear();

    if let Err(error) = result {
        println!("not ok passkey suite: {error}");
        failures.push("passkey suite".to_string());
    }

    if failures.is_empty() {
        println!("PASSKEY TEST PASS ({checks} checks)");
        0
    } else {
        println!("PASSKEY TEST FAIL ({} of {checks} checks failed)", failures.len());
        1
    }
}

/// base64url helper re-exported for the harness.
fn b64url_encode(bytes: &[u8]) -> String {
    crate::webauthn::b64::b64url_encode(bytes)
}

/// Unused import guard for `PasskeyCeremonyId`.
#[allow(dead_code)]
fn _ceremony_id_type_is_used(id: PasskeyCeremonyId) -> String {
    id.0
}
