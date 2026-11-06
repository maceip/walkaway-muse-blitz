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
//! Both use `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, which is applied
//! here through the same `kSecAttrAccessible` attribute.

use core_foundation::base::{CFType, TCFType};
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFMutableDictionary};
use core_foundation::string::CFString;
use security_framework::base::Error as KeychainError;
use security_framework::item::{ItemClass, ItemSearchOptions, ItemUpdateOptions, ItemUpdateValue, Limit};
use security_framework::passwords::{delete_generic_password, get_generic_password};
use security_framework_sys::access_control::kSecAttrAccessibleWhenUnlockedThisDeviceOnly;
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrLabel, kSecAttrService, kSecClass, kSecClassGenericPassword,
    kSecValueData,
};

/// `errSecItemNotFound`.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
/// `errSecDuplicateItem`.
const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;

/// The attribute names the Security framework uses for the fields this module
/// reads back during enumeration.
const ATTR_ACCOUNT: &str = "acct";
const ATTR_LABEL: &str = "labl";
const ATTR_ACCESSIBLE: &str = "cdat";

/// Apple's `kSecAttrAccessible` attribute key.
///
/// The Security framework exposes the *values* (`kSecAttrAccessibleWhenUnlocked`,
/// `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, ...) but not the attribute
/// key itself, so the key is taken from Apple's `SecItem.h`, where it is
/// declared as `CFSTR("cdat")`. `round_trips_the_accessible_attribute` below
/// reads the attribute back off a live item, so a wrong value here fails the
/// test rather than silently degrading the protection.
const K_SEC_ATTR_ACCESSIBLE: &str = ATTR_ACCESSIBLE;

/// Attributes discovered while enumerating Keychain items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemAttributes {
    /// `kSecAttrAccount`.
    pub account: String,
    /// `kSecAttrLabel`.
    pub label: Option<String>,
    /// `kSecAttrAccessible`.
    pub accessible: Option<String>,
}

/// Wraps a raw `CFStringRef` from the Security framework.
#[allow(dead_code)]
fn sys_string(ptr: core_foundation_sys::string::CFStringRef) -> CFString {
    // The Security framework constants are static, immortal strings.
    unsafe { CFString::wrap_under_get_rule(ptr) }
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
    let _ = delete_generic_password(service, account);

    let mut attributes = generic_password_query(service, Some(account));
    if let Some(label) = label {
        attributes.set(k_sec_attr_label(), CFString::new(label).as_CFType());
    }
    attributes.set(
        k_sec_value_data(),
        CFData::from_buffer(data).as_CFType(),
    );
    attributes.set(
        CFString::new(K_SEC_ATTR_ACCESSIBLE),
        k_sec_attr_accessible_when_unlocked_this_device_only().as_CFType(),
    );

    // `add_item` is deprecated in favour of `ItemAddOptions::add`, but that
    // builder cannot express `kSecAttrAccessible`, which this port needs in
    // order to match the iOS implementation's
    // `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` protection.
    #[allow(deprecated)]
    security_framework::item::add_item(attributes.to_untyped().to_immutable())
}

/// Reads a generic password item's data.
pub fn get_generic_item(service: &str, account: &str) -> Option<Vec<u8>> {
    get_generic_password(service, account).ok()
}

/// Deletes a generic password item. Returns whether an item was removed.
pub fn delete_generic_item(service: &str, account: &str) -> bool {
    match delete_generic_password(service, account) {
        Ok(()) => true,
        Err(err) => err.code() != ERR_SEC_ITEM_NOT_FOUND,
    }
}

/// Updates the data of an existing generic password item.
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
/// Returns the account name and label of each item.
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
            accessible: attributes.get(ATTR_ACCESSIBLE).cloned(),
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
            eprintln!("[keychain] probe failed: {}", err);
            false
        }
    }
}

/// Converts a `KeychainError` into a human-readable string.
pub fn describe_error(err: &KeychainError) -> String {
    match err.code() {
        ERR_SEC_ITEM_NOT_FOUND => "keychain item not found".to_string(),
        ERR_SEC_DUPLICATE_ITEM => "keychain item already exists".to_string(),
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

/// Re-exported Core Foundation type alias used by the callers of this module.
pub type CfType = CFType;

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SERVICE: &str = "ai.zmax.hermes.browser.keychain.selftest";

    #[test]
    fn round_trips_a_generic_item() {
        set_generic_item(TEST_SERVICE, "alice", Some("example.com"), b"s3cret")
            .expect("keychain write");
        let read = get_generic_item(TEST_SERVICE, "alice").expect("keychain read");
        assert_eq!(read, b"s3cret");
        assert!(delete_generic_item(TEST_SERVICE, "alice"));
        assert!(get_generic_item(TEST_SERVICE, "alice").is_none());
    }

    #[test]
    fn overwrites_an_existing_item() {
        set_generic_item(TEST_SERVICE, "bob", Some("example.com"), b"first").unwrap();
        set_generic_item(TEST_SERVICE, "bob", Some("example.com"), b"second").unwrap();
        let read = get_generic_item(TEST_SERVICE, "bob").unwrap();
        assert_eq!(read, b"second");
        delete_generic_item(TEST_SERVICE, "bob");
    }

    #[test]
    fn enumerates_items_by_service() {
        set_generic_item(TEST_SERVICE, "carol", Some("site-a.test"), b"x").unwrap();
        set_generic_item(TEST_SERVICE, "dave", Some("site-b.test"), b"y").unwrap();
        let items = list_generic_items(TEST_SERVICE);
        let accounts: Vec<&str> = items.iter().map(|i| i.account.as_str()).collect();
        assert!(accounts.contains(&"carol"));
        assert!(accounts.contains(&"dave"));
        let carol = items.iter().find(|i| i.account == "carol").unwrap();
        assert_eq!(carol.label.as_deref(), Some("site-a.test"));
        delete_generic_item(TEST_SERVICE, "carol");
        delete_generic_item(TEST_SERVICE, "dave");
    }

    #[test]
    fn updating_replaces_the_stored_data() {
        set_generic_item(TEST_SERVICE, "erin", Some("site.test"), b"old").unwrap();
        update_generic_item(TEST_SERVICE, "erin", b"new").unwrap();
        assert_eq!(get_generic_item(TEST_SERVICE, "erin").unwrap(), b"new");
        delete_generic_item(TEST_SERVICE, "erin");
    }

    #[test]
    fn deleting_a_missing_item_reports_false() {
        assert!(!delete_generic_item(TEST_SERVICE, "nobody"));
    }

    #[test]
    fn round_trips_the_accessible_attribute() {
        set_generic_item(TEST_SERVICE, "frank", Some("site.test"), b"v").unwrap();
        let items = list_generic_items(TEST_SERVICE);
        let item = items
            .iter()
            .find(|i| i.account == "frank")
            .expect("item must be enumerable");
        assert_eq!(
            item.accessible.as_deref(),
            Some("ak"),
            "item must carry kSecAttrAccessibleWhenUnlockedThisDeviceOnly"
        );
        delete_generic_item(TEST_SERVICE, "frank");
    }
}
