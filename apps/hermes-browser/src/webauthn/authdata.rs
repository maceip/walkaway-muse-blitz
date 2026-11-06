//! WebAuthn `authenticatorData` and `attestationObject` construction.
//!
//! Ported from the iOS Equipped Browser authenticator data assembly in
//! `WebAuthnManager.swift` (the `flags` / `authData` construction in the
//! registration and assertion paths).
//!
//! Layout (WebAuthn Level 3 §6.1):
//!   rpIdHash (32) || flags (1) || signCount (4, big endian)
//!   [ attestedCredentialData ]  when the AT flag is set
//!   [ extensions CBOR ]         when the ED flag is set

use sha2::{Digest, Sha256};

use crate::webauthn::cbor::Cbor;

/// User Present.
pub const FLAG_UP: u8 = 0x01;
/// User Verified.
pub const FLAG_UV: u8 = 0x04;
/// Backup Eligible.
pub const FLAG_BE: u8 = 0x08;
/// Backup State.
pub const FLAG_BS: u8 = 0x10;
/// Attested credential data included.
pub const FLAG_AT: u8 = 0x40;
/// Extension data included.
pub const FLAG_ED: u8 = 0x80;

/// The attested credential data appended to `authenticatorData` on registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttestedCredentialData {
    /// Authenticator attestation GUID. Zeroed for a self-attested software
    /// authenticator that has no AAGUID.
    pub aaguid: [u8; 16],
    /// Raw credential ID.
    pub credential_id: Vec<u8>,
    /// CBOR-encoded COSE_Key.
    pub credential_public_key: Vec<u8>,
}

impl AttestedCredentialData {
    /// Builds attested credential data from raw parts.
    pub fn new(aaguid: [u8; 16], credential_id: Vec<u8>, credential_public_key: Vec<u8>) -> Self {
        Self {
            aaguid,
            credential_id,
            credential_public_key,
        }
    }
}

/// A complete `authenticatorData` structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatorData {
    /// SHA-256 of the effective relying party identifier.
    pub rp_id_hash: [u8; 32],
    /// Bit flags (see the `FLAG_*` constants).
    pub flags: u8,
    /// Signature counter.
    pub sign_count: u32,
    /// Present when the AT flag is set.
    pub attested_credential_data: Option<AttestedCredentialData>,
    /// CBOR-encoded extension outputs, present when the ED flag is set.
    pub extensions: Option<Vec<u8>>,
}

impl AuthenticatorData {
    /// Computes the rpIdHash for a relying party identifier.
    pub fn rp_id_hash(rp_id: &str) -> [u8; 32] {
        let digest = Sha256::digest(rp_id.as_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }

    /// Serializes the structure to its wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(37 + 64);
        out.extend_from_slice(&self.rp_id_hash);
        out.push(self.flags);
        out.extend_from_slice(&self.sign_count.to_be_bytes());
        if self.flags & FLAG_AT != 0 {
            if let Some(attested) = &self.attested_credential_data {
                out.extend_from_slice(&attested.aaguid);
                let len = attested.credential_id.len() as u16;
                out.extend_from_slice(&len.to_be_bytes());
                out.extend_from_slice(&attested.credential_id);
                out.extend_from_slice(&attested.credential_public_key);
            }
        }
        if self.flags & FLAG_ED != 0 {
            if let Some(extensions) = &self.extensions {
                out.extend_from_slice(extensions);
            }
        }
        out
    }

    /// Builds the `attestationObject` that wraps this authenticator data.
    ///
    /// The format is `none`: the self attestation used by a software
    /// authenticator, where the attestation statement is empty and the
    /// relying party learns the credential public key from the attested
    /// credential data itself.
    pub fn attestation_object(&self) -> Vec<u8> {
        let auth_data = self.encode();
        Cbor::Map(vec![
            (Cbor::text("fmt"), Cbor::text("none")),
            (Cbor::text("attStmt"), Cbor::Map(Vec::new())),
            (Cbor::text("authData"), Cbor::Bytes(auth_data)),
        ])
        .encode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_fixed_prefix() {
        let data = AuthenticatorData {
            rp_id_hash: AuthenticatorData::rp_id_hash("example.com"),
            flags: FLAG_UP | FLAG_UV | FLAG_BE | FLAG_BS,
            sign_count: 7,
            attested_credential_data: None,
            extensions: None,
        };
        let encoded = data.encode();
        assert_eq!(encoded.len(), 37);
        assert_eq!(&encoded[32..33], &[FLAG_UP | FLAG_UV | FLAG_BE | FLAG_BS]);
        assert_eq!(&encoded[33..37], &7u32.to_be_bytes());
    }

    #[test]
    fn appends_attested_credential_data_when_the_at_flag_is_set() {
        let cose = Cbor::Map(vec![(Cbor::Uint(1), Cbor::Uint(2))]).encode();
        let data = AuthenticatorData {
            rp_id_hash: AuthenticatorData::rp_id_hash("example.com"),
            flags: FLAG_UP | FLAG_UV | FLAG_AT,
            sign_count: 0,
            attested_credential_data: Some(AttestedCredentialData::new(
                [0u8; 16],
                vec![9u8; 32],
                cose.clone(),
            )),
            extensions: None,
        };
        let encoded = data.encode();
        assert_eq!(encoded.len(), 37 + 16 + 2 + 32 + cose.len());
        assert_eq!(&encoded[37..53], &[0u8; 16]);
        assert_eq!(&encoded[53..55], &32u16.to_be_bytes());
        assert_eq!(&encoded[55..87], &[9u8; 32]);
        assert_eq!(&encoded[87..], &cose[..]);
    }

    #[test]
    fn attestation_object_is_a_cbor_map_with_fmt_none() {
        let data = AuthenticatorData {
            rp_id_hash: AuthenticatorData::rp_id_hash("example.com"),
            flags: FLAG_UP | FLAG_UV | FLAG_BE | FLAG_BS,
            sign_count: 0,
            attested_credential_data: None,
            extensions: None,
        };
        let object = data.attestation_object();
        // Map of three pairs.
        assert_eq!(object[0], 0xa3);
        let text = String::from_utf8_lossy(&object);
        assert!(text.contains("fmt"));
        assert!(text.contains("none"));
        assert!(text.contains("attStmt"));
        assert!(text.contains("authData"));
    }
}
