//! W3C WebAuthn Level 2 and Level 3 data models.
//!
//! Ported from the iOS Equipped Browser `WebAuthnManager.swift` data model
//! section ("W3C WebAuthn Level 2 & Level 3 Data Models" and the extension
//! models around `largeBlob`, `prf`, `credProps`, `credProtect`, `appid`,
//! `uvm`, `minPinLength`, `largeBlobKey`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Errors raised by the authenticator, mapped to W3C DOMException names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebAuthnError {
    /// The request was not allowed.
    NotAllowed(String),
    /// The object is already registered.
    InvalidState(String),
    /// The operation is not supported.
    NotSupported(String),
    /// A security check failed.
    Security(String),
    /// The arguments were of the wrong type.
    Type(String),
    /// A constraint was violated.
    Constraint(String),
    /// The operation was aborted.
    Abort(String),
    /// Encoding or decoding failed.
    Encoding(String),
    /// An unclassified failure.
    Unknown(String),
}

impl WebAuthnError {
    /// The DOMException name this error maps to.
    pub fn dom_exception_name(&self) -> &'static str {
        match self {
            WebAuthnError::NotAllowed(_) => "NotAllowedError",
            WebAuthnError::InvalidState(_) => "InvalidStateError",
            WebAuthnError::NotSupported(_) => "NotSupportedError",
            WebAuthnError::Security(_) => "SecurityError",
            WebAuthnError::Type(_) => "TypeError",
            WebAuthnError::Constraint(_) => "ConstraintError",
            WebAuthnError::Abort(_) => "AbortError",
            WebAuthnError::Encoding(_) => "EncodingError",
            WebAuthnError::Unknown(_) => "UnknownError",
        }
    }

    /// The human-readable detail, without the DOMException prefix.
    pub fn detail(&self) -> &str {
        match self {
            WebAuthnError::NotAllowed(m)
            | WebAuthnError::InvalidState(m)
            | WebAuthnError::NotSupported(m)
            | WebAuthnError::Security(m)
            | WebAuthnError::Type(m)
            | WebAuthnError::Constraint(m)
            | WebAuthnError::Abort(m)
            | WebAuthnError::Encoding(m)
            | WebAuthnError::Unknown(m) => m,
        }
    }

    /// Builds an error from a plain message, classifying it as `UnknownError`.
    pub fn unknown(message: impl Into<String>) -> WebAuthnError {
        WebAuthnError::Unknown(message.into())
    }
}

impl std::fmt::Display for WebAuthnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.dom_exception_name(), self.detail())
    }
}

impl std::error::Error for WebAuthnError {}

/// `largeBlob` extension client inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeBlobInputs {
    /// "preferred" or "required".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<String>,
    /// Request a read of the stored blob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<bool>,
    /// Blob bytes to write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<Vec<u8>>,
}

/// `largeBlob` extension client outputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeBlobOutputs {
    /// Whether the authenticator supports large blobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported: Option<bool>,
    /// The decrypted blob bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<Vec<u8>>,
    /// Whether the write succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written: Option<bool>,
}

/// `prf` extension salt inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrfValues {
    /// First salt.
    #[serde(default)]
    pub first: Vec<u8>,
    /// Second salt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second: Option<Vec<u8>>,
}

/// `prf` extension client inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrfInputs {
    /// Salts to evaluate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval: Option<PrfValues>,
    /// Per-credential salts.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "evalByCredential"
    )]
    pub eval_by_credential: Option<HashMap<String, PrfValues>>,
}

/// `prf` extension client outputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrfOutputs {
    /// Whether PRF is enabled for the credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Evaluated outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<PrfValues>,
}

/// `credProps` extension output.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredPropsOutput {
    /// Whether the credential is a resident (discoverable) credential.
    #[serde(default)]
    pub rk: bool,
}

/// All WebAuthn client extension inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionInputs {
    /// `largeBlob` inputs.
    #[serde(default, rename = "largeBlob", skip_serializing_if = "Option::is_none")]
    pub large_blob: Option<LargeBlobInputs>,
    /// `prf` inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prf: Option<PrfInputs>,
    /// `credProps` request.
    #[serde(default, rename = "credProps", skip_serializing_if = "Option::is_none")]
    pub cred_props: Option<bool>,
    /// `credProtect` policy: "userVerificationRequired", "userVerificationOptional",
    /// or "userVerificationOptionalWithCredentialIDList".
    #[serde(default, rename = "credProtect", skip_serializing_if = "Option::is_none")]
    pub cred_protect: Option<String>,
    /// `enforceCredentialProtectionPolicy` request.
    #[serde(
        default,
        rename = "enforceCredentialProtectionPolicy",
        skip_serializing_if = "Option::is_none"
    )]
    pub enforce_credential_protection_policy: Option<bool>,
    /// `minPinLength` request.
    #[serde(default, rename = "minPinLength", skip_serializing_if = "Option::is_none")]
    pub min_pin_length: Option<bool>,
    /// `uvm` request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uvm: Option<bool>,
    /// `appid` (legacy FIDO AppID) extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appid: Option<String>,
    /// `appidExclude` (legacy) extension.
    #[serde(default, rename = "appidExclude", skip_serializing_if = "Option::is_none")]
    pub appid_exclude: Option<String>,
    /// `largeBlobKey` request.
    #[serde(default, rename = "largeBlobKey", skip_serializing_if = "Option::is_none")]
    pub large_blob_key: Option<bool>,
}

/// All WebAuthn client extension outputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionOutputs {
    /// `largeBlob` outputs.
    #[serde(default, rename = "largeBlob", skip_serializing_if = "Option::is_none")]
    pub large_blob: Option<LargeBlobOutputs>,
    /// `prf` outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prf: Option<PrfOutputs>,
    /// `credProps` outputs.
    #[serde(default, rename = "credProps", skip_serializing_if = "Option::is_none")]
    pub cred_props: Option<CredPropsOutput>,
    /// `credProtect` level that was applied.
    #[serde(default, rename = "credProtect", skip_serializing_if = "Option::is_none")]
    pub cred_protect: Option<i64>,
    /// Minimum PIN length supported.
    #[serde(default, rename = "minPinLength", skip_serializing_if = "Option::is_none")]
    pub min_pin_length: Option<i64>,
    /// User verification methods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uvm: Option<Vec<Vec<i64>>>,
    /// `appid` was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appid: Option<bool>,
    /// `appidExclude` was used.
    #[serde(default, rename = "appidExclude", skip_serializing_if = "Option::is_none")]
    pub appid_exclude: Option<bool>,
    /// The `largeBlobKey` material.
    #[serde(default, rename = "largeBlobKey", skip_serializing_if = "Option::is_none")]
    pub large_blob_key: Option<Vec<u8>>,
}

/// A credential descriptor for `excludeCredentials` / `allowCredentials`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyCredentialDescriptor {
    /// Raw credential ID.
    #[serde(default)]
    pub id: Vec<u8>,
    /// Supported transports, e.g. "internal", "hybrid", "usb", "nfc", "ble".
    #[serde(default)]
    pub transports: Vec<String>,
    /// Whether the credential is discoverable.
    #[serde(default, rename = "rk", skip_serializing_if = "Option::is_none")]
    pub rk: Option<bool>,
}

/// The relying party entity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyCredentialRpEntity {
    /// The relying party identifier.
    #[serde(default)]
    pub id: String,
    /// Human-readable relying party name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The user entity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyCredentialUserEntity {
    /// User handle.
    #[serde(default)]
    pub id: Vec<u8>,
    /// Account identifier, typically an email or username.
    #[serde(default)]
    pub name: String,
    /// Human-readable account name.
    #[serde(default, rename = "displayName")]
    pub display_name: String,
}

/// A requested credential type and algorithm.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyCredentialParameters {
    /// COSE algorithm identifier. ES256 is -7.
    #[serde(default)]
    pub alg: i64,
    /// Credential type. Always "public-key".
    #[serde(rename = "type", default)]
    pub kind: String,
}

/// `authenticatorSelection` criteria.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatorSelectionCriteria {
    /// "platform" or "cross-platform".
    #[serde(
        default,
        rename = "authenticatorAttachment",
        skip_serializing_if = "Option::is_none"
    )]
    pub authenticator_attachment: Option<String>,
    /// Legacy `requireResidentKey` flag.
    #[serde(
        default,
        rename = "requireResidentKey",
        skip_serializing_if = "Option::is_none"
    )]
    pub require_resident_key: Option<bool>,
    /// "required", "preferred", or "discouraged".
    #[serde(default, rename = "residentKey", skip_serializing_if = "Option::is_none")]
    pub resident_key: Option<String>,
    /// "required", "preferred", or "discouraged".
    #[serde(
        default,
        rename = "userVerification",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_verification: Option<String>,
}

/// `PublicKeyCredentialCreationOptions`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyCredentialCreationOptions {
    /// The relying party.
    #[serde(default)]
    pub rp: PublicKeyCredentialRpEntity,
    /// The user account.
    #[serde(default)]
    pub user: PublicKeyCredentialUserEntity,
    /// The challenge bytes.
    #[serde(default)]
    pub challenge: Vec<u8>,
    /// Accepted algorithms.
    #[serde(default, rename = "pubKeyCredParams")]
    pub pub_key_cred_params: Vec<PublicKeyCredentialParameters>,
    /// Timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Credentials the relying party does not want re-registered.
    #[serde(default, rename = "excludeCredentials")]
    pub exclude_credentials: Vec<PublicKeyCredentialDescriptor>,
    /// Authenticator selection criteria.
    #[serde(
        default,
        rename = "authenticatorSelection",
        skip_serializing_if = "Option::is_none"
    )]
    pub authenticator_selection: Option<AuthenticatorSelectionCriteria>,
    /// Attestation conveyance preference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
    /// Extension inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ExtensionInputs>,
}

/// `PublicKeyCredentialRequestOptions`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyCredentialRequestOptions {
    /// The challenge bytes.
    #[serde(default)]
    pub challenge: Vec<u8>,
    /// The requested relying party identifier.
    #[serde(default, rename = "rpId", skip_serializing_if = "Option::is_none")]
    pub rp_id: Option<String>,
    /// Timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Credentials the relying party will accept.
    #[serde(default, rename = "allowCredentials")]
    pub allow_credentials: Vec<PublicKeyCredentialDescriptor>,
    /// User verification requirement.
    #[serde(
        default,
        rename = "userVerification",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_verification: Option<String>,
    /// Extension inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ExtensionInputs>,
    /// Relying party hints, e.g. "security-key", "client-device", "hybrid".
    #[serde(default)]
    pub hints: Vec<String>,
}

/// The registration response returned to the caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationCredential {
    /// Base64url credential ID.
    pub id: String,
    /// Base64url credential ID (identical to `id`).
    pub raw_id: String,
    /// Always "public-key".
    #[serde(rename = "type")]
    pub kind: String,
    /// The attestation response.
    pub response: RegistrationResponse,
    /// The authenticator attachment.
    #[serde(
        default,
        rename = "authenticatorAttachment",
        skip_serializing_if = "Option::is_none"
    )]
    pub authenticator_attachment: Option<String>,
    /// Extension outputs.
    #[serde(default, rename = "clientExtensionResults")]
    pub client_extension_results: ExtensionOutputs,
}

/// The attestation response payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationResponse {
    /// Base64url attestation object.
    #[serde(rename = "attestationObject")]
    pub attestation_object: String,
    /// Base64url client data JSON.
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    /// Supported transports.
    #[serde(default)]
    pub transports: Vec<String>,
}

/// The assertion response returned to the caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionCredential {
    /// Base64url credential ID.
    pub id: String,
    /// Base64url credential ID (identical to `id`).
    pub raw_id: String,
    /// Always "public-key".
    #[serde(rename = "type")]
    pub kind: String,
    /// The assertion response.
    pub response: AssertionResponse,
    /// The authenticator attachment.
    #[serde(
        default,
        rename = "authenticatorAttachment",
        skip_serializing_if = "Option::is_none"
    )]
    pub authenticator_attachment: Option<String>,
    /// Extension outputs.
    #[serde(default, rename = "clientExtensionResults")]
    pub client_extension_results: ExtensionOutputs,
}

/// The assertion response payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionResponse {
    /// Base64url authenticator data.
    #[serde(rename = "authenticatorData")]
    pub authenticator_data: String,
    /// Base64url client data JSON.
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    /// Base64url DER-encoded ECDSA signature.
    pub signature: String,
    /// Base64url user handle.
    #[serde(default, rename = "userHandle", skip_serializing_if = "Option::is_none")]
    pub user_handle: Option<String>,
}

/// Maps a `credProtect` policy string to its numeric level.
///
/// Level 1: `userVerificationOptional`. Level 2:
/// `userVerificationOptionalWithCredentialIDList`. Level 3:
/// `userVerificationRequired`.
pub fn cred_protect_level(policy: &str) -> Option<i64> {
    match policy {
        "userVerificationOptional" => Some(1),
        "userVerificationOptionalWithCredentialIDList" => Some(2),
        "userVerificationRequired" => Some(3),
        _ => None,
    }
}

/// Maps a `residentKey` preference to a boolean.
pub fn resident_key_required(selection: Option<&AuthenticatorSelectionCriteria>) -> bool {
    let Some(selection) = selection else {
        return false;
    };
    match selection.resident_key.as_deref() {
        Some("required") => true,
        Some("preferred") | Some("discouraged") | None => {
            selection.require_resident_key.unwrap_or(false)
        }
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_names_match_the_dom_exception_names() {
        assert_eq!(
            WebAuthnError::NotAllowed("x".into()).dom_exception_name(),
            "NotAllowedError"
        );
        assert_eq!(
            WebAuthnError::Security("x".into()).dom_exception_name(),
            "SecurityError"
        );
        assert_eq!(
            WebAuthnError::InvalidState("x".into()).dom_exception_name(),
            "InvalidStateError"
        );
    }

    #[test]
    fn cred_protect_levels_follow_the_spec() {
        assert_eq!(cred_protect_level("userVerificationOptional"), Some(1));
        assert_eq!(
            cred_protect_level("userVerificationOptionalWithCredentialIDList"),
            Some(2)
        );
        assert_eq!(cred_protect_level("userVerificationRequired"), Some(3));
        assert_eq!(cred_protect_level("nonsense"), None);
    }

    #[test]
    fn resident_key_required_reads_both_the_modern_and_legacy_flags() {
        let mut selection = AuthenticatorSelectionCriteria::default();
        assert!(!resident_key_required(None));
        assert!(!resident_key_required(Some(&selection)));
        selection.resident_key = Some("required".into());
        assert!(resident_key_required(Some(&selection)));
        selection.resident_key = Some("preferred".into());
        selection.require_resident_key = Some(true);
        assert!(resident_key_required(Some(&selection)));
    }

    #[test]
    fn extension_inputs_use_the_spec_member_names() {
        let json = serde_json::json!({
            "largeBlob": {"support": "preferred", "read": true},
            "prf": {"eval": {"first": [1, 2, 3]}},
            "credProps": true,
            "credProtect": "userVerificationRequired",
            "appidExclude": "https://example.com/appid.json",
            "largeBlobKey": true
        });
        let parsed: ExtensionInputs = serde_json::from_value(json).unwrap();
        assert_eq!(
            parsed.large_blob.as_ref().unwrap().support.as_deref(),
            Some("preferred")
        );
        assert!(parsed.cred_props.unwrap());
        assert_eq!(
            parsed.cred_protect.as_deref(),
            Some("userVerificationRequired")
        );
        assert_eq!(
            parsed.appid_exclude.as_deref(),
            Some("https://example.com/appid.json")
        );
        assert!(parsed.large_blob_key.unwrap());
    }

    #[test]
    fn extension_outputs_use_the_spec_member_names() {
        let outputs = ExtensionOutputs {
            cred_props: Some(CredPropsOutput { rk: true }),
            cred_protect: Some(2),
            ..Default::default()
        };
        let value = serde_json::to_value(&outputs).unwrap();
        assert_eq!(value["credProps"]["rk"], serde_json::json!(true));
        assert_eq!(value["credProtect"], serde_json::json!(2));
    }
}
