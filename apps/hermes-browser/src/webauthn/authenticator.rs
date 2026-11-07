//! The WebAuthn software authenticator: registration and assertion ceremonies.
//!
//! Ported from the iOS Equipped Browser `WebAuthnManager` create/get paths in
//! `WebAuthnManager.swift`. The Swift implementation signs with
//! `CryptoKit.P256.Signing.PrivateKey`; this port signs with OpenSSL ECDSA over
//! P-256 with SHA-256, producing the same DER-encoded signature format, which
//! is what a relying party verifies.

use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::webauthn::authdata::{
    AttestedCredentialData, AuthenticatorData, FLAG_AT, FLAG_BE, FLAG_BS, FLAG_ED, FLAG_UP, FLAG_UV,
};
use crate::webauthn::b64::b64url_encode;
use crate::webauthn::cbor::Cbor;
use crate::webauthn::clientdata::{client_data_json, CeremonyType};
use crate::webauthn::cose::{
    cose_key_from_pem, generate_p256_keypair, validate_p256_public_pem,
};
use crate::webauthn::largeblob;
use crate::webauthn::origin::{
    OriginValidation, ValidatedOrigin, validate_origin_and_effective_rp_id,
};
use crate::webauthn::prf;
use crate::webauthn::random;
use crate::webauthn::types::{
    cred_protect_level, AssertionCredential, AssertionResponse,
    CredPropsOutput, ExtensionInputs, ExtensionOutputs, LargeBlobOutputs, PrfOutputs,
    PublicKeyCredentialCreationOptions, PublicKeyCredentialRequestOptions, RegistrationCredential,
    RegistrationResponse, WebAuthnError,
};

/// The Equipped Browser AAGUID, ported from `WebAuthnManager.equippedAAGUID`.
/// The bytes spell "EQUIPPEDPASSKEY" followed by a version byte.
pub const EQUIPPED_AAGUID: [u8; 16] = [
    0x45, 0x51, 0x55, 0x49, 0x50, 0x50, 0x45, 0x44, 0x50, 0x41, 0x53, 0x53, 0x4b, 0x45, 0x59, 0x01,
];

/// The COSE algorithm identifier for ES256.
pub const COSE_ALG_ES256: i64 = -7;

/// The user verification method list reported when the `uvm` extension is
/// requested: user presence, internal user verification, and a zeroed entry.
fn uvm_output() -> Vec<Vec<i64>> {
    vec![vec![0x0000_0002, 0x0000_0004, 0x0000_0000]]
}

/// The minimum PIN length reported by the `minPinLength` extension.
const MIN_PIN_LENGTH: i64 = 6;

/// The largest challenge a relying party may send.
const MAX_CHALLENGE_LEN: usize = 1024;

/// A stored passkey, mirroring the iOS `StoredPasskeyRecord`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredPasskey {
    /// Raw credential ID.
    pub credential_id: Vec<u8>,
    /// The relying party identifier the credential is scoped to.
    pub relying_party_identifier: String,
    /// The user handle.
    pub user_id: Vec<u8>,
    /// Account identifier.
    pub user_name: String,
    /// Human-readable account name.
    pub user_display_name: String,
    /// COSE algorithm identifier. ES256 is -7.
    pub algorithm: i64,
    /// The private key in PKCS#8 PEM form.
    pub private_key_pem: String,
    /// The public key in SPKI PEM form.
    pub public_key_pem: String,
    /// Monotonic signature counter.
    pub sign_count: u32,
    /// Whether the credential is discoverable.
    pub is_resident_key: bool,
    /// The credential protection level (1, 2, or 3).
    pub cred_protect_level: i64,
    /// Whether the credential supports the `largeBlob` extension.
    pub large_blob_supported: bool,
    /// The `largeBlob` encryption key.
    pub large_blob_key: Vec<u8>,
    /// The sealed `largeBlob` payload.
    pub compressed_large_blob: Option<Vec<u8>>,
    /// The uncompressed length of the sealed `largeBlob` payload.
    pub large_blob_original_size: Option<usize>,
    /// The PRF secret.
    pub prf_secret: Vec<u8>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

impl StoredPasskey {
    /// The base64url credential ID, which is how WebAuthn identifies a
    /// credential to a relying party.
    pub fn credential_id_b64url(&self) -> String {
        b64url_encode(&self.credential_id)
    }
}

/// Storage for passkey records.
///
/// The iOS implementation keeps every record in one Keychain item; this trait
/// is the seam that lets the authenticator stay independent of where the
/// records live.
pub trait PasskeyStore {
    /// Returns every stored passkey.
    fn all(&self) -> Result<Vec<StoredPasskey>, String>;

    /// Inserts a new passkey.
    fn insert(&mut self, record: StoredPasskey) -> Result<(), String>;

    /// Replaces an existing passkey, matched by credential ID.
    fn update(&mut self, record: StoredPasskey) -> Result<(), String>;

    /// Removes a passkey by credential ID.
    fn delete(&mut self, credential_id: &[u8]) -> Result<(), String>;

    /// Returns the passkeys scoped to a relying party.
    fn for_rp(&self, rp_id: &str) -> Result<Vec<StoredPasskey>, String> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|p| p.relying_party_identifier.eq_ignore_ascii_case(rp_id))
            .collect())
    }

    /// Returns a passkey by its credential ID.
    fn by_credential_id(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<StoredPasskey>, String> {
        Ok(self
            .all()?
            .into_iter()
            .find(|p| p.credential_id == credential_id))
    }
}

/// Validates an origin for a ceremony, honouring a related origins check the
/// caller has already made.
///
/// The authenticator cannot run the related origins validation procedure
/// (WebAuthn Level 3 §5.11.1) itself: that procedure fetches the relying
/// party's `/.well-known/webauthn` document over the network, and a ceremony
/// must not block on the network while a page waits. When the origin is not a
/// suffix of the RP ID, the caller has to run the procedure first and say so
/// with `related_origin_validated`. Without that flag the origin is refused.
fn validated_origin(
    origin: &str,
    requested_rp_id: Option<&str>,
    cross_origin: bool,
    is_create: bool,
    related_origin_validated: bool,
) -> Result<ValidatedOrigin, WebAuthnError> {
    match validate_origin_and_effective_rp_id(origin, requested_rp_id, cross_origin, is_create)? {
        OriginValidation::Suffix(validated) => Ok(validated),
        OriginValidation::NeedsRelatedOrigins {
            validated,
            caller_origin,
            rp_id,
        } => {
            if !related_origin_validated {
                return Err(WebAuthnError::Security(format!(
                    "The origin '{caller_origin}' is not related to '{rp_id}'"
                )));
            }
            Ok(validated)
        }
    }
}

/// The software authenticator.
pub struct Authenticator {
    /// The AAGUID written into attested credential data.
    pub aaguid: [u8; 16],
}

impl Default for Authenticator {
    fn default() -> Self {
        Self {
            aaguid: EQUIPPED_AAGUID,
        }
    }
}

impl Authenticator {
    /// Creates an authenticator with the Equipped Browser AAGUID.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs a registration ceremony.
    ///
    /// Returns the credential the relying party receives and the stored
    /// record the authenticator keeps.
    ///
    /// `related_origin_validated` says the caller has already run the related
    /// origins validation procedure (WebAuthn Level 3 §5.11.1) and the
    /// relying party's well-known document accepted this origin. The
    /// authenticator cannot run that procedure itself, because it needs the
    /// network; without the flag, an origin that is not a suffix of the RP ID
    /// is refused.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        store: &mut dyn PasskeyStore,
        options: &PublicKeyCredentialCreationOptions,
        origin: &str,
        cross_origin: bool,
        top_origin: Option<&str>,
        now_iso: &str,
        related_origin_validated: bool,
    ) -> Result<(RegistrationCredential, StoredPasskey), WebAuthnError> {
        validate_challenge(&options.challenge)?;
        let validated = validated_origin(
            origin,
            Some(options.rp.id.as_str()),
            cross_origin,
            true,
            related_origin_validated,
        )?;
        let effective_rp_id = validated.effective_rp_id;

        // The relying party must accept ES256.
        let accepts_es256 = options.pub_key_cred_params.is_empty()
            || options
                .pub_key_cred_params
                .iter()
                .any(|p| p.alg == COSE_ALG_ES256 && p.kind == "public-key");
        if !accepts_es256 {
            return Err(WebAuthnError::NotSupported(
                "No supported public key algorithm was offered (ES256 is required)".to_string(),
            ));
        }

        // An excluded credential that this authenticator already holds means
        // the relying party is asking to replace an existing credential.
        for excluded in &options.exclude_credentials {
            if excluded.kind != "public-key" && !excluded.kind.is_empty() {
                continue;
            }
            if store.by_credential_id(&excluded.id)?.is_some() {
                return Err(WebAuthnError::InvalidState(
                    "A credential with this identifier is already registered".to_string(),
                ));
            }
        }

        let extensions = options.extensions.clone().unwrap_or_default();
        let is_resident_key = registration_resident_key(options);
        let cred_protect_level_value = registration_cred_protect_level(&extensions);

        let credential_id = random::bytes(32).map_err(WebAuthnError::unknown)?;
        let large_blob_key = random::bytes(32).map_err(WebAuthnError::unknown)?;
        let prf_secret = random::bytes(32).map_err(WebAuthnError::unknown)?;
        let (private_key_pem, public_key_pem) =
            generate_p256_keypair().map_err(WebAuthnError::unknown)?;

        // The key is checked before it is stored, so a malformed key can
        // never reach the relying party.
        validate_p256_public_pem(&public_key_pem).map_err(WebAuthnError::unknown)?;
        let cose_key = cose_key_from_pem(&public_key_pem).map_err(WebAuthnError::unknown)?;

        // The client data is hashed into the attestation, so build it first.
        let client_data = client_data_json(
            CeremonyType::Create,
            &options.challenge,
            &validated.normalized_origin,
            cross_origin,
            top_origin,
        );

        // Extension outputs and the CBOR extension block that accompanies them.
        let mut outputs = ExtensionOutputs::default();
        let mut cbor_extension_pairs: Vec<(Cbor, Cbor)> = Vec::new();

        if extensions.large_blob.is_some() {
            outputs.large_blob = Some(LargeBlobOutputs {
                supported: Some(true),
                ..Default::default()
            });
        }
        if extensions.large_blob_key == Some(true) {
            outputs.large_blob_key = Some(large_blob_key.clone());
        }
        if let Some(prf_input) = &extensions.prf {
            let results = match &prf_input.eval {
                Some(values) => Some(prf::evaluate(
                    &prf_secret,
                    &values.first,
                    values.second.as_deref(),
                )
                .map_err(WebAuthnError::unknown)?),
                None => None,
            };
            outputs.prf = Some(PrfOutputs {
                enabled: Some(true),
                results,
            });
            cbor_extension_pairs.push((Cbor::text("hmac-secret"), Cbor::Bool(true)));
        }
        if extensions.cred_props == Some(true) {
            outputs.cred_props = Some(CredPropsOutput {
                rk: is_resident_key,
            });
        }
        if extensions.cred_protect.is_some() {
            outputs.cred_protect = Some(cred_protect_level_value);
            cbor_extension_pairs.push((
                Cbor::text("credProtect"),
                Cbor::Uint(cred_protect_level_value as u64),
            ));
        }
        if extensions.min_pin_length == Some(true) {
            outputs.min_pin_length = Some(MIN_PIN_LENGTH);
            cbor_extension_pairs.push((
                Cbor::text("minPinLength"),
                Cbor::Uint(MIN_PIN_LENGTH as u64),
            ));
        }
        if extensions.uvm == Some(true) {
            outputs.uvm = Some(uvm_output());
        }

        let mut flags = FLAG_UP | FLAG_UV | FLAG_BE | FLAG_BS | FLAG_AT;
        let extensions_cbor = if cbor_extension_pairs.is_empty() {
            None
        } else {
            flags |= FLAG_ED;
            Some(Cbor::Map(cbor_extension_pairs).encode())
        };

        let authenticator_data = AuthenticatorData {
            rp_id_hash: AuthenticatorData::rp_id_hash(&effective_rp_id),
            flags,
            sign_count: 0,
            attested_credential_data: Some(AttestedCredentialData::new(
                self.aaguid,
                credential_id.clone(),
                cose_key,
            )),
            extensions: extensions_cbor,
        };

        let attestation_object = authenticator_data.attestation_object();

        // A resident credential replaces any earlier credential for the same
        // relying party and user handle.
        if is_resident_key {
            for existing in store.for_rp(&effective_rp_id)? {
                if existing.user_id == options.user.id {
                    store.delete(&existing.credential_id)?;
                }
            }
        }

        let attachment = if options
            .authenticator_selection
            .as_ref()
            .and_then(|s| s.authenticator_attachment.as_deref())
            == Some("cross-platform")
        {
            Some("cross-platform".to_string())
        } else {
            Some("platform".to_string())
        };

        let record = StoredPasskey {
            credential_id: credential_id.clone(),
            relying_party_identifier: effective_rp_id.clone(),
            user_id: options.user.id.clone(),
            user_name: options.user.name.clone(),
            user_display_name: options.user.display_name.clone(),
            algorithm: COSE_ALG_ES256,
            private_key_pem,
            public_key_pem,
            sign_count: 0,
            is_resident_key,
            cred_protect_level: cred_protect_level_value,
            large_blob_supported: true,
            large_blob_key,
            compressed_large_blob: None,
            large_blob_original_size: None,
            prf_secret,
            created_at: now_iso.to_string(),
        };
        store.insert(record.clone())?;

        let credential = RegistrationCredential {
            id: b64url_encode(&credential_id),
            raw_id: b64url_encode(&credential_id),
            kind: "public-key".to_string(),
            response: RegistrationResponse {
                attestation_object: b64url_encode(&attestation_object),
                client_data_json: b64url_encode(&client_data),
                transports: vec!["internal".to_string(), "hybrid".to_string()],
            },
            authenticator_attachment: attachment,
            client_extension_results: outputs,
        };

        Ok((credential, record))
    }

    /// Runs an assertion ceremony.
    ///
    /// Returns the assertion the relying party receives and the updated stored
    /// record.
    ///
    /// `related_origin_validated` says the caller has already run the related
    /// origins validation procedure (WebAuthn Level 3 §5.11.1) and the
    /// relying party's well-known document accepted this origin.
    #[allow(clippy::too_many_arguments)]
    pub fn get(
        &self,
        store: &mut dyn PasskeyStore,
        options: &PublicKeyCredentialRequestOptions,
        origin: &str,
        cross_origin: bool,
        top_origin: Option<&str>,
        related_origin_validated: bool,
    ) -> Result<(AssertionCredential, StoredPasskey), WebAuthnError> {
        validate_challenge(&options.challenge)?;
        let validated = validated_origin(
            origin,
            options.rp_id.as_deref(),
            cross_origin,
            false,
            related_origin_validated,
        )?;
        let effective_rp_id = validated.effective_rp_id;
        let extensions = options.extensions.clone().unwrap_or_default();

        let all = store.all()?;
        let appid = extensions.appid.as_deref();

        // Locate the credential. With an explicit allow list the credential ID
        // must match; without one the credential must be discoverable, and a
        // credential protected at level 2 or above cannot be used that way.
        let selected = if options.allow_credentials.is_empty() {
            all.iter().rev().find(|p| {
                p.relying_party_identifier.eq_ignore_ascii_case(&effective_rp_id)
                    && p.is_resident_key
                    && p.cred_protect_level < 2
            })
        } else {
            let allowed: Vec<&[u8]> = options
                .allow_credentials
                .iter()
                .filter(|c| c.kind == "public-key" || c.kind.is_empty())
                .map(|c| c.id.as_slice())
                .collect();
            all.iter().rev().find(|p| {
                let rp_matches = p
                    .relying_party_identifier
                    .eq_ignore_ascii_case(&effective_rp_id)
                    || appid.is_some_and(|a| p.relying_party_identifier.eq_ignore_ascii_case(a));
                rp_matches && allowed.contains(&p.credential_id.as_slice())
            })
        };

        let Some(mut record) = selected.cloned() else {
            return Err(WebAuthnError::NotAllowed(format!(
                "No matching passkey found for relying party {effective_rp_id}"
            )));
        };

        if record.cred_protect_level == 3 && options.user_verification.as_deref() == Some("discouraged")
        {
            return Err(WebAuthnError::NotAllowed(
                "Credential requires user verification (credProtect level 3)".to_string(),
            ));
        }

        // The signature counter is monotonic.
        record.sign_count = record.sign_count.wrapping_add(1);

        let mut outputs = ExtensionOutputs::default();

        if let Some(large_blob_input) = &extensions.large_blob {
            if large_blob_input.read == Some(true) {
                let blob = match (
                    record.large_blob_supported,
                    &record.compressed_large_blob,
                    record.large_blob_original_size,
                ) {
                    (true, Some(ciphertext), Some(original_size)) => Some(
                        largeblob::decrypt_and_decompress(
                            ciphertext,
                            original_size,
                            &record.large_blob_key,
                        )
                        .map_err(WebAuthnError::unknown)?,
                    ),
                    _ => None,
                };
                outputs.large_blob = Some(LargeBlobOutputs {
                    blob,
                    ..Default::default()
                });
            } else if let Some(write_data) = &large_blob_input.write {
                let written = if record.large_blob_supported
                    && write_data.len() <= largeblob::MAX_UNCOMPRESSED_BYTES
                {
                    match largeblob::compress_and_encrypt(write_data, &record.large_blob_key) {
                        Ok((ciphertext, original_size)) => {
                            record.compressed_large_blob = Some(ciphertext);
                            record.large_blob_original_size = Some(original_size);
                            true
                        }
                        Err(_) => false,
                    }
                } else {
                    false
                };
                outputs.large_blob = Some(LargeBlobOutputs {
                    written: Some(written),
                    ..Default::default()
                });
            }
        }

        if extensions.large_blob_key == Some(true) && record.large_blob_supported {
            outputs.large_blob_key = Some(record.large_blob_key.clone());
        }

        if let Some(prf_input) = &extensions.prf {
            let credential_id_b64 = record.credential_id_b64url();
            let chosen = prf_input
                .eval_by_credential
                .as_ref()
                .and_then(|by_credential| by_credential.get(&credential_id_b64))
                .or(prf_input.eval.as_ref());
            if let Some(values) = chosen {
                let evaluated = prf::evaluate(
                    &record.prf_secret,
                    &values.first,
                    values.second.as_deref(),
                )
                .map_err(WebAuthnError::unknown)?;
                outputs.prf = Some(PrfOutputs {
                    enabled: None,
                    results: Some(evaluated),
                });
            }
        }

        if let Some(appid) = appid {
            outputs.appid = Some(record.relying_party_identifier.eq_ignore_ascii_case(appid));
        }

        if extensions.uvm == Some(true) {
            outputs.uvm = Some(uvm_output());
        }

        let client_data = client_data_json(
            CeremonyType::Get,
            &options.challenge,
            &validated.normalized_origin,
            cross_origin,
            top_origin,
        );

        // The rpIdHash covers the credential's own relying party identifier,
        // matching the iOS implementation.
        let authenticator_data = AuthenticatorData {
            rp_id_hash: AuthenticatorData::rp_id_hash(&record.relying_party_identifier),
            flags: FLAG_UP | FLAG_UV | FLAG_BE | FLAG_BS,
            sign_count: record.sign_count,
            attested_credential_data: None,
            extensions: None,
        };
        let authenticator_data_bytes = authenticator_data.encode();

        let client_data_hash = Sha256::digest(&client_data);
        let mut signed_bytes = Vec::with_capacity(authenticator_data_bytes.len() + 32);
        signed_bytes.extend_from_slice(&authenticator_data_bytes);
        signed_bytes.extend_from_slice(&client_data_hash);

        let signature = sign_es256(&record.private_key_pem, &signed_bytes)?;

        store.update(record.clone())?;

        let attachment = if options
            .hints
            .iter()
            .any(|h| h == "security-key")
        {
            Some("cross-platform".to_string())
        } else {
            Some("platform".to_string())
        };

        let credential = AssertionCredential {
            id: b64url_encode(&record.credential_id),
            raw_id: b64url_encode(&record.credential_id),
            kind: "public-key".to_string(),
            response: AssertionResponse {
                authenticator_data: b64url_encode(&authenticator_data_bytes),
                client_data_json: b64url_encode(&client_data),
                signature: b64url_encode(&signature),
                user_handle: Some(b64url_encode(&record.user_id)),
            },
            authenticator_attachment: attachment,
            client_extension_results: outputs,
        };

        Ok((credential, record))
    }
}

/// Signs a payload with a P-256 private key, producing a DER-encoded ECDSA
/// signature over SHA-256 of the payload.
fn sign_es256(private_key_pem: &str, payload: &[u8]) -> Result<Vec<u8>, WebAuthnError> {
    let key = PKey::private_key_from_pem(private_key_pem.as_bytes())
        .map_err(|e| WebAuthnError::unknown(format!("parse private key: {e}")))?;
    let mut signer = Signer::new(MessageDigest::sha256(), &key)
        .map_err(|e| WebAuthnError::unknown(format!("signer: {e}")))?;
    signer
        .update(payload)
        .map_err(|e| WebAuthnError::unknown(format!("signer update: {e}")))?;
    signer
        .sign_to_vec()
        .map_err(|e| WebAuthnError::unknown(format!("sign: {e}")))
}

/// Checks the relying party's challenge.
///
/// The iOS implementation carried the full nine-case WebAuthn error set; the
/// desktop port raises the same `Type` and `Constraint` kinds here.
fn validate_challenge(challenge: &[u8]) -> Result<(), WebAuthnError> {
    if challenge.is_empty() {
        return Err(WebAuthnError::Type(
            "The challenge must not be empty".to_string(),
        ));
    }
    if challenge.len() > MAX_CHALLENGE_LEN {
        return Err(WebAuthnError::Constraint(format!(
            "The challenge must not exceed {MAX_CHALLENGE_LEN} bytes"
        )));
    }
    Ok(())
}

/// Whether a registration creates a discoverable (resident) credential.
///
/// Mirrors the iOS rule: `residentKey` other than "discouraged" makes the
/// credential resident, otherwise the legacy `requireResidentKey` flag decides,
/// and the default is resident.
fn registration_resident_key(options: &PublicKeyCredentialCreationOptions) -> bool {
    let Some(selection) = options.authenticator_selection.as_ref() else {
        return true;
    };
    match selection.resident_key.as_deref() {
        Some("discouraged") => selection.require_resident_key.unwrap_or(false),
        Some(_) => true,
        None => selection.require_resident_key.unwrap_or(true),
    }
}

/// The credential protection level for a registration.
///
/// Mirrors the iOS rule: the requested policy maps to 1, 2, or 3, and the
/// default is 1.
fn registration_cred_protect_level(extensions: &ExtensionInputs) -> i64 {
    extensions
        .cred_protect
        .as_deref()
        .and_then(cred_protect_level)
        .unwrap_or(1)
}

/// A `PasskeyStore` backed by an in-memory list.
///
/// Used by the authenticator's own tests and by the passkey test harness.
#[derive(Clone, Debug, Default)]
pub struct MemoryPasskeyStore {
    records: Vec<StoredPasskey>,
}

impl MemoryPasskeyStore {
    /// Creates an empty store.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }
}

impl PasskeyStore for MemoryPasskeyStore {
    fn all(&self) -> Result<Vec<StoredPasskey>, String> {
        Ok(self.records.clone())
    }

    fn insert(&mut self, record: StoredPasskey) -> Result<(), String> {
        self.records.push(record);
        Ok(())
    }

    fn update(&mut self, record: StoredPasskey) -> Result<(), String> {
        if let Some(existing) = self
            .records
            .iter_mut()
            .find(|p| p.credential_id == record.credential_id)
        {
            *existing = record;
            Ok(())
        } else {
            Err("no such passkey".to_string())
        }
    }

    fn delete(&mut self, credential_id: &[u8]) -> Result<(), String> {
        self.records
            .retain(|p| p.credential_id != credential_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webauthn::types::{
        PublicKeyCredentialParameters, PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity,
    };

    fn creation_options(challenge: &[u8]) -> PublicKeyCredentialCreationOptions {
        PublicKeyCredentialCreationOptions {
            rp: PublicKeyCredentialRpEntity {
                id: "example.com".to_string(),
                name: Some("Example".to_string()),
            },
            user: PublicKeyCredentialUserEntity {
                id: b"user-handle-1".to_vec(),
                name: "ada@example.com".to_string(),
                display_name: "Ada Lovelace".to_string(),
            },
            challenge: challenge.to_vec(),
            pub_key_cred_params: vec![PublicKeyCredentialParameters {
                alg: COSE_ALG_ES256,
                kind: "public-key".to_string(),
            }],
            timeout: None,
            exclude_credentials: Vec::new(),
            authenticator_selection: None,
            attestation: Some("none".to_string()),
            extensions: None,
        }
    }

    fn request_options(challenge: &[u8]) -> PublicKeyCredentialRequestOptions {
        PublicKeyCredentialRequestOptions {
            challenge: challenge.to_vec(),
            rp_id: Some("example.com".to_string()),
            timeout: None,
            allow_credentials: Vec::new(),
            user_verification: Some("preferred".to_string()),
            extensions: None,
            hints: Vec::new(),
        }
    }

    #[test]
    fn registration_produces_a_verifiable_credential() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        let options = creation_options(b"challenge-1");
        let (credential, record) = authenticator
            .create(
                &mut store,
                &options,
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();

        assert_eq!(credential.kind, "public-key");
        assert_eq!(credential.id, record.credential_id_b64url());
        assert!(!credential.response.attestation_object.is_empty());
        assert!(!credential.response.client_data_json.is_empty());
        assert_eq!(store.all().unwrap().len(), 1);
        assert_eq!(record.sign_count, 0);
        assert!(record.is_resident_key);
        assert_eq!(record.cred_protect_level, 1);
    }

    #[test]
    fn assertion_signature_verifies_against_the_stored_public_key() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        let (credential, _) = authenticator
            .create(
                &mut store,
                &creation_options(b"challenge-1"),
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();

        let (assertion, record) = authenticator
            .get(
                &mut store,
                &request_options(b"challenge-2"),
                "https://example.com",
                false,
                None,
                false,
            )
            .unwrap();

        // Recompute what the relying party verifies.
        let auth_data = crate::webauthn::b64::b64url_decode(&assertion.response.authenticator_data)
            .unwrap();
        let client_data =
            crate::webauthn::b64::b64url_decode(&assertion.response.client_data_json).unwrap();
        let signature = crate::webauthn::b64::b64url_decode(&assertion.response.signature).unwrap();

        let client_data_hash = Sha256::digest(&client_data);
        let mut signed = auth_data.clone();
        signed.extend_from_slice(&client_data_hash);

        let key = PKey::public_key_from_pem(record.public_key_pem.as_bytes()).unwrap();
        let mut verifier =
            openssl::sign::Verifier::new(MessageDigest::sha256(), &key).unwrap();
        verifier.update(&signed).unwrap();
        assert!(verifier.verify(&signature).unwrap());

        // The client data carries the challenge and origin.
        let json: serde_json::Value = serde_json::from_slice(&client_data).unwrap();
        assert_eq!(json["type"], "webauthn.get");
        assert_eq!(
            json["challenge"],
            crate::webauthn::b64::b64url_encode(b"challenge-2")
        );
        assert_eq!(json["origin"], "https://example.com");
        assert_eq!(credential.kind, "public-key");
    }

    #[test]
    fn the_signature_counter_advances_monotonically() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        authenticator
            .create(
                &mut store,
                &creation_options(b"c1"),
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();
        let (_, first) = authenticator
            .get(
                &mut store,
                &request_options(b"c2"),
                "https://example.com",
                false,
                None,
                false,
            )
            .unwrap();
        let (_, second) = authenticator
            .get(
                &mut store,
                &request_options(b"c3"),
                "https://example.com",
                false,
                None,
                false,
            )
            .unwrap();
        assert_eq!(first.sign_count, 1);
        assert_eq!(second.sign_count, 2);
    }

    #[test]
    fn an_assertion_for_an_unknown_relying_party_fails() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        authenticator
            .create(
                &mut store,
                &creation_options(b"c1"),
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();
        let err = authenticator
            .get(
                &mut store,
                &request_options(b"c2"),
                "https://other.com",
                false,
                None,
                false,
            )
            .unwrap_err();
        assert_eq!(err.dom_exception_name(), "SecurityError");
    }

    #[test]
    fn an_excluded_credential_is_refused() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        let (credential, _) = authenticator
            .create(
                &mut store,
                &creation_options(b"c1"),
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();
        let mut options = creation_options(b"c2");
        options.exclude_credentials = vec![crate::webauthn::types::PublicKeyCredentialDescriptor {
            id: crate::webauthn::b64::b64url_decode(&credential.raw_id).unwrap(),
            transports: Vec::new(),
            rk: None,
            kind: "public-key".to_string(),
        }];
        let err = authenticator
            .create(
                &mut store,
                &options,
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap_err();
        assert_eq!(err.dom_exception_name(), "InvalidStateError");
    }

    #[test]
    fn the_prf_extension_evaluates_during_registration_and_assertion() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        let mut options = creation_options(b"c1");
        options.extensions = Some(ExtensionInputs {
            prf: Some(crate::webauthn::types::PrfInputs {
                eval: Some(crate::webauthn::types::PrfValues {
                    first: b"salt".to_vec(),
                    second: None,
                }),
                eval_by_credential: None,
            }),
            ..Default::default()
        });
        let (credential, _) = authenticator
            .create(
                &mut store,
                &options,
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();
        let prf_out = credential
            .client_extension_results
            .prf
            .as_ref()
            .expect("prf output");
        assert_eq!(prf_out.enabled, Some(true));
        assert!(prf_out.results.is_some());

        let mut request = request_options(b"c2");
        request.extensions = Some(ExtensionInputs {
            prf: Some(crate::webauthn::types::PrfInputs {
                eval: Some(crate::webauthn::types::PrfValues {
                    first: b"salt".to_vec(),
                    second: None,
                }),
                eval_by_credential: None,
            }),
            ..Default::default()
        });
        let (assertion, _) = authenticator
            .get(
                &mut store,
                &request,
                "https://example.com",
                false,
                None,
                false,
            )
            .unwrap();
        let results = assertion
            .client_extension_results
            .prf
            .as_ref()
            .and_then(|p| p.results.as_ref())
            .expect("prf results");
        // The same secret and salt must produce the same output.
        assert_eq!(results.first.len(), 32);
        assert_eq!(
            results.first,
            prf_out.results.as_ref().unwrap().first,
            "PRF output must be stable across ceremonies"
        );
    }

    #[test]
    fn the_large_blob_extension_round_trips_through_the_store() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        authenticator
            .create(
                &mut store,
                &creation_options(b"c1"),
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();

        let mut write = request_options(b"c2");
        write.extensions = Some(ExtensionInputs {
            large_blob: Some(crate::webauthn::types::LargeBlobInputs {
                support: Some("required".to_string()),
                read: None,
                write: Some(b"secret resident data".to_vec()),
            }),
            ..Default::default()
        });
        let (assertion, record) = authenticator
            .get(
                &mut store,
                &write,
                "https://example.com",
                false,
                None,
                false,
            )
            .unwrap();
        assert_eq!(
            assertion
                .client_extension_results
                .large_blob
                .as_ref()
                .and_then(|l| l.written),
            Some(true)
        );
        assert!(record.compressed_large_blob.is_some());

        let mut read = request_options(b"c3");
        read.extensions = Some(ExtensionInputs {
            large_blob: Some(crate::webauthn::types::LargeBlobInputs {
                support: None,
                read: Some(true),
                write: None,
            }),
            ..Default::default()
        });
        let (assertion, _) = authenticator
            .get(&mut store, &read, "https://example.com", false, None, false)
            .unwrap();
        let blob = assertion
            .client_extension_results
            .large_blob
            .as_ref()
            .and_then(|l| l.blob.clone())
            .expect("blob must be returned");
        assert_eq!(blob, b"secret resident data");
    }

    #[test]
    fn a_non_resident_credential_is_not_discoverable() {
        let mut store = MemoryPasskeyStore::new();
        let authenticator = Authenticator::new();
        let mut options = creation_options(b"c1");
        options.authenticator_selection = Some(crate::webauthn::types::AuthenticatorSelectionCriteria {
            resident_key: Some("discouraged".to_string()),
            require_resident_key: Some(false),
            ..Default::default()
        });
        let (_, record) = authenticator
            .create(
                &mut store,
                &options,
                "https://example.com",
                false,
                None,
                "2026-11-06T00:00:00Z",
                false,
            )
            .unwrap();
        assert!(!record.is_resident_key);

        // A discoverable lookup must therefore fail.
        let err = authenticator
            .get(
                &mut store,
                &request_options(b"c2"),
                "https://example.com",
                false,
                None,
                false,
            )
            .unwrap_err();
        assert_eq!(err.dom_exception_name(), "NotAllowedError");
    }
}
