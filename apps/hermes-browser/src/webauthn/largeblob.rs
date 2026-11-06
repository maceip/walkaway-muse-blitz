//! CTAP2.1 `largeBlob` storage: DEFLATE compression plus AES-256-GCM sealing.
//!
//! Ported from the iOS Equipped Browser `LargeBlobCodec` in
//! `WebAuthnManager.swift`. The blob format matches the Swift implementation
//! byte for byte:
//!   compressed = zlib(plaintext)          (RFC 1950, as produced by Apple's
//!                                           `NSData.compressed(using: .zlib)`)
//!   stored    = nonce(12) || ciphertext || tag(16)
//! where the nonce and tag are those produced by Swift's `AES.GCM.seal(...).combined`.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::write::{ZlibDecoder, ZlibEncoder};
use openssl::symm::{Cipher, decrypt_aead, encrypt_aead};

/// Maximum uncompressed `largeBlob` size, matching the Swift constant.
pub const MAX_UNCOMPRESSED_BYTES: usize = 65_536;

/// AES-GCM nonce length in bytes.
const NONCE_LEN: usize = 12;
/// AES-GCM authentication tag length in bytes.
const TAG_LEN: usize = 16;

/// Compresses and encrypts a large blob for storage.
///
/// Returns the sealed blob and the original uncompressed length.
pub fn compress_and_encrypt(
    plain_data: &[u8],
    key: &[u8],
) -> Result<(Vec<u8>, usize), String> {
    if plain_data.len() > MAX_UNCOMPRESSED_BYTES {
        return Err(format!(
            "largeBlob write data exceeds maximum allowed size ({MAX_UNCOMPRESSED_BYTES} bytes)"
        ));
    }
    if key.len() != 32 {
        return Err(format!(
            "largeBlob key must be 32 bytes, got {}",
            key.len()
        ));
    }
    let compressed = compress_deflate(plain_data)?;
    let nonce = random_bytes(NONCE_LEN)?;
    let mut tag = vec![0u8; TAG_LEN];
    let ciphertext = encrypt_aead(
        Cipher::aes_256_gcm(),
        key,
        Some(&nonce),
        &[],
        &compressed,
        &mut tag,
    )
    .map_err(|e| format!("largeBlob AES-GCM seal: {e}"))?;

    let mut stored = Vec::with_capacity(nonce.len() + ciphertext.len() + tag.len());
    stored.extend_from_slice(&nonce);
    stored.extend_from_slice(&ciphertext);
    stored.extend_from_slice(&tag);
    Ok((stored, plain_data.len()))
}

/// Decrypts and decompresses a stored large blob.
pub fn decrypt_and_decompress(
    stored: &[u8],
    uncompressed_size: usize,
    key: &[u8],
) -> Result<Vec<u8>, String> {
    if key.len() != 32 {
        return Err(format!("largeBlob key must be 32 bytes, got {}", key.len()));
    }
    if stored.len() < NONCE_LEN + TAG_LEN {
        return Err("largeBlob ciphertext is too short".to_string());
    }
    let (nonce, rest) = stored.split_at(NONCE_LEN);
    let (ciphertext, tag) = rest.split_at(rest.len() - TAG_LEN);
    let compressed = decrypt_aead(
        Cipher::aes_256_gcm(),
        key,
        Some(nonce),
        &[],
        ciphertext,
        tag,
    )
    .map_err(|e| format!("largeBlob AES-GCM open: {e}"))?;
    decompress_deflate(&compressed, uncompressed_size)
}

/// Compresses data with zlib (RFC 1950) framing.
pub fn compress_deflate(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        // Matches the Swift helper, which short-circuits empty input.
        return Ok(Vec::new());
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| format!("largeBlob deflate: {e}"))?;
    encoder
        .finish()
        .map_err(|e| format!("largeBlob deflate finish: {e}"))
}

/// Decompresses zlib-framed data and checks the result against the expected size.
pub fn decompress_deflate(data: &[u8], expected_size: usize) -> Result<Vec<u8>, String> {
    if expected_size == 0 && data.is_empty() {
        return Ok(Vec::new());
    }
    let mut decoder = ZlibDecoder::new(Vec::new());
    decoder
        .write_all(data)
        .map_err(|e| format!("largeBlob inflate: {e}"))?;
    let out = decoder
        .finish()
        .map_err(|e| format!("largeBlob inflate finish: {e}"))?;
    if out.len() != expected_size {
        return Err(format!(
            "Decompressed largeBlob size mismatch ({} != {expected_size})",
            out.len()
        ));
    }
    Ok(out)
}

/// Reads `count` cryptographically random bytes from the system entropy source.
pub fn random_bytes(count: usize) -> Result<Vec<u8>, String> {
    crate::webauthn::random::bytes(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_blob() {
        let key = vec![7u8; 32];
        let plaintext = b"resident credential blob payload".to_vec();
        let (stored, size) = compress_and_encrypt(&plaintext, &key).unwrap();
        assert_eq!(size, plaintext.len());
        // nonce + ciphertext + tag
        assert_eq!(stored.len(), NONCE_LEN + plaintext.len() + TAG_LEN);
        let recovered = decrypt_and_decompress(&stored, size, &key).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn round_trips_an_empty_blob() {
        let key = vec![3u8; 32];
        let (stored, size) = compress_and_encrypt(&[], &key).unwrap();
        assert_eq!(size, 0);
        let recovered = decrypt_and_decompress(&stored, size, &key).unwrap();
        assert!(recovered.is_empty());
    }

    #[test]
    fn uses_a_fresh_nonce_per_write() {
        let key = vec![1u8; 32];
        let (a, _) = compress_and_encrypt(b"same plaintext", &key).unwrap();
        let (b, _) = compress_and_encrypt(b"same plaintext", &key).unwrap();
        assert_ne!(a, b, "identical plaintext must not produce identical blobs");
    }

    #[test]
    fn rejects_a_blob_sealed_with_a_different_key() {
        let (stored, size) = compress_and_encrypt(b"payload", &[1u8; 32]).unwrap();
        let err = decrypt_and_decompress(&stored, size, &[2u8; 32]).unwrap_err();
        assert!(err.contains("AES-GCM"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_a_blob_whose_size_does_not_match() {
        let key = vec![5u8; 32];
        let (stored, _) = compress_and_encrypt(b"payload", &key).unwrap();
        let err = decrypt_and_decompress(&stored, 999, &key).unwrap_err();
        assert!(err.contains("size mismatch"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_oversized_blobs() {
        let key = vec![9u8; 32];
        let oversized = vec![0u8; MAX_UNCOMPRESSED_BYTES + 1];
        let err = compress_and_encrypt(&oversized, &key).unwrap_err();
        assert!(err.contains("exceeds maximum allowed size"));
    }

    #[test]
    fn compresses_repetitive_data() {
        let data = vec![0x41u8; 4096];
        let compressed = compress_deflate(&data).unwrap();
        assert!(compressed.len() < data.len() / 4);
        let restored = decompress_deflate(&compressed, data.len()).unwrap();
        assert_eq!(restored, data);
    }
}
