//! WebAuthn Level 2/3 authenticator for the Hermes browser.
//!
//! Ported from the iOS Equipped Browser authenticator in
//! `BrowserExample/CustomBrowserEngine/UIProcess/WebAuthnManager.swift`
//! (3,093 lines), which implements a software authenticator with the full
//! WebAuthn extension surface:
//!
//!   * RFC 8949 CBOR encoding for attestation objects, COSE keys, and
//!     extension outputs.
//!   * ES256 (NIST P-256 ECDSA with SHA-256) credential keys.
//!   * `clientDataJSON`, `authenticatorData`, and `attestationObject`
//!     construction.
//!   * The `largeBlob`, `prf`, `credProps`, `credProtect`, `appid`, `uvm`,
//!     `minPinLength`, and `largeBlobKey` extensions.
//!   * Origin and relying party identifier validation.
//!
//! The iOS build additionally reached Apple's platform authenticator and
//! hardware security keys through `AuthenticationServices`. Those two paths
//! are iOS-only Apple APIs; this port implements the software authenticator
//! path, which is the path the Equipped Browser project uses for its own
//! passkeys.

pub mod authdata;
pub mod b64;
pub mod cbor;
pub mod clientdata;
pub mod cose;
pub mod largeblob;
pub mod origin;
pub mod prf;
pub mod random;
pub mod types;

pub use authdata::{AttestedCredentialData, AuthenticatorData};
pub use b64::{b64url_decode, b64url_encode};
pub use clientdata::{client_data_json, CeremonyType};
pub use cose::{
    cose_ec2_p256, cose_key_from_pem, generate_p256_keypair, public_key_xy_from_pem,
    validate_p256_public_pem,
};
pub use origin::{validate_origin_and_effective_rp_id, ValidatedOrigin};
pub use types::{
    cred_protect_level, resident_key_required, AssertionCredential, AssertionResponse,
    AuthenticatorSelectionCriteria, CredPropsOutput, ExtensionInputs, ExtensionOutputs,
    LargeBlobInputs, LargeBlobOutputs, PrfInputs, PrfOutputs, PrfValues,
    PublicKeyCredentialCreationOptions, PublicKeyCredentialDescriptor,
    PublicKeyCredentialParameters, PublicKeyCredentialRequestOptions,
    PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity, RegistrationCredential,
    RegistrationResponse, WebAuthnError,
};
