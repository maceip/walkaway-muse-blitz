//! Authenticated Unix-socket NDJSON transport for the supervised control API.
//!
//! The transport assigns a channel role from the `hello` token and forwards
//! parsed frames to the browser event loop. All state, authority checks, and
//! page execution live in `BrowserApp`; socket threads never touch the page.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use winit::event_loop::EventLoopProxy;

use crate::UserEvent;

pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Host,
    Agent,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Host => "host",
            Role::Agent => "agent",
        }
    }
}

/// Outbound routing from the event loop back to socket writer threads.
pub struct ControlHub {
    senders: Mutex<HashMap<u64, mpsc::Sender<String>>>,
}

impl ControlHub {
    pub fn new() -> Self {
        Self {
            senders: Mutex::new(HashMap::new()),
        }
    }

    fn register(&self, conn_id: u64, sender: mpsc::Sender<String>) {
        self.senders.lock().expect("hub").insert(conn_id, sender);
    }

    fn unregister(&self, conn_id: u64) {
        self.senders.lock().expect("hub").remove(&conn_id);
    }

    pub fn send(&self, conn_id: u64, frame: &Value) {
        let line = frame.to_string();
        if let Some(sender) = self.senders.lock().expect("hub").get(&conn_id) {
            let _ = sender.send(line);
        }
    }
}

fn token_eq(candidate: &str, expected: &str) -> bool {
    let a = candidate.as_bytes();
    let b = expected.as_bytes();
    let mut diff = (a.len() != b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

pub fn spawn_server(
    proxy: EventLoopProxy<UserEvent>,
    hub: Arc<ControlHub>,
    socket_path: PathBuf,
    host_token: String,
    agent_token: String,
) -> std::io::Result<()> {
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    eprintln!("[control] listening on {}", socket_path.display());
    let next_conn = Arc::new(AtomicU64::new(1));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let conn_id = next_conn.fetch_add(1, Ordering::SeqCst);
            let proxy = proxy.clone();
            let hub = hub.clone();
            let host_token = host_token.clone();
            let agent_token = agent_token.clone();
            std::thread::spawn(move || {
                handle_connection(stream, conn_id, proxy, hub, &host_token, &agent_token);
            });
        }
    });
    Ok(())
}

fn handle_connection(
    stream: UnixStream,
    conn_id: u64,
    proxy: EventLoopProxy<UserEvent>,
    hub: Arc<ControlHub>,
    host_token: &str,
    agent_token: &str,
) {
    let Ok(writer_stream) = stream.try_clone() else {
        return;
    };
    let (tx, rx) = mpsc::channel::<String>();
    let writer = std::thread::spawn(move || {
        let mut writer_stream = writer_stream;
        while let Ok(line) = rx.recv() {
            if writer_stream.write_all(line.as_bytes()).is_err()
                || writer_stream.write_all(b"\n").is_err()
                || writer_stream.flush().is_err()
            {
                break;
            }
        }
    });

    let mut reader = BufReader::new(stream);
    let mut authed: Option<Role> = None;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.len() > MAX_FRAME_BYTES {
            let _ = tx.send(json!({"v":1,"type":"refused","reason":"frame_too_large"}).to_string());
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let frame: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                let _ = tx.send(json!({"v":1,"type":"refused","reason":"internal"}).to_string());
                continue;
            }
        };
        if authed.is_none() {
            let token = frame.get("token").and_then(Value::as_str).unwrap_or("");
            let role = if frame.get("type").and_then(Value::as_str) == Some("hello")
                && token_eq(token, host_token)
            {
                Some(Role::Host)
            } else if frame.get("type").and_then(Value::as_str) == Some("hello")
                && token_eq(token, agent_token)
            {
                Some(Role::Agent)
            } else {
                None
            };
            let Some(role) = role else {
                let _ =
                    tx.send(json!({"v":1,"type":"refused","reason":"wrong_channel"}).to_string());
                break;
            };
            authed = Some(role);
            hub.register(conn_id, tx.clone());
            let request_id = frame.get("request_id").cloned().unwrap_or(Value::Null);
            let _ = proxy.send_event(UserEvent::ControlConnect { conn_id, role });
            let _ = tx.send(
                json!({
                    "v": 1, "type": "hello.ok", "request_id": request_id,
                    "role": role.as_str(), "protocol": 1,
                    "engine": {"name": "hermes-blitz", "version": env!("CARGO_PKG_VERSION")}
                })
                .to_string(),
            );
            continue;
        }
        if proxy
            .send_event(UserEvent::ControlMessage { conn_id, frame })
            .is_err()
        {
            break;
        }
    }
    hub.unregister(conn_id);
    let _ = proxy.send_event(UserEvent::ControlDisconnect { conn_id });
    drop(tx);
    let _ = writer.join();
}

/// ISO-8601 UTC timestamp without a date crate (civil-from-days conversion).
pub fn now_iso() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    let days = secs.div_euclid(86_400);
    let day_secs = secs.rem_euclid(86_400);
    let (hour, minute, second) = (day_secs / 3600, (day_secs % 3600) / 60, day_secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0] as u32,
            *chunk.get(1).unwrap_or(&0) as u32,
            *chunk.get(2).unwrap_or(&0) as u32,
        ];
        let n = (b[0] << 16) | (b[1] << 8) | b[2];
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Decodes standard base64 (`+/`), with or without `=` padding.
///
/// The final group carries fewer than four characters when the input is
/// unpadded, and then it encodes fewer than three bytes: a group of three
/// characters holds two bytes, and a group of two characters holds one. The
/// count has to come from the group's actual length. Counting only explicit
/// `=` padding would decode an unpadded 43-character string as 33 bytes
/// instead of 32, silently appending a byte and corrupting the payload.
pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    let mut values = Vec::with_capacity(text.len());
    for ch in text.bytes() {
        values.push(match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        });
    }
    // Explicit padding is a terminator, not data.
    while values.last() == Some(&64) {
        values.pop();
    }
    let mut out = Vec::with_capacity(values.len() / 4 * 3);
    for chunk in values.chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let n = ((chunk[0] as u32) << 18)
            | ((chunk[1] as u32) << 12)
            | ((*chunk.get(2).unwrap_or(&0) as u32) << 6)
            | (*chunk.get(3).unwrap_or(&0) as u32);
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_round_trips_arbitrary_bytes() {
        for len in 0..64usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = b64_encode(&bytes);
            assert_eq!(
                b64_decode(&encoded),
                Some(bytes.clone()),
                "length {len} did not round-trip"
            );
        }
    }

    #[test]
    fn b64_decodes_unpadded_input_to_the_right_number_of_bytes() {
        // 32 bytes is 43 base64 characters. The final group holds three
        // characters, which carry two bytes, not three. Decoding it as three
        // bytes would append a spurious byte and corrupt the payload.
        let bytes: Vec<u8> = (0..32u8).collect();
        let encoded = b64_encode(&bytes);
        assert_eq!(encoded.len(), 44);
        let unpadded = encoded.trim_end_matches('=');
        assert_eq!(unpadded.len(), 43);
        assert_eq!(
            b64_decode(unpadded),
            Some(bytes),
            "an unpadded 43-character string must decode to 32 bytes"
        );
    }

    #[test]
    fn b64_decodes_a_two_character_final_group_to_one_byte() {
        // "QQ" is one byte. "QUJD" is three bytes; "QUI" is two.
        assert_eq!(b64_decode("QQ"), Some(vec![0x41]));
        assert_eq!(b64_decode("QUI"), Some(vec![0x41, 0x42]));
        assert_eq!(b64_decode("QUJD"), Some(vec![0x41, 0x42, 0x43]));
    }

    #[test]
    fn b64_accepts_explicit_padding() {
        assert_eq!(b64_decode("QQ=="), Some(vec![0x41]));
        assert_eq!(b64_decode("QUI="), Some(vec![0x41, 0x42]));
        assert_eq!(b64_decode("QUJD"), Some(vec![0x41, 0x42, 0x43]));
    }
}
