//! Cryptographically secure random bytes.
//!
//! All authenticator randomness (credential IDs, PRF secrets, largeBlob keys,
//! user handles, challenges) comes from the operating system entropy source.

/// Fills `buf` with cryptographically secure random bytes.
pub fn fill(buf: &mut [u8]) -> Result<(), String> {
    getrandom::fill(buf).map_err(|e| format!("system entropy: {e}"))
}

/// Returns `count` cryptographically secure random bytes.
pub fn bytes(count: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; count];
    fill(&mut buf)?;
    Ok(buf)
}

/// Returns a random 32-byte value.
pub fn bytes_32() -> Result<[u8; 32], String> {
    let mut buf = [0u8; 32];
    fill(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_distinct_values() {
        let a = bytes(32).unwrap();
        let b = bytes(32).unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn produces_the_requested_length() {
        for len in [0usize, 1, 16, 32, 65] {
            assert_eq!(bytes(len).unwrap().len(), len);
        }
    }
}
