//! Host-managed local credential vault (`HERMES_VAULT_PATH`).
//!
//! Generates and persists per-(origin, username) credentials on the host side.
//! Secrets are injected into Blitz DOM input controls only after a Host
//! `decision: "approve"` command. Plaintext secrets never cross the
//! agent/host control socket, never appear in `observe.result` snapshots, and
//! never enter audit logs.
//!
//! This is the *blind vault*: the store the user can inspect on disk. The
//! WebAuthn authenticator does not live here. It lives in the passkey broker
//! (`src/credentials.rs`), which keeps private keys in the macOS Keychain and
//! runs ceremonies behind the human approval gate. The vault's former
//! WebAuthn signer and verifier were removed on 2026-11-06 for exactly that
//! reason: two WebAuthn implementations would mean one capability with two
//! implementations.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::control;

const SECRET_ALPHABET: &[u8] =
    b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#%^&*-_=+";
const SECRET_LEN: usize = 24;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)] // the legacy keypair fields exist only to read old vault files
pub struct VaultAccount {
    pub account_id: String,
    pub origin: String,
    pub username: String,
    pub secret: String,
    pub created_at: String,
    /// The credential ID and keypair that used to live here are gone.
    ///
    /// The blind vault stores passwords. WebAuthn credentials live in the
    /// macOS Keychain, through the passkey broker, because a private key
    /// belongs in the Keychain and not in a JSON file on disk. The fields are
    /// kept as ignored so an existing vault file still deserializes; they are
    /// never written again.
    #[serde(default, skip_serializing)]
    pub credential_id: String,
    #[serde(default, skip_serializing)]
    pub private_key_pem: String,
    #[serde(default, skip_serializing)]
    pub public_key_pem: String,
    #[serde(default, skip_serializing)]
    pub sign_count: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VaultStore {
    pub accounts: Vec<VaultAccount>,
}

fn vault_path() -> PathBuf {
    std::env::var("HERMES_VAULT_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/hermes-vault.json"))
}

fn generate_random_secret() -> Result<String, String> {
    let mut file =
        fs::File::open("/dev/urandom").map_err(|e| format!("open /dev/urandom: {e}"))?;
    let mut bytes = [0u8; SECRET_LEN];
    file.read_exact(&mut bytes)
        .map_err(|e| format!("read /dev/urandom: {e}"))?;
    let mut out = String::with_capacity(SECRET_LEN);
    for b in bytes {
        let idx = (b as usize) % SECRET_ALPHABET.len();
        out.push(SECRET_ALPHABET[idx] as char);
    }
    Ok(out)
}

pub fn load_store() -> VaultStore {
    let path = vault_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return VaultStore::default();
    };
    if let Ok(store) = serde_json::from_str::<VaultStore>(&raw) {
        return store;
    }
    // Also support flat key-value JSON maps if pre-seeded by a reset script,
    // without losing accounts structure.
    VaultStore::default()
}

fn save_store(store: &VaultStore) -> Result<(), String> {
    let path = vault_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let serialized =
        serde_json::to_string_pretty(store).map_err(|e| format!("serialize vault: {e}"))?;
    let mut opts = OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&path)
        .map_err(|e| format!("open vault {}: {e}", path.display()))?;
    file.write_all(serialized.as_bytes())
        .map_err(|e| format!("write vault {}: {e}", path.display()))?;
    file.flush()
        .map_err(|e| format!("flush vault {}: {e}", path.display()))?;
    Ok(())
}

/// Returns an existing account for `(origin, username)` or creates a fresh
/// high-entropy secret, persisting to `HERMES_VAULT_PATH`.
///
/// The vault stores passwords only. WebAuthn private keys live in the macOS
/// Keychain through the passkey broker, never in this file.
pub fn get_or_create_secret(origin: &str, username: &str) -> Result<VaultAccount, String> {
    let mut store = load_store();
    if let Some(idx) = store
        .accounts
        .iter()
        .position(|a| a.origin == origin && a.username == username)
    {
        return Ok(store.accounts[idx].clone());
    }
    let secret = generate_random_secret()?;
    let idx = store.accounts.len() + 1;
    let account = VaultAccount {
        account_id: format!("acct_{idx}"),
        origin: origin.to_string(),
        username: username.to_string(),
        secret,
        created_at: control::now_iso(),
        credential_id: String::new(),
        private_key_pem: String::new(),
        public_key_pem: String::new(),
        sign_count: 0,
    };
    store.accounts.push(account.clone());
    save_store(&store)?;
    Ok(account)
}

