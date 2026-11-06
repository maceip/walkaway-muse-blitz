//! COSE_Key encoding for EC2 / P-256 / ES256.
//!
//! Ported from the iOS Equipped Browser `coseKeyCBOR` construction in
//! `WebAuthnManager.swift`. The COSE labels are: 1 = kty (EC2 = 2),
//! 3 = alg (ES256 = -7), -1 = crv (P-256 = 1), -2 = x, -3 = y.

use openssl::bn::{BigNum, BigNumContext};
use openssl::ec::{EcGroup, EcPoint};
use openssl::nid::Nid;
use openssl::pkey::PKey;

use crate::webauthn::cbor::Cbor;

/// COSE key type: Elliptic Curve with x and y coordinates.
pub const COSE_KTY_EC2: u64 = 2;
/// COSE algorithm: ECDSA with SHA-256.
pub const COSE_ALG_ES256: i64 = -7;
/// COSE elliptic curve: NIST P-256.
pub const COSE_CRV_P256: u64 = 1;

/// Builds the COSE_Key map for a P-256 public key.
pub fn cose_ec2_p256(x: &[u8; 32], y: &[u8; 32]) -> Cbor {
    Cbor::Map(vec![
        (Cbor::Uint(1), Cbor::Uint(COSE_KTY_EC2)),
        (Cbor::Uint(3), Cbor::Neg(COSE_ALG_ES256)),
        (Cbor::Neg(-1), Cbor::Uint(COSE_CRV_P256)),
        (Cbor::Neg(-2), Cbor::Bytes(x.to_vec())),
        (Cbor::Neg(-3), Cbor::Bytes(y.to_vec())),
    ])
}

/// Extracts the affine x/y coordinates of a P-256 public key in PEM (SPKI) form.
pub fn public_key_xy_from_pem(pem: &str) -> Result<([u8; 32], [u8; 32]), String> {
    let pkey =
        PKey::public_key_from_pem(pem.as_bytes()).map_err(|e| format!("parse public key: {e}"))?;
    let ec = pkey
        .ec_key()
        .map_err(|e| format!("not an EC key: {e}"))?;
    let group = ec.group();
    let point = ec.public_key();
    let mut ctx = BigNumContext::new().map_err(|e| format!("bn context: {e}"))?;
    let mut x = BigNum::new().map_err(|e| format!("bn x: {e}"))?;
    let mut y = BigNum::new().map_err(|e| format!("bn y: {e}"))?;
    point
        .affine_coordinates_gfp(group, &mut x, &mut y, &mut ctx)
        .map_err(|e| format!("affine coordinates: {e}"))?;
    Ok((bn_to_32(&x)?, bn_to_32(&y)?))
}

/// Converts a big-endian `BigNum` to a fixed 32-byte array, left padded.
fn bn_to_32(value: &BigNum) -> Result<[u8; 32], String> {
    let bytes = value.to_vec();
    if bytes.len() > 32 {
        return Err("EC coordinate wider than 32 bytes".to_string());
    }
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(out)
}

/// Encodes a P-256 public key in PEM (SPKI) form as a COSE_Key CBOR map.
pub fn cose_key_from_pem(pem: &str) -> Result<Vec<u8>, String> {
    let (x, y) = public_key_xy_from_pem(pem)?;
    Ok(cose_ec2_p256(&x, &y).encode())
}

/// Generates a fresh NIST P-256 keypair and returns (private PEM, public PEM).
pub fn generate_p256_keypair() -> Result<(String, String), String> {
    let group =
        EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).map_err(|e| format!("ec group: {e}"))?;
    let key = openssl::ec::EcKey::generate(&group).map_err(|e| format!("ec keygen: {e}"))?;
    let pkey =
        PKey::from_ec_key(key).map_err(|e| format!("pkey from ec: {e}"))?;
    let private_pem = String::from_utf8(
        pkey.private_key_to_pem_pkcs8()
            .map_err(|e| format!("private pem: {e}"))?,
    )
    .map_err(|e| format!("private pem utf8: {e}"))?;
    let public_pem = String::from_utf8(
        pkey.public_key_to_pem()
            .map_err(|e| format!("public pem: {e}"))?,
    )
    .map_err(|e| format!("public pem utf8: {e}"))?;
    Ok((private_pem, public_pem))
}

/// Verifies that a PEM public key is a valid P-256 key on the expected curve.
pub fn validate_p256_public_pem(pem: &str) -> Result<(), String> {
    let pkey =
        PKey::public_key_from_pem(pem.as_bytes()).map_err(|e| format!("parse public key: {e}"))?;
    let ec = pkey
        .ec_key()
        .map_err(|e| format!("not an EC key: {e}"))?;
    let group = ec.group();
    if group.curve_name() != Some(Nid::X9_62_PRIME256V1) {
        return Err("public key is not on the P-256 curve".to_string());
    }
    let point = ec.public_key();
    let mut ctx = BigNumContext::new().map_err(|e| format!("bn context: {e}"))?;
    if !point
        .is_on_curve(group, &mut ctx)
        .map_err(|e| format!("point on curve: {e}"))?
    {
        return Err("public key point is not on the curve".to_string());
    }
    Ok(())
}

/// Builds an uncompressed EC point for the given affine coordinates.
pub fn point_from_xy(x: &[u8; 32], y: &[u8; 32]) -> Result<EcPoint, String> {
    let group =
        EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).map_err(|e| format!("ec group: {e}"))?;
    let mut ctx = BigNumContext::new().map_err(|e| format!("bn context: {e}"))?;
    let bx = BigNum::from_slice(x).map_err(|e| format!("bn x: {e}"))?;
    let by = BigNum::from_slice(y).map_err(|e| format!("bn y: {e}"))?;
    let mut point = EcPoint::new(&group).map_err(|e| format!("ec point: {e}"))?;
    point
        .set_affine_coordinates_gfp(&group, &bx, &by, &mut ctx)
        .map_err(|e| format!("set affine coordinates: {e}"))?;
    Ok(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_p256_keypair_and_reads_back_its_coordinates() {
        let (private_pem, public_pem) = generate_p256_keypair().unwrap();
        assert!(private_pem.contains("PRIVATE KEY"));
        assert!(public_pem.contains("PUBLIC KEY"));
        validate_p256_public_pem(&public_pem).unwrap();
        let (x, y) = public_key_xy_from_pem(&public_pem).unwrap();
        // The point must satisfy the curve equation on P-256.
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
        let point = point_from_xy(&x, &y).unwrap();
        let mut ctx = BigNumContext::new().unwrap();
        assert!(point.is_on_curve(&group, &mut ctx).unwrap());
        assert!(!point.is_infinity(&group));
        assert_eq!(group.degree(), 2);
    }

    #[test]
    fn cose_key_has_the_expected_labels_and_order() {
        let (_private_pem, public_pem) = generate_p256_keypair().unwrap();
        let encoded = cose_key_from_pem(&public_pem).unwrap();
        // Map with 5 pairs.
        assert_eq!(encoded[0], 0xa5);
        // Keys in deterministic order: 1, 3, -1, -2, -3.
        let mut keys = Vec::new();
        let mut i = 1usize;
        while keys.len() < 5 {
            let key = encoded[i];
            keys.push(key);
            // Skip the key and its value: both are short here (1 byte each for
            // the integers, 33 bytes for the byte strings).
            i += match key {
                0x20 | 0x21 | 0x22 => 1 + 33,
                _ => 1 + 1,
            };
        }
        assert_eq!(keys, vec![0x01, 0x03, 0x20, 0x21, 0x22]);
    }
}
