//! Minimal RFC 8949 CBOR encoder.
//!
//! Port of the iOS Equipped Browser encoder (`WebAuthnManager.swift`, section
//! "Minimal RFC 8949 CBOR Encoder (for attestationObject, COSE_Key, and
//! extension outputs)"), which backs the Equipped Browser authenticator.
//!
//! One deliberate refinement over the Swift original: map keys are emitted in
//! RFC 8949 §4.2.1 deterministic order (ascending by encoded key bytes,
//! last value wins on duplicate keys). The Swift encoder emitted map pairs in
//! source order. Deterministic order is what CTAP2 and real relying parties
//! expect, and no verifier in this project depends on the Swift byte order.

/// A CBOR data item. Only the types WebAuthn needs are representable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cbor {
    /// Major type 0.
    Uint(u64),
    /// Major type 1. Holds the actual (negative) value, e.g. `Neg(-7)`.
    Neg(i64),
    /// Major type 2.
    Bytes(Vec<u8>),
    /// Major type 3.
    Text(String),
    /// Major type 4.
    Array(Vec<Cbor>),
    /// Major type 5.
    Map(Vec<(Cbor, Cbor)>),
    /// Major type 7 (simple values 20 and 21).
    Bool(bool),
}

impl Cbor {
    /// Convenience constructor for a negative integer.
    pub fn neg(value: i64) -> Cbor {
        Cbor::Neg(value)
    }

    /// Convenience constructor for a text string.
    pub fn text(value: impl Into<String>) -> Cbor {
        Cbor::Text(value.into())
    }

    /// Convenience constructor for a byte string.
    pub fn bytes(value: impl Into<Vec<u8>>) -> Cbor {
        Cbor::Bytes(value.into())
    }

    /// Encodes the item to its CBOR byte representation.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_into(&mut out);
        out
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        match self {
            Cbor::Uint(u) => header(0, *u, out),
            // Major type 1 encodes the value `-1 - argument`.
            Cbor::Neg(n) => {
                let arg = (-1i128 - (*n as i128)) as u64;
                header(1, arg, out);
            }
            Cbor::Bytes(b) => {
                header(2, b.len() as u64, out);
                out.extend_from_slice(b);
            }
            Cbor::Text(t) => {
                let utf8 = t.as_bytes();
                header(3, utf8.len() as u64, out);
                out.extend_from_slice(utf8);
            }
            Cbor::Array(items) => {
                header(4, items.len() as u64, out);
                for item in items {
                    item.encode_into(out);
                }
            }
            Cbor::Map(pairs) => {
                let encoded: Vec<(Vec<u8>, Vec<u8>)> = pairs
                    .iter()
                    .map(|(k, v)| (k.encode(), v.encode()))
                    .collect();
                // RFC 8949 4.2.1: sort by encoded key bytes, duplicates keep the last value.
                let mut sorted = encoded;
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                let mut deduped: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(sorted.len());
                for (k, v) in sorted {
                    if let Some(last) = deduped.last_mut() {
                        if last.0 == k {
                            last.1 = v;
                            continue;
                        }
                    }
                    deduped.push((k, v));
                }
                header(5, deduped.len() as u64, out);
                for (k, v) in deduped {
                    out.extend_from_slice(&k);
                    out.extend_from_slice(&v);
                }
            }
            Cbor::Bool(b) => out.push(if *b { 0xf5 } else { 0xf4 }),
        }
    }
}

/// Writes a major-type header with the shortest possible argument encoding.
fn header(major: u8, argument: u64, out: &mut Vec<u8>) {
    let prefix = major << 5;
    if argument < 24 {
        out.push(prefix | argument as u8);
    } else if argument <= u64::from(u8::MAX) {
        out.push(prefix | 24);
        out.push(argument as u8);
    } else if argument <= u64::from(u16::MAX) {
        out.push(prefix | 25);
        out.extend_from_slice(&(argument as u16).to_be_bytes());
    } else if argument <= u64::from(u32::MAX) {
        out.push(prefix | 26);
        out.extend_from_slice(&(argument as u32).to_be_bytes());
    } else {
        out.push(prefix | 27);
        out.extend_from_slice(&argument.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_shortest_integer_forms() {
        assert_eq!(Cbor::Uint(0).encode(), vec![0x00]);
        assert_eq!(Cbor::Uint(23).encode(), vec![0x17]);
        assert_eq!(Cbor::Uint(24).encode(), vec![0x18, 0x18]);
        assert_eq!(Cbor::Uint(256).encode(), vec![0x19, 0x01, 0x00]);
        assert_eq!(Cbor::Uint(65536).encode(), vec![0x1a, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encodes_negative_integers_as_major_type_one() {
        // -1 encodes as 0x20, -7 encodes as 0x26.
        assert_eq!(Cbor::Neg(-1).encode(), vec![0x20]);
        assert_eq!(Cbor::Neg(-7).encode(), vec![0x26]);
    }

    #[test]
    fn encodes_text_bytes_and_booleans() {
        assert_eq!(Cbor::text("").encode(), vec![0x60]);
        assert_eq!(Cbor::text("a").encode(), vec![0x61, 0x61]);
        assert_eq!(Cbor::bytes(vec![1, 2, 3]).encode(), vec![0x43, 1, 2, 3]);
        assert_eq!(Cbor::Bool(true).encode(), vec![0xf5]);
        assert_eq!(Cbor::Bool(false).encode(), vec![0xf4]);
    }

    #[test]
    fn encodes_arrays_and_maps() {
        let arr = Cbor::Array(vec![Cbor::Uint(1), Cbor::Uint(2)]);
        assert_eq!(arr.encode(), vec![0x82, 0x01, 0x02]);

        let map = Cbor::Map(vec![
            (Cbor::text("b"), Cbor::Uint(2)),
            (Cbor::text("a"), Cbor::Uint(1)),
        ]);
        // Deterministic order: "a" before "b".
        assert_eq!(
            map.encode(),
            vec![0xa2, 0x61, b'a', 0x01, 0x61, b'b', 0x02]
        );
    }

    #[test]
    fn map_duplicate_keys_keep_the_last_value() {
        let map = Cbor::Map(vec![
            (Cbor::text("k"), Cbor::Uint(1)),
            (Cbor::text("k"), Cbor::Uint(2)),
        ]);
        assert_eq!(map.encode(), vec![0xa1, 0x61, b'k', 0x02]);
    }

    #[test]
    fn cose_style_integer_keys_sort_numerically_by_encoded_bytes() {
        // COSE_Key labels: 1 (kty), 3 (alg), -1 (crv), -2 (x), -3 (y).
        // Encoded key bytes: 0x01, 0x03, 0x20, 0x21, 0x22.
        let map = Cbor::Map(vec![
            (Cbor::Neg(-3), Cbor::bytes(vec![0x01])),
            (Cbor::Neg(-1), Cbor::Uint(1)),
            (Cbor::Uint(3), Cbor::Neg(-7)),
            (Cbor::Uint(1), Cbor::Uint(2)),
            (Cbor::Neg(-2), Cbor::bytes(vec![0x02])),
        ]);
        let encoded = map.encode();
        // Map of 5 pairs, then the pairs in ascending encoded-key order:
        // 1 -> 2, 3 -> -7, -1 -> 1, -2 -> bstr, -3 -> bstr.
        assert_eq!(
            encoded,
            vec![
                0xa5, // map, 5 pairs
                0x01, 0x02, // 1: 2
                0x03, 0x26, // 3: -7
                0x20, 0x01, // -1: 1
                0x21, 0x41, 0x02, // -2: bstr(1) = 0x02
                0x22, 0x41, 0x01, // -3: bstr(1) = 0x01
            ]
        );
    }
}
