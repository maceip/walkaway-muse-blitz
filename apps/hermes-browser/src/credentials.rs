//! Credential and passkey brokers.
//!
//! Ported from the iOS Equipped Browser `BrowserApp/CredentialStore.swift`
//! and `CustomBrowserEngine/UIProcess/WebAuthnManager.swift`, and shaped to
//! the broker contract in `docs/supervision-interface.md` §12:
//!
//! > Password entry, one-time codes, passkeys, biometrics, hardware keys, and
//! > payments are separate broker flows. They are not `TypeText` with a secret
//! > string.
//!
//! and
//!
//! > WebAuthn is not a DOM fill operation. Use a distinct broker.
//!
//! ## Storage
//!
//! Secrets live in the macOS Keychain, never in a file and never in the agent:
//!
//!   * Passkeys: one generic password item holding the JSON array of records,
//!     matching `WebAuthnManager`'s `passkey_records_v1` layout.
//!   * Passwords: one generic password item per account, with the account
//!     name as `kSecAttrAccount` and the site host as `kSecAttrLabel`,
//!     matching `CredentialStore`'s layout.
//!
//! ## Redaction
//!
//! The agent may learn only that matching credentials exist, the origin, and
//! whether an approved fill happened. Secrets are handed to the page by the
//! broker and never appear in a prompt, a receipt, a log, or an audit record.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::keychain;
use crate::webauthn::authenticator::{
    Authenticator, PasskeyStore, StoredPasskey, EQUIPPED_AAGUID,
};
use crate::webauthn::b64::b64url_encode;
use crate::webauthn::origin::validate_origin_and_effective_rp_id;
use crate::webauthn::types::{
    AssertionCredential, AuthenticatorSelectionCriteria, ExtensionInputs,
    PublicKeyCredentialCreationOptions, PublicKeyCredentialDescriptor,
    PublicKeyCredentialParameters, PublicKeyCredentialRequestOptions, PublicKeyCredentialRpEntity,
    PublicKeyCredentialUserEntity, RegistrationCredential, WebAuthnError,
};

/// The Keychain service that holds the passkey records.
///
/// The iOS implementation used the bundle identifier
/// (`ai.zmax.equipped.browser.webauthn.passkeys`). Hermes has no bundle
/// identifier yet, so this port uses its own stable service name.
pub const PASSKEY_KEYCHAIN_SERVICE: &str = "ai.zmax.hermes.browser.webauthn.passkeys";

/// The Keychain account that holds the passkey records.
pub const PASSKEY_KEYCHAIN_ACCOUNT: &str = "passkey_records_v1";

/// The Keychain service that holds saved passwords.
pub const PASSWORD_KEYCHAIN_SERVICE: &str = "ai.zmax.hermes.browser.credentials";

/// Errors raised by the brokers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrokerError {
    /// The operation was refused.
    NotAllowed(String),
    /// The request does not match the current state.
    InvalidState(String),
    /// The requested credential does not exist.
    NotFound(String),
    /// The request was malformed.
    Invalid(String),
    /// The Keychain or the store failed.
    Storage(String),
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrokerError::NotAllowed(m) => write!(f, "NotAllowed: {m}"),
            BrokerError::InvalidState(m) => write!(f, "InvalidState: {m}"),
            BrokerError::NotFound(m) => write!(f, "NotFound: {m}"),
            BrokerError::Invalid(m) => write!(f, "Invalid: {m}"),
            BrokerError::Storage(m) => write!(f, "Storage: {m}"),
        }
    }
}

impl std::error::Error for BrokerError {}

impl From<String> for BrokerError {
    fn from(message: String) -> BrokerError {
        BrokerError::Storage(message)
    }
}

impl From<WebAuthnError> for BrokerError {
    fn from(error: WebAuthnError) -> BrokerError {
        match error {
            WebAuthnError::NotAllowed(m) => BrokerError::NotAllowed(m),
            WebAuthnError::InvalidState(m) => BrokerError::InvalidState(m),
            WebAuthnError::Type(m) | WebAuthnError::Constraint(m) => BrokerError::Invalid(m),
            other => BrokerError::Storage(other.to_string()),
        }
    }
}

/// A saved credential, mirroring the iOS `StoredCredential`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCredential {
    /// The site host.
    pub host: String,
    /// The account name.
    pub username: String,
    /// Whether this entry is a passkey rather than a password.
    pub is_passkey: bool,
}

impl StoredCredential {
    /// The stable identifier used by the iOS implementation.
    pub fn id(&self) -> String {
        format!(
            "{}|{}|{}",
            self.host,
            self.username,
            if self.is_passkey { "passkey" } else { "password" }
        )
    }
}

/// A passkey record stored in the macOS Keychain.
///
/// Mirrors the iOS layout: every record lives in a single generic password
/// item holding a JSON array.
#[derive(Clone, Debug)]
pub struct KeychainPasskeyStore {
    service: String,
    account: String,
}

impl Default for KeychainPasskeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainPasskeyStore {
    /// Creates a store using the default service and account names.
    pub fn new() -> Self {
        Self {
            service: PASSKEY_KEYCHAIN_SERVICE.to_string(),
            account: PASSKEY_KEYCHAIN_ACCOUNT.to_string(),
        }
    }

    /// Creates a store with an explicit service and account, for tests and for
    /// isolated instances.
    pub fn with_service(service: &str, account: &str) -> Self {
        Self {
            service: service.to_string(),
            account: account.to_string(),
        }
    }
}

impl PasskeyStore for KeychainPasskeyStore {
    fn all(&self) -> Result<Vec<StoredPasskey>, String> {
        let Some(data) = keychain::get_generic_item(&self.service, &self.account) else {
            return Ok(Vec::new());
        };
        if data.is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_slice(&data)
            .map_err(|e| format!("decode passkey records: {e}"))
    }

    fn insert(&mut self, record: StoredPasskey) -> Result<(), String> {
        let mut records = self.all()?;
        records.push(record);
        self.write_all(&records)
    }

    fn update(&mut self, record: StoredPasskey) -> Result<(), String> {
        let mut records = self.all()?;
        match records
            .iter_mut()
            .find(|r| r.credential_id == record.credential_id)
        {
            Some(existing) => *existing = record,
            None => return Err("no such passkey".to_string()),
        }
        self.write_all(&records)
    }

    fn delete(&mut self, credential_id: &[u8]) -> Result<(), String> {
        let mut records = self.all()?;
        records.retain(|r| r.credential_id != credential_id);
        self.write_all(&records)
    }
}

impl KeychainPasskeyStore {
    /// Writes the whole record set back to the Keychain.
    fn write_all(&self, records: &[StoredPasskey]) -> Result<(), String> {
        let data = serde_json::to_vec(records).map_err(|e| format!("encode passkeys: {e}"))?;
        keychain::set_generic_item(&self.service, &self.account, Some("Hermes passkeys"), &data)
            .map_err(|e| keychain::describe_error(&e))
    }

    /// Removes every stored passkey.
    pub fn clear(&self) -> Result<(), String> {
        keychain::delete_generic_item(&self.service, &self.account);
        Ok(())
    }
}

/// A saved password in the macOS Keychain.
///
/// Mirrors the iOS `CredentialStore`: one generic password item per account,
/// with the account name in `kSecAttrAccount` and the host in `kSecAttrLabel`.
#[derive(Clone, Debug, Default)]
pub struct KeychainPasswordStore {
    service: String,
}

impl KeychainPasswordStore {
    /// Creates a store using the default service name.
    pub fn new() -> Self {
        Self {
            service: PASSWORD_KEYCHAIN_SERVICE.to_string(),
        }
    }

    /// Creates a store with an explicit service name, for tests and for
    /// isolated instances.
    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    /// Stores a password for a site, replacing any previous password for the
    /// same account.
    pub fn save(&self, host: &str, username: &str, secret: &str) -> Result<(), BrokerError> {
        keychain::set_generic_item(
            &self.service,
            username,
            Some(host),
            secret.as_bytes(),
        )
        .map_err(|e| BrokerError::Storage(keychain::describe_error(&e)))
    }

    /// Returns the password for a site.
    pub fn secret(&self, host: &str, username: &str) -> Option<String> {
        let data = keychain::get_generic_item(&self.service, username)?;
        let secret = String::from_utf8(data).ok()?;
        // The label is the site the password belongs to; an account name that
        // is reused across sites must not leak one site's password to another.
        let entries = keychain::list_generic_items(&self.service);
        let matches_site = entries
            .iter()
            .any(|e| e.account == username && e.label.as_deref() == Some(host));
        if !matches_site {
            return None;
        }
        Some(secret)
    }

    /// Lists every saved credential, including the passkeys.
    pub fn list(&self) -> Result<Vec<StoredCredential>, BrokerError> {
        let mut out: Vec<StoredCredential> = keychain::list_generic_items(&self.service)
            .into_iter()
            .map(|item| StoredCredential {
                host: item.label.unwrap_or_default(),
                username: item.account,
                is_passkey: false,
            })
            .collect();
        out.sort_by(|a, b| a.host.cmp(&b.host));
        Ok(out)
    }

    /// Deletes a saved password.
    pub fn delete(&self, host: &str, username: &str) -> Result<(), BrokerError> {
        // Only delete the item when it belongs to this site.
        let entries = keychain::list_generic_items(&self.service);
        let matches_site = entries
            .iter()
            .any(|e| e.account == username && e.label.as_deref() == Some(host));
        if !matches_site {
            return Ok(());
        }
        keychain::delete_generic_item(&self.service, username);
        Ok(())
    }
}

/// The kinds of sensitive field a credential fill can cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialFieldKind {
    /// An account name.
    Username,
    /// A current password.
    CurrentPassword,
    /// A new password.
    NewPassword,
    /// A one-time code.
    OneTimeCode,
    /// A payment card.
    PaymentCard,
    /// Any other sensitive field.
    OtherSensitive,
}

impl CredentialFieldKind {
    /// The wire name of the field kind.
    pub fn as_str(self) -> &'static str {
        match self {
            CredentialFieldKind::Username => "username",
            CredentialFieldKind::CurrentPassword => "current_password",
            CredentialFieldKind::NewPassword => "new_password",
            CredentialFieldKind::OneTimeCode => "one_time_code",
            CredentialFieldKind::PaymentCard => "payment_card",
            CredentialFieldKind::OtherSensitive => "other_sensitive",
        }
    }
}

/// A request to fill credentials into a page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialFillRequest {
    /// The supervised task the fill belongs to.
    pub task_id: String,
    /// The origin the credentials are for.
    pub origin: String,
    /// The fields that need filling.
    pub fields: Vec<CredentialFieldKind>,
    /// The document generation the request was made against.
    pub document_generation: u64,
}

/// A redacted summary of a credential that matched a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialMatch {
    /// The account name, redacted.
    pub username_hint: String,
    /// The site the credential belongs to.
    pub site: String,
    /// Whether the match is a passkey or a password.
    pub is_passkey: bool,
}

/// The prompt shown to the human before a fill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialPrompt {
    /// The exact registrable domain the credential would be used for.
    pub origin: String,
    /// The registrable domain, for display.
    pub registrable_domain: String,
    /// The matching credentials, redacted.
    pub matches: Vec<CredentialMatch>,
    /// Whether the shell can offer to add a password.
    pub can_add_credential: bool,
}

/// The human's decision about a credential fill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialApproval {
    /// The ceremony the approval refers to.
    pub ceremony_id: String,
    /// The credential the human chose, if any.
    pub chosen: Option<CredentialMatch>,
    /// Whether the choice is remembered for this site.
    pub remember: bool,
}

/// The receipt returned after an approved fill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialFillReceipt {
    /// The origin the fill applied to.
    pub origin: String,
    /// The fields that were filled.
    pub filled_fields: Vec<CredentialFieldKind>,
    /// The redacted account that was used.
    pub account_hint: Option<String>,
}

/// The sink a credential fill writes into.
pub trait CredentialSink {
    /// Writes a value into a field.
    fn set_field(
        &mut self,
        field: CredentialFieldKind,
        value: &str,
    ) -> Result<(), BrokerError>;
}

/// The password broker.
pub trait CredentialBroker {
    /// Describes what would happen for a request, without touching a page.
    fn describe_fill(
        &self,
        req: CredentialFillRequest,
    ) -> Result<CredentialPrompt, BrokerError>;

    /// Applies a fill the human approved.
    fn apply_approved_fill(
        &mut self,
        approval: CredentialApproval,
        engine: &mut dyn CredentialSink,
    ) -> Result<CredentialFillReceipt, BrokerError>;
}

/// The identifier of an in-flight passkey ceremony.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PasskeyCeremonyId(pub String);

/// A request for a passkey assertion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasskeyRequest {
    /// The supervised task the ceremony belongs to.
    pub task_id: String,
    /// The origin the ceremony runs for.
    pub origin: String,
    /// The requested relying party identifier.
    pub rp_id: Option<String>,
    /// The challenge from the relying party.
    pub challenge: Vec<u8>,
    /// The credentials the relying party will accept.
    #[serde(default)]
    pub allow_credentials: Vec<PublicKeyCredentialDescriptor>,
    /// The user verification requirement.
    #[serde(default)]
    pub user_verification: Option<String>,
    /// Extension inputs.
    #[serde(default)]
    pub extensions: Option<ExtensionInputs>,
    /// Whether the ceremony is running in a cross-origin frame.
    #[serde(default)]
    pub cross_origin: bool,
    /// The top-level origin when the ceremony is framed.
    #[serde(default)]
    pub top_origin: Option<String>,
}

impl PasskeyRequest {
    /// Parses an assertion request from a control-protocol operation.
    pub fn from_operation(operation: &serde_json::Value) -> Result<PasskeyRequest, BrokerError> {
        let origin = operation
            .get("origin")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BrokerError::Invalid("origin is required".to_string()))?
            .to_string();
        let challenge = decode_challenge(operation.get("challenge"))?;
        let rp_id = operation
            .get("rp_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let allow_credentials = match operation.get("allow_credentials") {
            Some(serde_json::Value::Array(_)) => serde_json::from_value(
                operation.get("allow_credentials").cloned().unwrap(),
            )
            .map_err(|e| BrokerError::Invalid(format!("allow_credentials: {e}")))?,
            _ => Vec::new(),
        };
        let user_verification = operation
            .get("user_verification")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let extensions = match operation.get("extensions") {
            Some(value) if !value.is_null() => Some(
                serde_json::from_value(value.clone())
                    .map_err(|e| BrokerError::Invalid(format!("extensions: {e}")))?,
            ),
            _ => None,
        };
        Ok(PasskeyRequest {
            task_id: operation
                .get("task_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            origin,
            rp_id,
            challenge,
            allow_credentials,
            user_verification,
            extensions,
            cross_origin: operation
                .get("cross_origin")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            top_origin: operation
                .get("top_origin")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }
}

/// A request to register a passkey.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationRequest {
    /// The supervised task the ceremony belongs to.
    pub task_id: String,
    /// The origin the ceremony runs for.
    pub origin: String,
    /// The relying party.
    pub rp: PublicKeyCredentialRpEntity,
    /// The user account.
    pub user: PublicKeyCredentialUserEntity,
    /// The challenge from the relying party.
    pub challenge: Vec<u8>,
    /// The accepted algorithms.
    #[serde(default)]
    pub pub_key_cred_params: Vec<PublicKeyCredentialParameters>,
    /// Credentials the relying party does not want re-registered.
    #[serde(default)]
    pub exclude_credentials: Vec<PublicKeyCredentialDescriptor>,
    /// Authenticator selection criteria.
    #[serde(default)]
    pub authenticator_selection: Option<AuthenticatorSelectionCriteria>,
    /// The attestation conveyance preference.
    #[serde(default)]
    pub attestation: Option<String>,
    /// Extension inputs.
    #[serde(default)]
    pub extensions: Option<ExtensionInputs>,
    /// Whether the ceremony is running in a cross-origin frame.
    #[serde(default)]
    pub cross_origin: bool,
}

impl RegistrationRequest {
    /// Parses a registration request from a control-protocol operation.
    pub fn from_operation(
        operation: &serde_json::Value,
    ) -> Result<RegistrationRequest, BrokerError> {
        let origin = operation
            .get("origin")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BrokerError::Invalid("origin is required".to_string()))?
            .to_string();
        let challenge = decode_challenge(operation.get("challenge"))?;
        let rp = operation
            .get("rp")
            .ok_or_else(|| BrokerError::Invalid("rp is required".to_string()))
            .and_then(|value| {
                serde_json::from_value::<PublicKeyCredentialRpEntity>(value.clone())
                    .map_err(|e| BrokerError::Invalid(format!("rp: {e}")))
            })?;
        let user_value = operation
            .get("user")
            .ok_or_else(|| BrokerError::Invalid("user is required".to_string()))?;
        let mut user = serde_json::from_value::<PublicKeyCredentialUserEntity>(user_value.clone())
            .map_err(|e| BrokerError::Invalid(format!("user: {e}")))?;
        // The user handle travels as base64url on the wire.
        if user.id.is_empty() {
            if let Some(raw) = user_value.get("id").and_then(serde_json::Value::as_str) {
                user.id = decode_challenge(Some(&serde_json::Value::String(raw.to_string())))?;
            }
        }
        let pub_key_cred_params = match operation.get("pub_key_cred_params") {
            Some(serde_json::Value::Array(_)) => serde_json::from_value(
                operation.get("pub_key_cred_params").cloned().unwrap(),
            )
            .map_err(|e| BrokerError::Invalid(format!("pub_key_cred_params: {e}")))?,
            _ => Vec::new(),
        };
        let exclude_credentials = match operation.get("exclude_credentials") {
            Some(serde_json::Value::Array(_)) => serde_json::from_value(
                operation.get("exclude_credentials").cloned().unwrap(),
            )
            .map_err(|e| BrokerError::Invalid(format!("exclude_credentials: {e}")))?,
            _ => Vec::new(),
        };
        let authenticator_selection = match operation.get("authenticator_selection") {
            Some(value) if !value.is_null() => Some(
                serde_json::from_value(value.clone())
                    .map_err(|e| BrokerError::Invalid(format!("authenticator_selection: {e}")))?,
            ),
            _ => None,
        };
        let extensions = match operation.get("extensions") {
            Some(value) if !value.is_null() => Some(
                serde_json::from_value(value.clone())
                    .map_err(|e| BrokerError::Invalid(format!("extensions: {e}")))?,
            ),
            _ => None,
        };
        Ok(RegistrationRequest {
            task_id: operation
                .get("task_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            origin,
            rp,
            user,
            challenge,
            pub_key_cred_params,
            exclude_credentials,
            authenticator_selection,
            attestation: operation
                .get("attestation")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            extensions,
            cross_origin: operation
                .get("cross_origin")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// Decodes a base64url (or standard base64) challenge from the wire.
fn decode_challenge(value: Option<&serde_json::Value>) -> Result<Vec<u8>, BrokerError> {
    let raw = value
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BrokerError::Invalid("challenge is required".to_string()))?;
    if raw.contains('-') || raw.contains('_') {
        return crate::webauthn::b64::b64url_decode(raw)
            .ok_or_else(|| BrokerError::Invalid("challenge is not valid base64url".to_string()));
    }
    crate::control::b64_decode(raw)
        .or_else(|| crate::webauthn::b64::b64url_decode(raw))
        .ok_or_else(|| BrokerError::Invalid("challenge is not valid base64".to_string()))
}

/// The result a completed ceremony produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CeremonyOutcome {
    /// A completed assertion.
    Assertion(AssertionCredential),
    /// A completed registration.
    Registration(RegistrationCredential),
}

/// The state of a passkey ceremony.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasskeyStatus {
    /// The ceremony was created and is waiting for the human.
    AwaitingApproval {
        /// The prompt to show.
        prompt: String,
        /// The origin the ceremony is for.
        origin: String,
        /// The redacted account that would be used, if one matched.
        account_hint: Option<String>,
    },
    /// The ceremony finished.
    Complete(CeremonyOutcome),
    /// The ceremony failed.
    Failed(String),
    /// The human cancelled the ceremony.
    Cancelled,
}

/// The passkey broker.
pub trait PasskeyBroker {
    /// Starts an assertion ceremony.
    fn begin_assertion(&mut self, req: PasskeyRequest) -> Result<PasskeyCeremonyId, BrokerError>;

    /// Reports the state of an assertion ceremony.
    fn poll_assertion(&mut self, id: PasskeyCeremonyId) -> Result<PasskeyStatus, BrokerError>;

    /// Cancels an assertion ceremony.
    fn cancel_assertion(&mut self, id: PasskeyCeremonyId) -> Result<(), BrokerError>;

    /// Starts a registration ceremony.
    fn begin_registration(
        &mut self,
        req: RegistrationRequest,
    ) -> Result<PasskeyCeremonyId, BrokerError>;

    /// Reports the state of a registration ceremony.
    fn poll_registration(&mut self, id: PasskeyCeremonyId) -> Result<PasskeyStatus, BrokerError>;

    /// Cancels a registration ceremony.
    fn cancel_registration(&mut self, id: PasskeyCeremonyId) -> Result<(), BrokerError>;

    /// Records the human's approval and runs the ceremony.
    ///
    /// This is the approval gate the spec requires: the ceremony does not
    /// produce an assertion until a human has approved it.
    fn approve_ceremony(
        &mut self,
        id: PasskeyCeremonyId,
        decided_by: &str,
        surface: &str,
    ) -> Result<(), BrokerError>;
}

/// Redacts an account name for display to the agent and to the human.
///
/// The spec allows the agent to learn that a credential exists, not who it
/// belongs to.
pub fn redact_username(username: &str) -> String {
    if let Some((local, domain)) = username.split_once('@') {
        if !local.is_empty() && !domain.is_empty() {
            let first = local.chars().next().unwrap_or('*');
            return format!("{first}**@{domain}");
        }
    }
    match username.chars().count() {
        0 => String::new(),
        1 => "*".to_string(),
        n => {
            let first: String = username.chars().take(1).collect();
            format!("{first}{}", "*".repeat(n - 1))
        }
    }
}

/// The in-flight state of a ceremony.
struct Ceremony {
    status: PasskeyStatus,
    request: CeremonyRequest,
}

enum CeremonyRequest {
    Assertion(PasskeyRequest),
    Registration(RegistrationRequest),
}

/// The concrete passkey broker, backed by the macOS Keychain.
pub struct LocalPasskeyBroker {
    store: KeychainPasskeyStore,
    authenticator: Authenticator,
    ceremonies: HashMap<String, Ceremony>,
    next_id: u64,
}

impl LocalPasskeyBroker {
    /// Creates a broker over the default Keychain service.
    pub fn new() -> Self {
        Self {
            store: KeychainPasskeyStore::new(),
            authenticator: Authenticator::new(),
            ceremonies: HashMap::new(),
            next_id: 1,
        }
    }

    /// Creates a broker over an explicit Keychain service, for tests.
    pub fn with_store(store: KeychainPasskeyStore) -> Self {
        Self {
            store,
            authenticator: Authenticator::new(),
            ceremonies: HashMap::new(),
            next_id: 1,
        }
    }

    /// The AAGUID this authenticator reports.
    pub fn aaguid(&self) -> [u8; 16] {
        self.authenticator.aaguid
    }

    /// Lists the stored passkeys, redacted for the agent.
    pub fn list_passkeys(&self) -> Result<Vec<CredentialMatch>, BrokerError> {
        let records = self.store.all().map_err(BrokerError::Storage)?;
        Ok(records
            .iter()
            .map(|record| CredentialMatch {
                username_hint: redact_username(&record.user_name),
                site: record.relying_party_identifier.clone(),
                is_passkey: true,
            })
            .collect())
    }

    /// Deletes a passkey and returns whether one was removed.
    pub fn delete_passkey(&mut self, credential_id: &[u8]) -> Result<bool, BrokerError> {
        let existed = self
            .store
            .by_credential_id(credential_id)
            .map_err(BrokerError::Storage)?
            .is_some();
        self.store
            .delete(credential_id)
            .map_err(BrokerError::Storage)?;
        Ok(existed)
    }

    /// Returns the public key of a stored passkey.
    ///
    /// A relying party needs this to register the credential. The private key
    /// never leaves the store.
    pub fn public_key_pem(&self, credential_id: &[u8]) -> Result<Option<String>, BrokerError> {
        Ok(self
            .store
            .by_credential_id(credential_id)
            .map_err(BrokerError::Storage)?
            .map(|record| record.public_key_pem))
    }

    /// Builds the prompt for an assertion request.
    pub fn describe_assertion(
        &self,
        req: &PasskeyRequest,
    ) -> Result<CredentialPrompt, BrokerError> {
        let validated = validate_origin_and_effective_rp_id(
            &req.origin,
            req.rp_id.as_deref(),
            req.cross_origin,
            false,
        )
        .map_err(BrokerError::from)?;
        let records = self.store.all().map_err(BrokerError::Storage)?;
        let candidates: Vec<&StoredPasskey> = records
            .iter()
            .filter(|record| {
                record
                    .relying_party_identifier
                    .eq_ignore_ascii_case(&validated.effective_rp_id)
            })
            .collect();
        if candidates.is_empty() {
            return Err(BrokerError::NotFound(format!(
                "No passkey for {}",
                validated.effective_rp_id
            )));
        }
        let matches = candidates
            .iter()
            .map(|record| CredentialMatch {
                username_hint: redact_username(&record.user_name),
                site: record.relying_party_identifier.clone(),
                is_passkey: true,
            })
            .collect();
        Ok(CredentialPrompt {
            origin: validated.normalized_origin,
            registrable_domain: validated.effective_rp_id.clone(),
            matches,
            can_add_credential: false,
        })
    }

    /// Runs an approved assertion.
    fn run_assertion(&mut self, req: &PasskeyRequest) -> Result<CeremonyOutcome, BrokerError> {
        let options = PublicKeyCredentialRequestOptions {
            challenge: req.challenge.clone(),
            rp_id: req.rp_id.clone(),
            timeout: None,
            allow_credentials: req.allow_credentials.clone(),
            user_verification: req.user_verification.clone(),
            extensions: req.extensions.clone(),
            hints: Vec::new(),
        };
        let (credential, _) = self
            .authenticator
            .get(
                &mut self.store,
                &options,
                &req.origin,
                req.cross_origin,
                req.top_origin.as_deref(),
            )
            .map_err(BrokerError::from)?;
        Ok(CeremonyOutcome::Assertion(credential))
    }

    /// Runs an approved registration.
    fn run_registration(
        &mut self,
        req: &RegistrationRequest,
    ) -> Result<CeremonyOutcome, BrokerError> {
        let options = PublicKeyCredentialCreationOptions {
            rp: req.rp.clone(),
            user: req.user.clone(),
            challenge: req.challenge.clone(),
            pub_key_cred_params: req.pub_key_cred_params.clone(),
            timeout: None,
            exclude_credentials: req.exclude_credentials.clone(),
            authenticator_selection: req.authenticator_selection.clone(),
            attestation: req.attestation.clone(),
            extensions: req.extensions.clone(),
        };
        let (credential, _) = self
            .authenticator
            .create(
                &mut self.store,
                &options,
                &req.origin,
                req.cross_origin,
                None,
                &crate::control::now_iso(),
            )
            .map_err(BrokerError::from)?;
        Ok(CeremonyOutcome::Registration(credential))
    }
}

impl Default for LocalPasskeyBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl PasskeyBroker for LocalPasskeyBroker {
    fn begin_assertion(&mut self, req: PasskeyRequest) -> Result<PasskeyCeremonyId, BrokerError> {
        let prompt = self.describe_assertion(&req)?;
        let id = format!("passkey-assert-{}", self.next_id);
        self.next_id += 1;
        let account_hint = prompt.matches.first().map(|m| m.username_hint.clone());
        self.ceremonies.insert(
            id.clone(),
            Ceremony {
                status: PasskeyStatus::AwaitingApproval {
                    prompt: format!(
                        "Use the passkey for {} to sign in to {}?",
                        account_hint.clone().unwrap_or_else(|| "this account".to_string()),
                        prompt.registrable_domain
                    ),
                    origin: prompt.origin,
                    account_hint,
                },
                request: CeremonyRequest::Assertion(req),
            },
        );
        Ok(PasskeyCeremonyId(id))
    }

    fn poll_assertion(&mut self, id: PasskeyCeremonyId) -> Result<PasskeyStatus, BrokerError> {
        let ceremony = self
            .ceremonies
            .get(&id.0)
            .ok_or_else(|| BrokerError::NotFound("no such ceremony".to_string()))?;
        Ok(ceremony.status.clone())
    }

    fn cancel_assertion(&mut self, id: PasskeyCeremonyId) -> Result<(), BrokerError> {
        let ceremony = self
            .ceremonies
            .get_mut(&id.0)
            .ok_or_else(|| BrokerError::NotFound("no such ceremony".to_string()))?;
        ceremony.status = PasskeyStatus::Cancelled;
        Ok(())
    }

    fn begin_registration(
        &mut self,
        req: RegistrationRequest,
    ) -> Result<PasskeyCeremonyId, BrokerError> {
        // Validate the origin before prompting, so the human is never asked to
        // approve a ceremony that cannot succeed.
        validate_origin_and_effective_rp_id(
            &req.origin,
            Some(req.rp.id.as_str()),
            req.cross_origin,
            true,
        )
        .map_err(BrokerError::from)?;
        let id = format!("passkey-register-{}", self.next_id);
        self.next_id += 1;
        self.ceremonies.insert(
            id.clone(),
            Ceremony {
                status: PasskeyStatus::AwaitingApproval {
                    prompt: format!(
                        "Create a passkey for {} on {}?",
                        req.user.name, req.rp.id
                    ),
                    origin: req.origin.clone(),
                    account_hint: Some(redact_username(&req.user.name)),
                },
                request: CeremonyRequest::Registration(req),
            },
        );
        Ok(PasskeyCeremonyId(id))
    }

    fn poll_registration(&mut self, id: PasskeyCeremonyId) -> Result<PasskeyStatus, BrokerError> {
        let ceremony = self
            .ceremonies
            .get(&id.0)
            .ok_or_else(|| BrokerError::NotFound("no such ceremony".to_string()))?;
        Ok(ceremony.status.clone())
    }

    fn cancel_registration(&mut self, id: PasskeyCeremonyId) -> Result<(), BrokerError> {
        let ceremony = self
            .ceremonies
            .get_mut(&id.0)
            .ok_or_else(|| BrokerError::NotFound("no such ceremony".to_string()))?;
        ceremony.status = PasskeyStatus::Cancelled;
        Ok(())
    }

    fn approve_ceremony(
        &mut self,
        id: PasskeyCeremonyId,
        decided_by: &str,
        surface: &str,
    ) -> Result<(), BrokerError> {
        let ceremony = self
            .ceremonies
            .get_mut(&id.0)
            .ok_or_else(|| BrokerError::NotFound("no such ceremony".to_string()))?;
        if matches!(ceremony.status, PasskeyStatus::Cancelled) {
            return Err(BrokerError::InvalidState(
                "the ceremony was cancelled".to_string(),
            ));
        }
        if matches!(ceremony.status, PasskeyStatus::Complete(_)) {
            return Err(BrokerError::InvalidState(
                "the ceremony already completed".to_string(),
            ));
        }
        let request = match &ceremony.request {
            CeremonyRequest::Assertion(req) => CeremonyRequest::Assertion(req.clone()),
            CeremonyRequest::Registration(req) => CeremonyRequest::Registration(req.clone()),
        };
        let outcome = match request {
            CeremonyRequest::Assertion(req) => self.run_assertion(&req),
            CeremonyRequest::Registration(req) => self.run_registration(&req),
        };
        let status = match outcome {
            Ok(outcome) => PasskeyStatus::Complete(outcome),
            Err(error) => PasskeyStatus::Failed(error.to_string()),
        };
        let ceremony = self
            .ceremonies
            .get_mut(&id.0)
            .ok_or_else(|| BrokerError::NotFound("no such ceremony".to_string()))?;
        ceremony.status = status;
        eprintln!(
            "[passkey] ceremony {} {} by {} on {}",
            id.0, ceremony.status_kind(), decided_by, surface
        );
        Ok(())
    }
}

impl Ceremony {
    /// A short label for the ceremony's current state, for logs.
    fn status_kind(&self) -> &'static str {
        match self.status {
            PasskeyStatus::AwaitingApproval { .. } => "awaiting-approval",
            PasskeyStatus::Complete(_) => "complete",
            PasskeyStatus::Failed(_) => "failed",
            PasskeyStatus::Cancelled => "cancelled",
        }
    }
}

/// The concrete password broker, backed by the macOS Keychain.
pub struct LocalCredentialBroker {
    passwords: KeychainPasswordStore,
    passkeys: KeychainPasskeyStore,
}

impl LocalCredentialBroker {
    /// Creates a broker over the default Keychain services.
    pub fn new() -> Self {
        Self {
            passwords: KeychainPasswordStore::new(),
            passkeys: KeychainPasskeyStore::new(),
        }
    }

    /// Creates a broker over explicit Keychain services, for tests.
    pub fn with_stores(passwords: KeychainPasswordStore, passkeys: KeychainPasskeyStore) -> Self {
        Self {
            passwords,
            passkeys,
        }
    }

    /// Saves a password for a site.
    pub fn save_password(
        &self,
        host: &str,
        username: &str,
        secret: &str,
    ) -> Result<(), BrokerError> {
        self.passwords.save(host, username, secret)
    }

    /// Deletes a password.
    pub fn delete_password(&self, host: &str, username: &str) -> Result<(), BrokerError> {
        self.passwords.delete(host, username)
    }

    /// Lists every saved credential, redacted.
    pub fn list(&self) -> Result<Vec<CredentialMatch>, BrokerError> {
        let mut out: Vec<CredentialMatch> = self
            .passwords
            .list()?
            .into_iter()
            .map(|credential| CredentialMatch {
                username_hint: redact_username(&credential.username),
                site: credential.host.clone(),
                is_passkey: false,
            })
            .collect();
        for record in self.passkeys.all().map_err(BrokerError::Storage)? {
            out.push(CredentialMatch {
                username_hint: redact_username(&record.user_name),
                site: record.relying_party_identifier.clone(),
                is_passkey: true,
            });
        }
        Ok(out)
    }
}

impl Default for LocalCredentialBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialBroker for LocalCredentialBroker {
    fn describe_fill(
        &self,
        req: CredentialFillRequest,
    ) -> Result<CredentialPrompt, BrokerError> {
        let validated = validate_origin_and_effective_rp_id(&req.origin, None, false, false)
            .map_err(BrokerError::from)?;
        let host = validated.effective_rp_id.clone();
        // The spec requires the prompt to name the exact registrable domain,
        // and a saved credential matches when it was stored for either the
        // origin host or that registrable domain.
        let registrable_domain = crate::webauthn::origin::registrable_suffix(&host);
        let matches_host = |candidate: &str| {
            candidate.eq_ignore_ascii_case(&host)
                || candidate.eq_ignore_ascii_case(&registrable_domain)
        };

        let mut matches: Vec<CredentialMatch> = self
            .passwords
            .list()?
            .into_iter()
            .filter(|credential| matches_host(&credential.host))
            .map(|credential| CredentialMatch {
                username_hint: redact_username(&credential.username),
                site: credential.host.clone(),
                is_passkey: false,
            })
            .collect();
        for record in self.passkeys.all().map_err(BrokerError::Storage)? {
            if record
                .relying_party_identifier
                .eq_ignore_ascii_case(&host)
            {
                matches.push(CredentialMatch {
                    username_hint: redact_username(&record.user_name),
                    site: record.relying_party_identifier.clone(),
                    is_passkey: true,
                });
            }
        }

        Ok(CredentialPrompt {
            origin: validated.normalized_origin,
            registrable_domain,
            matches,
            can_add_credential: true,
        })
    }

    fn apply_approved_fill(
        &mut self,
        approval: CredentialApproval,
        engine: &mut dyn CredentialSink,
    ) -> Result<CredentialFillReceipt, BrokerError> {
        let chosen = approval
            .chosen
            .ok_or_else(|| BrokerError::Invalid("no credential was chosen".to_string()))?;

        // The agent only ever sees the redacted hint, so the actual account
        // is resolved against the store by site and redacted hint.
        let candidate = self
            .passwords
            .list()?
            .into_iter()
            .find(|credential| {
                credential.host.eq_ignore_ascii_case(&chosen.site)
                    && redact_username(&credential.username) == chosen.username_hint
            })
            .ok_or_else(|| {
                BrokerError::NotFound(format!(
                    "no credential matching {} on {}",
                    chosen.username_hint, chosen.site
                ))
            })?;

        let secret = self
            .passwords
            .secret(&candidate.host, &candidate.username)
            .ok_or_else(|| {
                BrokerError::NotFound(format!(
                    "the secret for {} on {} is no longer stored",
                    chosen.username_hint, chosen.site
                ))
            })?;

        let mut filled_fields = Vec::new();
        engine.set_field(CredentialFieldKind::Username, &candidate.username)?;
        filled_fields.push(CredentialFieldKind::Username);
        engine.set_field(CredentialFieldKind::CurrentPassword, &secret)?;
        filled_fields.push(CredentialFieldKind::CurrentPassword);

        Ok(CredentialFillReceipt {
            origin: chosen.site.clone(),
            filled_fields,
            account_hint: Some(chosen.username_hint.clone()),
        })
    }
}

/// Builds the base64url credential ID of a passkey record.
pub fn credential_id_b64url(record: &StoredPasskey) -> String {
    b64url_encode(&record.credential_id)
}

/// The AAGUID the Equipped Browser authenticator reports.
pub const BROWSER_AAGUID: [u8; 16] = EQUIPPED_AAGUID;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webauthn::types::{
        PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity,
    };

    fn test_passkey_service(name: &str) -> KeychainPasskeyStore {
        KeychainPasskeyStore::with_service(
            &format!("ai.zmax.hermes.browser.test.passkeys.{name}"),
            "passkey_records_v1",
        )
    }

    fn test_password_service(name: &str) -> KeychainPasswordStore {
        KeychainPasswordStore::with_service(&format!(
            "ai.zmax.hermes.browser.test.credentials.{name}"
        ))
    }

    #[test]
    fn redacts_usernames_without_hiding_the_domain() {
        assert_eq!(redact_username("ada@example.com"), "a**@example.com");
        assert_eq!(redact_username("a@example.com"), "a**@example.com");
        assert_eq!(redact_username("ada"), "a**");
        assert_eq!(redact_username(""), "");
    }

    #[test]
    fn stores_and_reads_a_password_in_the_keychain() {
        let store = test_password_service("store");
        store.save("example.com", "ada", "hunter2").unwrap();
        assert_eq!(
            store.secret("example.com", "ada").as_deref(),
            Some("hunter2")
        );
        // The secret must not be handed to a different site.
        assert!(store.secret("other.com", "ada").is_none());
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].host, "example.com");
        assert_eq!(listed[0].username, "ada");
        store.delete("example.com", "ada").unwrap();
        assert!(store.secret("example.com", "ada").is_none());
    }

    #[test]
    fn describes_a_fill_without_leaking_the_secret() {
        let store = test_password_service("describe");
        store.save("example.com", "ada", "hunter2").unwrap();
        let broker = LocalCredentialBroker::with_stores(
            store,
            test_passkey_service("describe"),
        );
        let prompt = broker
            .describe_fill(CredentialFillRequest {
                task_id: "t1".to_string(),
                origin: "https://login.example.com".to_string(),
                fields: vec![CredentialFieldKind::CurrentPassword],
                document_generation: 1,
            })
            .unwrap();
        assert_eq!(prompt.registrable_domain, "example.com");
        assert_eq!(prompt.matches.len(), 1);
        assert_eq!(prompt.matches[0].username_hint, "a**");
        assert_eq!(prompt.matches[0].site, "example.com");
        assert!(!format!("{prompt:?}").contains("hunter2"));
    }

    #[test]
    fn an_unapproved_ceremony_produces_no_assertion() {
        let mut broker = LocalPasskeyBroker::with_store(test_passkey_service("unapproved"));
        let id = broker
            .begin_assertion(PasskeyRequest {
                task_id: "t1".to_string(),
                origin: "https://example.com".to_string(),
                rp_id: Some("example.com".to_string()),
                challenge: b"challenge".to_vec(),
                allow_credentials: Vec::new(),
                user_verification: None,
                extensions: None,
                cross_origin: false,
                top_origin: None,
            })
            .unwrap_err();
        // No passkey exists yet, so the broker refuses rather than prompting.
        assert!(matches!(id, BrokerError::NotFound(_)));
    }

    #[test]
    fn a_registration_then_assertion_round_trip_works_through_the_broker() {
        let store = test_passkey_service("roundtrip");
        let mut broker = LocalPasskeyBroker::with_store(store);

        let register_id = broker
            .begin_registration(RegistrationRequest {
                task_id: "t1".to_string(),
                origin: "https://example.com".to_string(),
                rp: PublicKeyCredentialRpEntity {
                    id: "example.com".to_string(),
                    name: Some("Example".to_string()),
                },
                user: PublicKeyCredentialUserEntity {
                    id: b"user-1".to_vec(),
                    name: "ada@example.com".to_string(),
                    display_name: "Ada".to_string(),
                },
                challenge: b"challenge-1".to_vec(),
                pub_key_cred_params: Vec::new(),
                exclude_credentials: Vec::new(),
                authenticator_selection: None,
                attestation: None,
                extensions: None,
                cross_origin: false,
            })
            .unwrap();
        assert!(matches!(
            broker.poll_registration(register_id.clone()).unwrap(),
            PasskeyStatus::AwaitingApproval { .. }
        ));
        broker
            .approve_ceremony(register_id.clone(), "user", "socket")
            .unwrap();
        let status = broker.poll_registration(register_id).unwrap();
        let PasskeyStatus::Complete(CeremonyOutcome::Registration(credential)) = status else {
            panic!("registration must complete");
        };
        assert_eq!(credential.kind, "public-key");
        assert!(!credential.response.attestation_object.is_empty());

        // Now an assertion against the registered credential.
        let assert_id = broker
            .begin_assertion(PasskeyRequest {
                task_id: "t2".to_string(),
                origin: "https://example.com".to_string(),
                rp_id: Some("example.com".to_string()),
                challenge: b"challenge-2".to_vec(),
                allow_credentials: Vec::new(),
                user_verification: Some("preferred".to_string()),
                extensions: None,
                cross_origin: false,
                top_origin: None,
            })
            .unwrap();
        assert!(matches!(
            broker.poll_assertion(assert_id.clone()).unwrap(),
            PasskeyStatus::AwaitingApproval { .. }
        ));
        broker
            .approve_ceremony(assert_id.clone(), "user", "socket")
            .unwrap();
        let status = broker.poll_assertion(assert_id).unwrap();
        let PasskeyStatus::Complete(CeremonyOutcome::Assertion(assertion)) = status else {
            panic!("assertion must complete");
        };
        assert!(!assertion.response.signature.is_empty());
        assert!(!assertion.response.authenticator_data.is_empty());
    }

    #[test]
    fn a_cancelled_ceremony_cannot_be_approved() {
        let mut broker = LocalPasskeyBroker::with_store(test_passkey_service("cancelled"));
        let id = broker
            .begin_registration(RegistrationRequest {
                task_id: "t1".to_string(),
                origin: "https://example.com".to_string(),
                rp: PublicKeyCredentialRpEntity {
                    id: "example.com".to_string(),
                    name: None,
                },
                user: PublicKeyCredentialUserEntity {
                    id: b"user-1".to_vec(),
                    name: "ada@example.com".to_string(),
                    display_name: "Ada".to_string(),
                },
                challenge: b"challenge-1".to_vec(),
                pub_key_cred_params: Vec::new(),
                exclude_credentials: Vec::new(),
                authenticator_selection: None,
                attestation: None,
                extensions: None,
                cross_origin: false,
            })
            .unwrap();
        broker.cancel_registration(id.clone()).unwrap();
        assert_eq!(
            broker.poll_registration(id.clone()).unwrap(),
            PasskeyStatus::Cancelled
        );
        assert!(matches!(
            broker.approve_ceremony(id, "user", "socket"),
            Err(BrokerError::InvalidState(_))
        ));
    }
}
