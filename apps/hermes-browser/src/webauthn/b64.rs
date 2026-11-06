//! base64url (RFC 4648 §5) without padding.
//!
//! WebAuthn encodes challenges, credential IDs, and the attestation/assertion
//! blobs in base64url. The control transport in `control.rs` uses standard
//! base64 (`+/`), which is not interchangeable, so the authenticator carries
//! its own encoder.

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encodes bytes as unpadded base64url.
pub fn b64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0] as u32,
            *chunk.get(1).unwrap_or(&0) as u32,
            *chunk.get(2).unwrap_or(&0) as u32,
        ];
        let n = (b[0] << 16) | (b[1] << 8) | b[2];
        out.push(B64URL[(n >> 18) as usize & 63] as char);
        out.push(B64URL[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64URL[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[n as usize & 63] as char);
        }
    }
    out
}

/// Decodes unpadded (or padded) base64url.
pub fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    // Padding is a terminator, not data.
    let text = match text.find('=') {
        Some(index) => &text[..index],
        None => text,
    };
    let mut values = Vec::with_capacity(text.len());
    for ch in text.bytes() {
        values.push(match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        });
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
    fn round_trips_arbitrary_bytes() {
        for len in 0..64usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 % 251) as u8).collect();
            let encoded = b64url_encode(&bytes);
            assert!(!encoded.contains('+'));
            assert!(!encoded.contains('/'));
            assert!(!encoded.contains('='));
            assert_eq!(b64url_decode(&encoded).unwrap(), bytes);
        }
    }

    #[test]
    fn uses_the_url_safe_alphabet() {
        // 0xFB 0xFF encodes to "-_" in base64url and "+/" in standard base64.
        assert_eq!(b64url_encode(&[0xfb, 0xff, 0xbf]), "-_-_");
    }

    #[test]
    fn matches_rfc4648_vectors() {
        assert_eq!(b64url_encode(b""), "");
        assert_eq!(b64url_encode(b"f"), "Zg");
        assert_eq!(b64url_encode(b"fo"), "Zm8");
        assert_eq!(b64url_encode(b"foo"), "Zm9v");
        assert_eq!(b64url_encode(b"foob"), "Zm9vYg");
        assert_eq!(b64url_encode(b"fooba"), "Zm9vYmE");
        assert_eq!(b64url_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decodes_padded_input_too() {
        assert_eq!(b64url_decode("Zm9vYg==").unwrap(), b"foob");
    }
}
