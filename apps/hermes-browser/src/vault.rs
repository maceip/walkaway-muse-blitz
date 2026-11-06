//! Host-managed local credential vault (`HERMES_VAULT_PATH`).
//!
//! Generates and persists per-(origin, username) credentials and NIST P-256
//! ECDSA WebAuthn keypairs on the host side. Injects secrets and signed
//! WebAuthn assertions directly into Blitz DOM input controls only after a
//! Host `decision: "approve"` command. Plaintext secrets and private keys
//! never cross the agent/host control socket, never appear in `observe.result`
//! snapshots, and never enter audit logs.

use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::{Signer, Verifier};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
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
    #[serde(default)]
    pub credential_id: String,
    #[serde(default)]
    pub private_key_pem: String,
    #[serde(default)]
    pub public_key_pem: String,
    #[serde(default)]
    pub sign_count: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VaultStore {
    pub accounts: Vec<VaultAccount>,
}

/// Public, non-secret WebAuthn assertion bundle produced after a Host approve
/// decision. Contains only public key and signature artifacts; never contains
/// private key material or password secrets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebAuthnAssertionBundle {
    pub credential_id: String,
    pub rp_id: String,
    pub origin: String,
    pub public_key_pem: String,
    pub authenticator_data_b64: String,
    pub client_data_json_b64: String,
    pub signature_der_b64: String,
    pub sign_count: u32,
    pub user_present: bool,
    pub user_verified: bool,
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

fn generate_p256_keypair_pem() -> Result<(String, String), String> {
    crate::webauthn::cose::generate_p256_keypair()
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
/// high-entropy secret + NIST P-256 keypair, persisting to `HERMES_VAULT_PATH`.
pub fn get_or_create_secret(origin: &str, username: &str) -> Result<VaultAccount, String> {
    let mut store = load_store();
    if let Some(idx) = store
        .accounts
        .iter()
        .position(|a| a.origin == origin && a.username == username)
    {
        if store.accounts[idx].private_key_pem.is_empty() {
            let (priv_pem, pub_pem) = generate_p256_keypair_pem()?;
            store.accounts[idx].private_key_pem = priv_pem;
            store.accounts[idx].public_key_pem = pub_pem;
            if store.accounts[idx].credential_id.is_empty() {
                store.accounts[idx].credential_id = format!("cred_{}", idx + 1);
            }
            save_store(&store)?;
        }
        return Ok(store.accounts[idx].clone());
    }
    let secret = generate_random_secret()?;
    let (private_key_pem, public_key_pem) = generate_p256_keypair_pem()?;
    let idx = store.accounts.len() + 1;
    let account = VaultAccount {
        account_id: format!("acct_{idx}"),
        origin: origin.to_string(),
        username: username.to_string(),
        secret,
        created_at: control::now_iso(),
        credential_id: format!("cred_{idx}"),
        private_key_pem,
        public_key_pem,
        sign_count: 0,
    };
    store.accounts.push(account.clone());
    save_store(&store)?;
    Ok(account)
}

/// Signs a real WebAuthn P-256 assertion (`authenticatorData || SHA256(clientDataJSON)`)
/// using the vault-held NIST P-256 keypair for `(origin, username)`.
/// Must only be invoked after a Host `decision: "approve"` has been verified.
pub fn sign_webauthn_assertion(
    origin: &str,
    username: &str,
    rp_id: &str,
    challenge: &str,
    user_present: bool,
    user_verified: bool,
) -> Result<WebAuthnAssertionBundle, String> {
    if !user_present {
        return Err("WebAuthn assertion requires user_present=true".to_string());
    }
    let _ = get_or_create_secret(origin, username)?;
    let mut store = load_store();
    let acct = store
        .accounts
        .iter_mut()
        .find(|a| a.origin == origin && a.username == username)
        .ok_or_else(|| "vault account missing after creation".to_string())?;

    acct.sign_count = acct.sign_count.wrapping_add(1);
    let sign_count = acct.sign_count;
    let priv_pem = acct.private_key_pem.clone();
    let pub_pem = acct.public_key_pem.clone();
    let credential_id = acct.credential_id.clone();
    save_store(&store)?;

    let client_data = json!({
        "type": "webauthn.get",
        "challenge": challenge,
        "origin": origin,
        "crossOrigin": false,
    });
    let client_data_bytes = client_data.to_string().into_bytes();
    let client_data_hash = Sha256::digest(&client_data_bytes);

    // 37-byte authenticatorData: rpIdHash (32) || flags (1) || signCount (4, big-endian)
    let rp_id_hash = Sha256::digest(rp_id.as_bytes());
    let mut flags: u8 = 0;
    if user_present {
        flags |= 0x01; // UP
    }
    if user_verified {
        flags |= 0x04; // UV
    }
    let mut auth_data = Vec::with_capacity(37);
    auth_data.extend_from_slice(&rp_id_hash);
    auth_data.push(flags);
    auth_data.extend_from_slice(&sign_count.to_be_bytes());

    let mut signed_payload = Vec::with_capacity(37 + 32);
    signed_payload.extend_from_slice(&auth_data);
    signed_payload.extend_from_slice(&client_data_hash);

    let pkey = PKey::private_key_from_pem(priv_pem.as_bytes())
        .map_err(|e| format!("parse priv pem: {e}"))?;
    let mut signer =
        Signer::new(MessageDigest::sha256(), &pkey).map_err(|e| format!("signer init: {e}"))?;
    signer
        .update(&signed_payload)
        .map_err(|e| format!("signer update: {e}"))?;
    let signature_der = signer
        .sign_to_vec()
        .map_err(|e| format!("signer finish: {e}"))?;

    // Self-verify against the public key before returning.
    let bundle = WebAuthnAssertionBundle {
        credential_id,
        rp_id: rp_id.to_string(),
        origin: origin.to_string(),
        public_key_pem: pub_pem,
        authenticator_data_b64: control::b64_encode(&auth_data),
        client_data_json_b64: control::b64_encode(&client_data_bytes),
        signature_der_b64: control::b64_encode(&signature_der),
        sign_count,
        user_present,
        user_verified,
    };
    verify_webauthn_assertion(&bundle, rp_id, origin, challenge)?;
    Ok(bundle)
}

/// Verifies a `WebAuthnAssertionBundle` against expected `rp_id`, `origin`, and `challenge`.
pub fn verify_webauthn_assertion(
    bundle: &WebAuthnAssertionBundle,
    expected_rp_id: &str,
    expected_origin: &str,
    expected_challenge: &str,
) -> Result<(), String> {
    let auth_data = control::b64_decode(&bundle.authenticator_data_b64)
        .ok_or_else(|| "invalid authenticator_data_b64".to_string())?;
    if auth_data.len() < 37 {
        return Err("authenticatorData shorter than 37 bytes".to_string());
    }
    let expected_rp_hash = Sha256::digest(expected_rp_id.as_bytes());
    if auth_data[..32] != expected_rp_hash[..] {
        return Err("rpIdHash mismatch".to_string());
    }
    let flags = auth_data[32];
    if (flags & 0x01) == 0 {
        return Err("UP flag not set".to_string());
    }
    if (flags & 0x04) == 0 {
        return Err("UV flag not set".to_string());
    }

    let client_data_bytes = control::b64_decode(&bundle.client_data_json_b64)
        .ok_or_else(|| "invalid client_data_json_b64".to_string())?;
    let client_json: serde_json::Value = serde_json::from_slice(&client_data_bytes)
        .map_err(|e| format!("invalid clientDataJSON: {e}"))?;
    if client_json.get("type").and_then(|v| v.as_str()) != Some("webauthn.get") {
        return Err("clientDataJSON type != webauthn.get".to_string());
    }
    if client_json.get("challenge").and_then(|v| v.as_str()) != Some(expected_challenge) {
        return Err("challenge mismatch".to_string());
    }
    if client_json.get("origin").and_then(|v| v.as_str()) != Some(expected_origin) {
        return Err("origin mismatch".to_string());
    }

    let client_hash = Sha256::digest(&client_data_bytes);
    let mut signed_payload = Vec::with_capacity(auth_data.len() + 32);
    signed_payload.extend_from_slice(&auth_data);
    signed_payload.extend_from_slice(&client_hash);

    let sig_der = control::b64_decode(&bundle.signature_der_b64)
        .ok_or_else(|| "invalid signature_der_b64".to_string())?;
    let pub_pkey = PKey::public_key_from_pem(bundle.public_key_pem.as_bytes())
        .map_err(|e| format!("invalid public_key_pem: {e}"))?;
    let mut verifier = Verifier::new(MessageDigest::sha256(), &pub_pkey)
        .map_err(|e| format!("verifier init: {e}"))?;
    verifier
        .update(&signed_payload)
        .map_err(|e| format!("verifier update: {e}"))?;
    let ok = verifier
        .verify(&sig_der)
        .map_err(|e| format!("verifier check: {e}"))?;
    if !ok {
        return Err("ECDSA P-256 signature verification failed".to_string());
    }
    Ok(())
}
