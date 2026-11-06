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
//!
//! ## Never prompt the user
//!
//! Reading an item that another binary created makes macOS ask for the login
//! keychain password. [`ensure_no_user_interaction`] switches that off for
//! the whole process, and `main` calls it before anything can reach the
//! Keychain, so an access that would have prompted fails with an error
//! instead. No run of this binary — browser, test suite, or harness — can
//! put a password prompt on the user's screen.

use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFMutableDictionary};
use core_foundation::string::CFString;
use security_framework::base::Error as KeychainError;
use security_framework::os::macos::keychain::{KeychainUserInteractionLock, SecKeychain};
use security_framework_sys::access_control::kSecAttrAccessibleWhenUnlockedThisDeviceOnly;
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrLabel, kSecAttrService, kSecClass, kSecClassGenericPassword,
    kSecMatchLimit, kSecMatchLimitAll, kSecReturnAttributes, kSecReturnData, kSecValueData,
};
use security_framework_sys::keychain_item::{SecItemCopyMatching, SecItemDelete, SecItemUpdate};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

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
const ATTR_SERVICE: &str = "svce";

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

/// Switches off macOS keychain user interaction for this process.
///
/// After this call, an access that would normally make macOS ask the user for
/// the login keychain password fails with `errSecInteractionNotAllowed`
/// instead. Nothing in this project can therefore put a password prompt on
/// the user's screen.
///
/// The lock returned by `SecKeychain::disable_user_interaction` re-enables
/// interaction when it is dropped, so the guard is kept alive inside the
/// `OnceLock` for the whole life of the process. Dropping it — as a plain
/// `map(|_lock| ())` would — would silently restore the very prompt this
/// function exists to prevent.
pub fn ensure_no_user_interaction() -> Result<(), String> {
    static ONCE: OnceLock<Result<KeychainUserInteractionLock, String>> = OnceLock::new();
    ONCE.get_or_init(|| {
        SecKeychain::disable_user_interaction()
            .map_err(|e| format!("disable keychain interaction: {e}"))
    })
    .as_ref()
    .map(|_lock| ())
    .map_err(|e| e.clone())
}

/// Reports whether macOS is allowed to show a keychain user interface.
///
/// `false` means an access that would have asked the user for the login
/// keychain password fails with an error instead.
pub fn user_interaction_allowed() -> Result<bool, String> {
    SecKeychain::user_interaction_allowed().map_err(|e| format!("read keychain interaction state: {e}"))
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

/// `kSecReturnData`.
fn k_sec_return_data() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecReturnData) }
}

/// `kSecReturnAttributes`.
fn k_sec_return_attributes() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecReturnAttributes) }
}

/// `kSecMatchLimit`.
fn k_sec_match_limit() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecMatchLimit) }
}

/// `kSecMatchLimitAll`.
fn k_sec_match_limit_all() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecMatchLimitAll) }
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
    let mut query = generic_password_query(service, Some(account));
    query.set(k_sec_return_data(), CFBoolean::true_value().as_CFType());
    // The default match limit is a single item. `kSecMatchLimit` takes a
    // CFString, so it is left unset rather than passed a number.

    let mut result: core_foundation_sys::base::CFTypeRef = std::ptr::null();
    let status = unsafe {
        SecItemCopyMatching(query.to_untyped().to_immutable().as_concrete_TypeRef(), &mut result)
    };
    if status != 0 || result.is_null() {
        return None;
    }
    let data = unsafe {
        CFData::wrap_under_get_rule(result.cast::<core_foundation_sys::data::__CFData>())
    };
    Some(data.bytes().to_vec())
}

/// Deletes a generic password item. Returns whether an item was removed.
///
/// Returns `true` only when the Security framework confirms the deletion. A
/// failure — including "this binary is not trusted to touch that item" —
/// returns `false`, so a caller can never report an item as removed when it
/// is still there.
pub fn delete_generic_item(service: &str, account: &str) -> bool {
    let query = generic_password_query(service, Some(account));
    let status = unsafe {
        SecItemDelete(query.to_untyped().to_immutable().as_concrete_TypeRef())
    };
    status == 0
}

/// Updates the data of an existing generic password item.
///
/// The production write path replaces an item wholesale, matching the Swift
/// `SecItemDelete` then `SecItemAdd` sequence this port is faithful to, so
/// no caller needs an in-place update yet. The primitive is kept because it
/// is part of the Keychain backend and the test suite exercises it against
/// the real framework.
#[allow(dead_code)]
pub fn update_generic_item(
    service: &str,
    account: &str,
    data: &[u8],
) -> Result<(), KeychainError> {
    let query = generic_password_query(service, Some(account));

    let mut update = CFMutableDictionary::<CFString, CFType>::from_CFType_pairs(&[]);
    update.set(k_sec_value_data(), CFData::from_buffer(data).as_CFType());

    let status = unsafe {
        SecItemUpdate(
            query.to_untyped().to_immutable().as_concrete_TypeRef(),
            update.to_untyped().to_immutable().as_concrete_TypeRef(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(KeychainError::from_code(status))
    }
}

/// Lists the accounts of every generic password item under a service.
///
/// Returns the account name, label, and accessibility attribute of each item.
pub fn list_generic_items(service: &str) -> Vec<ItemAttributes> {
    let mut query = generic_password_query(service, None);
    query.set(
        k_sec_return_attributes(),
        CFBoolean::true_value().as_CFType(),
    );
    query.set(k_sec_match_limit(), k_sec_match_limit_all().as_CFType());

    let mut result: core_foundation_sys::base::CFTypeRef = std::ptr::null();
    let status = unsafe {
        SecItemCopyMatching(query.to_untyped().to_immutable().as_concrete_TypeRef(), &mut result)
    };
    if status != 0 || result.is_null() {
        return Vec::new();
    }

    // The result is an array of attribute dictionaries.
    let array = unsafe {
        core_foundation::array::CFArray::<core_foundation::base::CFType>::wrap_under_create_rule(
            result.cast::<core_foundation_sys::array::__CFArray>(),
        )
    };
    let mut out = Vec::new();
    for item in array.iter() {
        let dictionary = unsafe {
            core_foundation::dictionary::CFDictionary::<core_foundation::base::CFType, core_foundation::base::CFType>::wrap_under_get_rule(
                item.as_CFTypeRef().cast::<core_foundation_sys::dictionary::__CFDictionary>(),
            )
        };
        let mut account = None;
        let mut label = None;
        let mut accessible = None;
        let (keys, values) = dictionary.get_keys_and_values();
        for (key, value) in keys.iter().zip(values.iter()) {
            let key = unsafe { CFString::wrap_under_get_rule((*key).cast()) };
            let value = unsafe { CFString::wrap_under_get_rule((*value).cast()) };
            match key.to_string().as_str() {
                ATTR_ACCOUNT => account = Some(value.to_string()),
                ATTR_LABEL => label = Some(value.to_string()),
                K_SEC_ATTR_ACCESSIBLE => accessible = Some(value.to_string()),
                _ => {}
            }
        }
        let Some(account) = account else {
            continue;
        };
        out.push(ItemAttributes {
            account,
            label,
            accessible,
        });
    }
    out
}

/// Lists every generic password item in the default keychain search list.
///
/// Used by the cleanup path to find the items a previous test run left behind.
pub fn list_all_generic_items() -> Vec<(String, ItemAttributes)> {
    let mut query = CFMutableDictionary::<CFString, CFType>::from_CFType_pairs(&[]);
    query.set(k_sec_class(), k_sec_class_generic_password().as_CFType());
    query.set(
        k_sec_return_attributes(),
        CFBoolean::true_value().as_CFType(),
    );
    query.set(k_sec_match_limit(), k_sec_match_limit_all().as_CFType());

    let mut result: core_foundation_sys::base::CFTypeRef = std::ptr::null();
    let status = unsafe {
        SecItemCopyMatching(query.to_untyped().to_immutable().as_concrete_TypeRef(), &mut result)
    };
    if status != 0 || result.is_null() {
        return Vec::new();
    }

    let array = unsafe {
        core_foundation::array::CFArray::<core_foundation::base::CFType>::wrap_under_create_rule(
            result.cast::<core_foundation_sys::array::__CFArray>(),
        )
    };
    let mut out = Vec::new();
    for item in array.iter() {
        let dictionary = unsafe {
            core_foundation::dictionary::CFDictionary::<core_foundation::base::CFType, core_foundation::base::CFType>::wrap_under_get_rule(
                item.as_CFTypeRef().cast::<core_foundation_sys::dictionary::__CFDictionary>(),
            )
        };
        let mut account = None;
        let mut label = None;
        let mut service = None;
        let (keys, values) = dictionary.get_keys_and_values();
        for (key, value) in keys.iter().zip(values.iter()) {
            let key = unsafe { CFString::wrap_under_get_rule((*key).cast()) };
            let value = unsafe { CFString::wrap_under_get_rule((*value).cast()) };
            match key.to_string().as_str() {
                ATTR_ACCOUNT => account = Some(value.to_string()),
                ATTR_LABEL => label = Some(value.to_string()),
                ATTR_SERVICE => service = Some(value.to_string()),
                _ => {}
            }
        }
        let (Some(service), Some(account)) = (service, account) else {
            continue;
        };
        out.push((
            service,
            ItemAttributes {
                account,
                label,
                accessible: None,
            },
        ));
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

    /// Every keychain test runs with macOS keychain interaction switched off,
    /// so a run can never ask the user for the login keychain password.
    fn no_prompt() {
        crate::keychain::ensure_no_user_interaction()
            .expect("disable keychain interaction");
    }

    /// Each test uses a service name unique to this process, so a run never
    /// touches an item that another binary created.
    fn service_for(test: &str) -> String {
        format!(
            "ai.zmax.hermes.browser.keychain.selftest.{}.{}",
            std::process::id(),
            test
        )
    }

    #[test]
    fn round_trips_a_generic_item() {
        no_prompt();
        let service = service_for("alice");
        set_generic_item(&service, "alice", Some("example.com"), b"s3cret")
            .expect("keychain write");
        let read = get_generic_item(&service, "alice").expect("keychain read");
        assert_eq!(read, b"s3cret");
        assert!(delete_generic_item(&service, "alice"));
        assert!(get_generic_item(&service, "alice").is_none());
    }

    #[test]
    fn overwrites_an_existing_item() {
        no_prompt();
        let service = service_for("bob");
        set_generic_item(&service, "bob", Some("example.com"), b"first").unwrap();
        set_generic_item(&service, "bob", Some("example.com"), b"second").unwrap();
        let read = get_generic_item(&service, "bob").unwrap();
        assert_eq!(read, b"second");
        delete_generic_item(&service, "bob");
    }

    #[test]
    fn enumerates_items_by_service() {
        no_prompt();
        let service = service_for("carol");
        set_generic_item(&service, "carol", Some("site-a.test"), b"x").unwrap();
        set_generic_item(&service, "dave", Some("site-b.test"), b"y").unwrap();
        let items = list_generic_items(&service);
        let accounts: Vec<&str> = items.iter().map(|i| i.account.as_str()).collect();
        assert!(accounts.contains(&"carol"));
        assert!(accounts.contains(&"dave"));
        let carol = items.iter().find(|i| i.account == "carol").unwrap();
        assert_eq!(carol.label.as_deref(), Some("site-a.test"));
        delete_generic_item(&service, "carol");
        delete_generic_item(&service, "dave");
    }

    #[test]
    fn updating_replaces_the_stored_data() {
        no_prompt();
        let service = service_for("erin");
        set_generic_item(&service, "erin", Some("site.test"), b"old").unwrap();
        update_generic_item(&service, "erin", b"new").unwrap();
        assert_eq!(get_generic_item(&service, "erin").unwrap(), b"new");
        delete_generic_item(&service, "erin");
    }

    #[test]
    fn deleting_a_missing_item_reports_false() {
        no_prompt();
        assert!(!delete_generic_item(&service_for("nobody"), "nobody"));
    }

    #[test]
    fn round_trips_the_accessible_attribute() {
        no_prompt();
        let service = service_for("frank");
        set_generic_item(&service, "frank", Some("site.test"), b"v").unwrap();
        let items = list_generic_items(&service);
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
        delete_generic_item(&service, "frank");
    }

    #[test]
    fn stores_binary_data_intact() {
        no_prompt();
        let service = service_for("grace");
        let payload: Vec<u8> = (0..=255u8).collect();
        set_generic_item(&service, "grace", Some("site.test"), &payload).unwrap();
        assert_eq!(get_generic_item(&service, "grace").unwrap(), payload);
        delete_generic_item(&service, "grace");
    }

    #[test]
    fn keychain_user_interaction_is_switched_off() {
        // The guarantee is checked against the live platform, not against the
        // intent of the code. `SecKeychainGetUserInteractionAllowed` reports
        // what macOS would do if an access needed a password: with
        // interaction off it fails with an error instead of prompting.
        ensure_no_user_interaction().expect("disable keychain interaction");
        assert_eq!(
            user_interaction_allowed().expect("read interaction state"),
            false,
            "the keychain must not be allowed to prompt the user for a password"
        );
    }

    #[test]
    fn removes_the_items_earlier_test_runs_left_behind() {
        no_prompt();
        // The test binary is trusted for the items it created, so it can
        // clean up after itself. Items left by a *different* build are not
        // trusted and are reported rather than silently claimed.
        let mut removed = 0usize;
        let mut refused = 0usize;
        for (service, attributes) in list_all_generic_items() {
            if !service.starts_with("ai.zmax.hermes.browser.") {
                continue;
            }
            if delete_generic_item(&service, &attributes.account) {
                removed += 1;
            } else {
                refused += 1;
            }
        }
        eprintln!("[keychain] cleanup: {removed} removed, {refused} left in place");
    }

    #[test]
    fn reports_the_active_backend() {
        no_prompt();
        let service = service_for("backend");
        set_generic_item(&service, "backend", Some("x"), b"v").unwrap();
        eprintln!(
            "[keychain] active backend: {}",
            backend().map(|b| b.as_str()).unwrap_or("undetermined")
        );
        delete_generic_item(&service, "backend");
    }
}
