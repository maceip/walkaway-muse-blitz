//! Host-managed local credential vault (`HERMES_VAULT_PATH`).
//!
//! Generates and persists per-(origin, username) credentials on the host side
//! and injects secrets directly into Blitz DOM input controls during an
//! approved `fill_form` (`action: "fill_secret"`) command. Plaintext secrets
//! never cross the agent/host control socket, never appear in `observe.result`
//! snapshots, and never enter audit logs.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::control;

const SECRET_ALPHABET: &[u8] =
    b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#%^&*-_=+";
const SECRET_LEN: usize = 24;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultAccount {
    pub account_id: String,
    pub origin: String,
    pub username: String,
    pub secret: String,
    pub created_at: String,
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
    serde_json::from_str::<VaultStore>(&raw).unwrap_or_default()
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
/// high-entropy secret, persisting the updated store to `HERMES_VAULT_PATH`.
pub fn get_or_create_secret(origin: &str, username: &str) -> Result<VaultAccount, String> {
    let mut store = load_store();
    if let Some(existing) = store
        .accounts
        .iter()
        .find(|a| a.origin == origin && a.username == username)
        .cloned()
    {
        return Ok(existing);
    }
    let secret = generate_random_secret()?;
    let account = VaultAccount {
        account_id: format!("acct_{}", store.accounts.len() + 1),
        origin: origin.to_string(),
        username: username.to_string(),
        secret,
        created_at: control::now_iso(),
    };
    store.accounts.push(account.clone());
    save_store(&store)?;
    Ok(account)
}
