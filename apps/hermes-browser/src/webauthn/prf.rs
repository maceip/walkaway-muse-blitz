//! WebAuthn PRF (HMAC-Secret) evaluator.
//!
//! Ported from the iOS Equipped Browser `WebAuthnPRFEvaluator` in
//! `WebAuthnManager.swift`.
//!
//! Per W3C WebAuthn Level 3 §10.1.4 the salt is hashed before use:
//!   saltInput  = "WebAuthn PRF" || 0x00 || salt
//!   saltHash   = SHA-256(saltInput)
//!   output     = HMAC-SHA-256(key = credential PRF secret, message = saltHash)

use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use sha2::{Digest, Sha256};

/// The PRF salt prefix defined by WebAuthn Level 3.
const PRF_PREFIX: &[u8] = b"WebAuthn PRF";

/// The evaluated PRF outputs for a single assertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrfValues {
    /// Output for the first salt.
    pub first: Vec<u8>,
    /// Output for the second salt, when the relying party asked for one.
    pub second: Option<Vec<u8>>,
}

/// Evaluates the PRF for the given credential secret and salts.
pub fn evaluate(
    secret: &[u8],
    first_salt: &[u8],
    second_salt: Option<&[u8]>,
) -> Result<PrfValues, String> {
    let first = evaluate_single_salt(secret, first_salt)?;
    let second = match second_salt {
        Some(salt) => Some(evaluate_single_salt(secret, salt)?),
        None => None,
    };
    Ok(PrfValues { first, second })
}

/// Evaluates one PRF salt.
fn evaluate_single_salt(secret: &[u8], salt: &[u8]) -> Result<Vec<u8>, String> {
    let mut salt_input = Vec::with_capacity(PRF_PREFIX.len() + 1 + salt.len());
    salt_input.extend_from_slice(PRF_PREFIX);
    salt_input.push(0x00);
    salt_input.extend_from_slice(salt);
    let salt_hash = Sha256::digest(&salt_input);

    let key = PKey::hmac(secret).map_err(|e| format!("prf hmac key: {e}"))?;
    let mut signer =
        Signer::new(MessageDigest::sha256(), &key).map_err(|e| format!("prf signer: {e}"))?;
    signer
        .update(&salt_hash)
        .map_err(|e| format!("prf update: {e}"))?;
    signer
        .sign_to_vec()
        .map_err(|e| format!("prf sign: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_a_32_byte_hmac() {
        let values = evaluate(b"secret", b"salt", None).unwrap();
        assert_eq!(values.first.len(), 32);
        assert!(values.second.is_none());
    }

    #[test]
    fn is_deterministic_and_salt_dependent() {
        let a = evaluate(b"secret", b"salt", None).unwrap();
        let b = evaluate(b"secret", b"salt", None).unwrap();
        assert_eq!(a.first, b.first);
        let c = evaluate(b"secret", b"other", None).unwrap();
        assert_ne!(a.first, c.first);
        let d = evaluate(b"different", b"salt", None).unwrap();
        assert_ne!(a.first, d.first);
    }

    #[test]
    fn computes_the_second_output_when_a_second_salt_is_supplied() {
        let values = evaluate(b"secret", b"salt1", Some(b"salt2")).unwrap();
        let second = values.second.unwrap();
        assert_eq!(second.len(), 32);
        assert_ne!(values.first, second);
    }

    #[test]
    fn matches_the_specified_salt_construction() {
        // Recompute the expected output independently of the implementation.
        let secret = b"credential-secret";
        let salt = b"rpsalt";
        let mut salt_input = Vec::new();
        salt_input.extend_from_slice(b"WebAuthn PRF");
        salt_input.push(0x00);
        salt_input.extend_from_slice(salt);
        let expected_hash = Sha256::digest(&salt_input);

        let key = PKey::hmac(secret).unwrap();
        let mut signer = Signer::new(MessageDigest::sha256(), &key).unwrap();
        signer.update(&expected_hash).unwrap();
        let expected = signer.sign_to_vec().unwrap();

        let values = evaluate(secret, salt, None).unwrap();
        assert_eq!(values.first, expected);
    }
}
