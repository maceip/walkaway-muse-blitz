//! macOS Keychain storage for credentials and passkeys.
//!
//! Ported from the iOS Equipped Browser Keychain usage:
//!   * `WebAuthnManager.swift` stores every passkey record as one generic
//!     password item (service `ai.zmax.equipped.browser.webauthn.passkeys`,
//!     account `passkey_records_v1`) holding a JSON array.
//!   * `CredentialStore.swift` stores one generic password per account, with
//!     the account name as `kSecAttrAccount`, the site host as `kSecAttrLabel`,
//!     and the bundle identifier as `kSecAttrService`.
//!
//! Both use `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`.
//!
//! ## macOS and `kSecAttrAccessible`
//!
//! The iOS build could set `kSecAttrAccessible` directly. On macOS the SDK
//! documents the attribute as "iOS; also macOS if `kSecAttrSynchronizable`
//! and/or `kSecUseDataProtectionKeychain` set"
//! (`Security.framework/Headers/SecItem.h`). This module therefore writes to
//! the **data protection keychain** (`kSecUseDataProtectionKeychain = true`),
//! which is what makes the accessibility attribute meaningful on the desktop.
//!
//! If the data protection keychain is unavailable to this process (for
//! example an unsigned binary with no keychain access group), the write falls
//! back to the file-based login keychain, where macOS ignores
//! `kSecAttrAccessible` and protects the item with the keychain ACL instead.
//! [`backend`] reports which one is in force, so the difference is never
//! silent.

use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFMutableDictionary};
use core_foundation::string::CFString;
use security_framework::base::Error as KeychainError;
use security_framework::item::{
    ItemClass, ItemSearchOptions, ItemUpdateOptions, ItemUpdateValue, Limit, SearchResult,
};
use security_framework_sys::access_control::kSecAttrAccessibleWhenUnlockedThisDeviceOnly;
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrLabel, kSecAttrService, kSecClass, kSecClassGenericPassword,
    kSecValueData,
};
use std::sync::atomic::{AtomicU8, Ordering};

/// `errSecItemNotFound`.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
/// `errSecDuplicateItem`.
const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;
/// `errSecMissingEntitlement`.
const ERR_SEC_MISSING_ENTITLEMENT: i32 = -34018;
/// `errSecNotAvailable`.
const ERR_SEC_NOT_AVAILABLE: i32 = -25291;
/// `errSecInteractionNotAllowed`.
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;

/// The attribute names the Security framework uses for the fields this module
/// reads back during enumeration.
const ATTR_ACCOUNT: &str = "acct";
const ATTR_LABEL: &str = "labl";

/// Apple's `kSecAttrAccessible` attribute key.
///
/// The Security framework exposes the *values* (`kSecAttrAccessibleWhenUnlocked`,
/// `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, ...) but not the attribute
/// key itself, so the key is taken from Apple's `SecItem.h`. The literal was
/// read out of the live framework at runtime with `dlsym` rather than
/// guessed, and `round_trips_the_accessible_attribute` below reads the
/// attribute back off a live item, so a wrong value fails the test rather
/// than silently degrading the protection.
const K_SEC_ATTR_ACCESSIBLE: &str = "pdmn";

/// Apple's `kSecUseDataProtectionKeychain` query key, also read out of the
/// live framework with `dlsym`.
const K_SEC_USE_DATA_PROTECTION_KEYCHAIN: &str = "nleg";

/// Which macOS keychain the credentials actually live in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// The data protection keychain. `kSecAttrAccessible` is honored, so
    /// items are protected exactly as the iOS implementation protects them.
    DataProtection,
    /// The file-based login keychain. macOS ignores `kSecAttrAccessible`
    /// here; items are protected by the keychain ACL and the login password.
    LegacyLogin,
}

impl Backend {
    /// Human-readable description for logs and status surfaces.
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::DataProtection => "data-protection-keychain",
            Backend::LegacyLogin => "legacy-login-keychain",
        }
    }
}

static BACKEND: AtomicU8 = AtomicU8::new(0);

fn record_backend(backend: Backend) {
    let code = match backend {
        Backend::DataProtection => 1,
        Backend::LegacyLogin => 2,
    };
    BACKEND.store(code, Ordering::Relaxed);
}

/// Reports which keychain backend is in force.
///
/// Returns `None` until the first write has determined it, so that a caller
/// never reports an assumption as an observation.
pub fn backend() -> Option<Backend> {
    match BACKEND.load(Ordering::Relaxed) {
        1 => Some(Backend::DataProtection),
        2 => Some(Backend::LegacyLogin),
        _ => None,
    }
}

/// `kSecClass`.
fn k_sec_class() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecClass) }
}

/// `kSecClassGenericPassword`.
fn k_sec_class_generic_password() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecClassGenericPassword) }
}

/// `kSecAttrService`.
fn k_sec_attr_service() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrService) }
}

/// `kSecAttrAccount`.
fn k_sec_attr_account() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrAccount) }
}

/// `kSecAttrLabel`.
fn k_sec_attr_label() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrLabel) }
}

/// `kSecValueData`.
fn k_sec_value_data() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecValueData) }
}

/// `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`.
fn k_sec_attr_accessible_when_unlocked_this_device_only() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrAccessibleWhenUnlockedThisDeviceOnly) }
}

/// Attributes discovered while enumerating Keychain items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemAttributes {
    /// `kSecAttrAccount`.
    pub account: String,
    /// `kSecAttrLabel`.
    pub label: Option<String>,
    /// `kSecAttrAccessible`, when the item carries one.
    pub accessible: Option<String>,
}

/// Builds a query dictionary for a generic password item.
fn generic_password_query(
    service: &str,
    account: Option<&str>,
) -> CFMutableDictionary<CFString, CFType> {
    let mut query = CFMutableDictionary::<CFString, CFType>::from_CFType_pairs(&[]);
    query.set(k_sec_class(), k_sec_class_generic_password().as_CFType());
    query.set(k_sec_attr_service(), CFString::new(service).as_CFType());
    if let Some(account) = account {
        query.set(k_sec_attr_account(), CFString::new(account).as_CFType());
    }
    query
}

/// Reports whether an error means the data protection keychain is unusable
/// for this process.
fn data_protection_unavailable(err: &KeychainError) -> bool {
    matches!(
        err.code(),
        ERR_SEC_MISSING_ENTITLEMENT | ERR_SEC_NOT_AVAILABLE | ERR_SEC_INTERACTION_NOT_ALLOWED
    )
}

/// Stores a generic password item, replacing any existing item with the same
/// service and account.
pub fn set_generic_item(
    service: &str,
    account: &str,
    label: Option<&str>,
    data: &[u8],
) -> Result<(), KeychainError> {
    // Remove first so the write is idempotent, matching the Swift
    // `SecItemDelete` then `SecItemAdd` sequence.
    let _ = delete_generic_item(service, account);

    // Attempt 1: the data protection keychain, where `kSecAttrAccessible`
    // carries its iOS meaning.
    let mut attributes = generic_password_query(service, Some(account));
    if let Some(label) = label {
        attributes.set(k_sec_attr_label(), CFString::new(label).as_CFType());
    }
    attributes.set(k_sec_value_data(), CFData::from_buffer(data).as_CFType());
    attributes.set(
        CFString::new(K_SEC_USE_DATA_PROTECTION_KEYCHAIN),
        CFBoolean::true_value().as_CFType(),
    );
    attributes.set(
        CFString::new(K_SEC_ATTR_ACCESSIBLE),
        k_sec_attr_accessible_when_unlocked_this_device_only().as_CFType(),
    );

    // `add_item` is deprecated in favour of `ItemAddOptions::add`, but that
    // builder cannot express `kSecAttrAccessible` or
    // `kSecUseDataProtectionKeychain`, both of which this port needs in order
    // to match the iOS implementation's protection.
    #[allow(deprecated)]
    let first = security_framework::item::add_item(attributes.to_untyped().to_immutable());
    match first {
        Ok(()) => {
            record_backend(Backend::DataProtection);
            Ok(())
        }
        Err(err) if data_protection_unavailable(&err) => {
            // Attempt 2: the file-based login keychain. macOS ignores
            // `kSecAttrAccessible` here, so the item is protected by the
            // keychain ACL rather than by the accessibility attribute.
            eprintln!(
                "[keychain] data protection keychain unavailable ({}); using the login keychain",
                describe_error(&err)
            );
            let mut legacy = generic_password_query(service, Some(account));
            if let Some(label) = label {
                legacy.set(k_sec_attr_label(), CFString::new(label).as_CFType());
            }
            legacy.set(k_sec_value_data(), CFData::from_buffer(data).as_CFType());
            #[allow(deprecated)]
            let result =
                security_framework::item::add_item(legacy.to_untyped().to_immutable());
            if result.is_ok() {
                record_backend(Backend::LegacyLogin);
            }
            result
        }
        Err(err) => Err(err),
    }
}

/// Reads a generic password item's data.
pub fn get_generic_item(service: &str, account: &str) -> Option<Vec<u8>> {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(account)
        .load_data(true)
        .limit(Limit::Max(1));
    let results = search.search().ok()?;
    for result in results {
        if let SearchResult::Data(data) = result {
            return Some(data);
        }
    }
    None
}

/// Deletes a generic password item. Returns whether an item was removed.
pub fn delete_generic_item(service: &str, account: &str) -> bool {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(account);
    match search.delete() {
        Ok(()) => true,
        Err(err) => err.code() != ERR_SEC_ITEM_NOT_FOUND,
    }
}

/// Updates the data of an existing generic password item.
#[allow(dead_code)]
pub fn update_generic_item(
    service: &str,
    account: &str,
    data: &[u8],
) -> Result<(), KeychainError> {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(account);

    let mut update = ItemUpdateOptions::new();
    update.set_value(ItemUpdateValue::Data(CFData::from_buffer(data)));

    security_framework::item::update_item(&search, &update)
}

/// Lists the accounts of every generic password item under a service.
///
/// Returns the account name, label, and accessibility attribute of each item.
pub fn list_generic_items(service: &str) -> Vec<ItemAttributes> {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .load_attributes(true)
        .limit(Limit::All);

    let Ok(results) = search.search() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for result in results {
        let Some(attributes) = result.simplify_dict() else {
            continue;
        };
        let Some(account) = attributes.get(ATTR_ACCOUNT) else {
            continue;
        };
        out.push(ItemAttributes {
            account: account.clone(),
            label: attributes.get(ATTR_LABEL).cloned(),
            accessible: attributes.get(K_SEC_ATTR_ACCESSIBLE).cloned(),
        });
    }
    out
}

/// Reports whether the Keychain is reachable and writable in this process.
///
/// A headless run that cannot reach the Keychain must say so rather than
/// silently pretending that a credential was stored.
pub fn is_available() -> bool {
    let probe_service = "ai.zmax.hermes.browser.keychain.probe";
    let probe_account = "availability";
    match set_generic_item(probe_service, probe_account, Some("probe"), b"probe") {
        Ok(()) => {
            let _ = delete_generic_item(probe_service, probe_account);
            true
        }
        Err(err) => {
            eprintln!("[keychain] probe failed: {}", describe_error(&err));
            false
        }
    }
}

/// Converts a `KeychainError` into a human-readable string.
pub fn describe_error(err: &KeychainError) -> String {
    match err.code() {
        ERR_SEC_ITEM_NOT_FOUND => "keychain item not found".to_string(),
        ERR_SEC_DUPLICATE_ITEM => "keychain item already exists".to_string(),
        ERR_SEC_MISSING_ENTITLEMENT => {
            "keychain item requires an entitlement this process does not have".to_string()
        }
        ERR_SEC_NOT_AVAILABLE => "keychain is not available".to_string(),
        ERR_SEC_INTERACTION_NOT_ALLOWED => {
            "keychain interaction was not allowed".to_string()
        }
        code => format!("keychain error {code}: {}", err),
    }
}

/// Converts an untyped Core Foundation dictionary into its string form.
#[allow(dead_code)]
fn dictionary_to_string(dictionary: &CFDictionary) -> String {
    let mut out = String::new();
    let (keys, values) = dictionary.get_keys_and_values();
    for (key, value) in keys.iter().zip(values.iter()) {
        let key = unsafe { CFString::wrap_under_get_rule((*key).cast()) };
        let value = unsafe { CFString::wrap_under_get_rule((*value).cast()) };
        out.push_str(&format!("{key}={value};"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each test uses its own Keychain service so that the tests, which run
    /// in parallel, cannot observe or delete each other's items.
    fn service_for(test: &str) -> String {
        format!("ai.zmax.hermes.browser.keychain.selftest.{test}")
    }

    #[test]
    fn round_trips_a_generic_item() {
        set_generic_item(&service_for("alice"), "alice", Some("example.com"), b"s3cret")
            .expect("keychain write");
        let read = get_generic_item(&service_for("alice"), "alice").expect("keychain read");
        assert_eq!(read, b"s3cret");
        assert!(delete_generic_item(&service_for("alice"), "alice"));
        assert!(get_generic_item(&service_for("alice"), "alice").is_none());
    }

    #[test]
    fn overwrites_an_existing_item() {
        set_generic_item(&service_for("bob"), "bob", Some("example.com"), b"first").unwrap();
        set_generic_item(&service_for("bob"), "bob", Some("example.com"), b"second").unwrap();
        let read = get_generic_item(&service_for("bob"), "bob").unwrap();
        assert_eq!(read, b"second");
        delete_generic_item(&service_for("bob"), "bob");
    }

    #[test]
    fn enumerates_items_by_service() {
        set_generic_item(&service_for("carol"), "carol", Some("site-a.test"), b"x").unwrap();
        set_generic_item(&service_for("dave"), "dave", Some("site-b.test"), b"y").unwrap();
        let items = list_generic_items(&service_for("carol"));
        let accounts: Vec<&str> = items.iter().map(|i| i.account.as_str()).collect();
        assert!(accounts.contains(&"carol"));
        let carol = items.iter().find(|i| i.account == "carol").unwrap();
        assert_eq!(carol.label.as_deref(), Some("site-a.test"));
        delete_generic_item(&service_for("carol"), "carol");
        delete_generic_item(&service_for("dave"), "dave");
    }

    #[test]
    fn updating_replaces_the_stored_data() {
        set_generic_item(&service_for("erin"), "erin", Some("site.test"), b"old").unwrap();
        update_generic_item(&service_for("erin"), "erin", b"new").unwrap();
        assert_eq!(get_generic_item(&service_for("erin"), "erin").unwrap(), b"new");
        delete_generic_item(&service_for("erin"), "erin");
    }

    #[test]
    fn deleting_a_missing_item_reports_false() {
        assert!(!delete_generic_item(&service_for("nobody"), "nobody"));
    }

    #[test]
    fn round_trips_the_accessible_attribute() {
        set_generic_item(&service_for("frank"), "frank", Some("site.test"), b"v").unwrap();
        let items = list_generic_items(&service_for("frank"));
        let item = items
            .iter()
            .find(|i| i.account == "frank")
            .expect("item must be enumerable");
        match backend() {
            Some(Backend::DataProtection) => {
                // "aku" is kSecAttrAccessibleWhenUnlockedThisDeviceOnly.
                assert_eq!(
                    item.accessible.as_deref(),
                    Some("aku"),
                    "data protection item must carry kSecAttrAccessibleWhenUnlockedThisDeviceOnly"
                );
            }
            Some(Backend::LegacyLogin) => {
                // The file-based login keychain does not carry the attribute;
                // the item is protected by the keychain ACL instead.
                assert_eq!(item.accessible, None);
            }
            None => panic!("the write must have determined a keychain backend"),
        }
        delete_generic_item(&service_for("frank"), "frank");
    }

    #[test]
    fn stores_binary_data_intact() {
        let payload: Vec<u8> = (0..=255u8).collect();
        set_generic_item(&service_for("grace"), "grace", Some("site.test"), &payload).unwrap();
        assert_eq!(get_generic_item(&service_for("grace"), "grace").unwrap(), payload);
        delete_generic_item(&service_for("grace"), "grace");
    }

    #[test]
    fn reports_the_active_backend() {
        set_generic_item(&service_for("backend"), "backend", Some("x"), b"v").unwrap();
        eprintln!(
            "[keychain] active backend: {}",
            backend().map(|b| b.as_str()).unwrap_or("undetermined")
        );
        delete_generic_item(&service_for("backend"), "backend");
    }
}
