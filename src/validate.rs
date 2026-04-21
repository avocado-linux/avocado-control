//! AMF (Avocado Runtime Manifest) validator.
//!
//! Ports the KOS boot-time KMF verification into a standalone avocadoctl
//! command. The three checks we run, in order:
//!
//! 1. **Signature + chain** — the manifest's `meta.auth.signature` must
//!    verify against the public key in `meta.auth.certificates[0]` (the
//!    leaf), and that leaf must chain back to a trusted root in the
//!    supplied `--ca-path` directory. Mirrors
//!    [`kyanite_utils::kmf::Kmf::verify_signature`] and
//!    [`verify_cert_chain`] semantics — SHA256 + RSA PKCS#1 v1.5 over the
//!    canonical form of the manifest (the JSON with the `meta` key
//!    stripped), same `X509VerifyFlags::PARTIAL_CHAIN | NO_CHECK_TIME`
//!    flags on the store.
//!
//! 2. **Leaf CN policy** — re-implements kos_bootcert's CN-glob match.
//!    CN format is `<mode>.<vendor>[.<mfg>]`. Accepted globs depend on
//!    `/run/kos/mode` (prod vs test; prod is the default if the file is
//!    missing/unreadable) and `/run/kos/mfgAuthority` (optional
//!    manufacturer scope). Match table:
//!
//!    | mfgAuth | mode | allowed CNs |
//!    | ---     | ---  | ---         |
//!    | unset   | prod | `prod.*` |
//!    | unset   | test | `prod.*`, `test.*` |
//!    | `M`     | prod | `prod.M`, `prod.*.M` |
//!    | `M`     | test | `prod.M`, `prod.*.M`, `test.M`, `test.*.M` |
//!
//! 3. **Component presence** — every component in the manifest has its
//!    image file on disk at the content-addressable path (or the
//!    name-version fallback), matching `staging::validate_manifest_images`.
//!
//! Exit status: `Ok(())` on full success; `Err(ValidateError)` on any
//! failure. The caller maps this to an exit code.

use crate::manifest::{RuntimeManifest, MANIFEST_FILENAME};
use openssl::hash::MessageDigest;
use openssl::rsa::Padding;
use openssl::sign::Verifier;
use openssl::ssl::SslFiletype;
use openssl::stack::Stack;
use openssl::x509::store::{X509Lookup, X509StoreBuilder};
use openssl::x509::verify::X509VerifyFlags;
use openssl::x509::{X509StoreContext, X509};
use serde_json::Value;
use std::fs;
use std::path::Path;
use thiserror::Error;

const MODE_FILE: &str = "/run/kos/mode";
const MFG_AUTHORITY_FILE: &str = "/run/kos/mfgAuthority";

#[derive(Debug, Error)]
pub enum ValidateError {
    #[error("active manifest not found at {0}")]
    ManifestMissing(String),
    #[error("failed to read manifest {0}: {1}")]
    ManifestRead(String, std::io::Error),
    #[error("manifest is not valid JSON: {0}")]
    ManifestParse(serde_json::Error),
    #[error("manifest has no meta.auth block (unsigned)")]
    MissingAuth,
    #[error("manifest cert chain is empty")]
    EmptyCertChain,
    #[error("failed to base64-decode auth field: {0}")]
    Base64(base64::DecodeError),
    #[error("failed to parse DER certificate: {0}")]
    CertParse(openssl::error::ErrorStack),
    #[error("openssl error: {0}")]
    OpenSsl(openssl::error::ErrorStack),
    #[error("signature verification failed")]
    BadSignature,
    #[error("certificate chain does not reach a trusted root in {0}")]
    UntrustedChain(String),
    #[error("leaf certificate CN '{0}' not accepted by policy (mode={1:?}, mfgAuthority={2:?})")]
    CnRejected(String, String, Option<String>),
    #[error("component '{name}' image missing at {path}")]
    ComponentMissing { name: String, path: String },
    #[error("could not extract CN from leaf certificate subject")]
    NoCn,
}

impl From<openssl::error::ErrorStack> for ValidateError {
    fn from(e: openssl::error::ErrorStack) -> Self {
        ValidateError::OpenSsl(e)
    }
}

/// Public entrypoint. Validates the active manifest at
/// `<base_dir>/active/manifest.json`.
///
/// * `base_dir`: usually `/var/lib/avocado` (from `RuntimeManifest::base_dir()`).
/// * `ca_path`: directory containing trusted root PEM certs. Must be
///   OpenSSL-hash-formatted (`c_rehash` or `openssl rehash`).
pub fn validate_active_manifest(base_dir: &Path, ca_path: &Path) -> Result<(), ValidateError> {
    let manifest_path = base_dir.join("active").join(MANIFEST_FILENAME);
    if !manifest_path.exists() {
        return Err(ValidateError::ManifestMissing(
            manifest_path.display().to_string(),
        ));
    }
    let raw = fs::read_to_string(&manifest_path)
        .map_err(|e| ValidateError::ManifestRead(manifest_path.display().to_string(), e))?;

    let value: Value = serde_json::from_str(&raw).map_err(ValidateError::ManifestParse)?;

    // 1. Signature + chain.
    verify_signature_and_chain(&value, ca_path)?;

    // 2. CN policy (kos_bootcert semantics, reading /run/kos/mode).
    let leaf_cn = extract_leaf_cn(&value)?;
    enforce_cn_policy(&leaf_cn)?;

    // 3. All components present on disk.
    // Parse into the typed RuntimeManifest so we get the same path-
    // resolution logic as the staging code.
    let manifest: RuntimeManifest =
        serde_json::from_str(&raw).map_err(ValidateError::ManifestParse)?;
    verify_components_present(&manifest, base_dir)?;

    Ok(())
}

// ── 1. signature + chain ──────────────────────────────────────────────

/// Canonicalize the manifest the same way kyanite's
/// `Kmf::to_canonical_string()` does: strip the top-level `meta` key,
/// then serialize compact (no whitespace), preserving original key order.
/// Returns the bytes that the stored signature was computed over.
fn canonical_bytes(value: &Value) -> Vec<u8> {
    let obj = value.as_object().expect("manifest root must be a JSON object");
    // Re-emit a fresh object preserving insertion order, dropping `meta`.
    // serde_json's default serializer is compact when called without the
    // `pretty` variant; we want no whitespace.
    let filtered: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "meta")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let canonical_value = Value::Object(filtered);
    serde_json::to_string(&canonical_value)
        .expect("canonical manifest serialize failed")
        .into_bytes()
}

fn verify_signature_and_chain(value: &Value, ca_path: &Path) -> Result<(), ValidateError> {
    let auth = value
        .get("meta")
        .and_then(|m| m.get("auth"))
        .ok_or(ValidateError::MissingAuth)?;

    let sig_b64 = auth
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or(ValidateError::MissingAuth)?;
    let certs = auth
        .get("certificates")
        .and_then(|v| v.as_array())
        .ok_or(ValidateError::MissingAuth)?;
    if certs.is_empty() {
        return Err(ValidateError::EmptyCertChain);
    }

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let sig = b64.decode(sig_b64).map_err(ValidateError::Base64)?;

    let mut chain: Vec<X509> = Vec::with_capacity(certs.len());
    for c in certs {
        let c_str = c.as_str().ok_or(ValidateError::EmptyCertChain)?;
        let der = b64.decode(c_str).map_err(ValidateError::Base64)?;
        let cert = X509::from_der(&der).map_err(ValidateError::CertParse)?;
        chain.push(cert);
    }
    let leaf = &chain[0];

    // --- signature ---
    let pubkey = leaf.public_key()?;
    let mut verifier = Verifier::new(MessageDigest::sha256(), &pubkey)?;
    verifier.set_rsa_padding(Padding::PKCS1)?;
    let canonical = canonical_bytes(value);
    verifier.update(&canonical)?;
    if !verifier.verify(&sig)? {
        return Err(ValidateError::BadSignature);
    }

    // --- chain ---
    // Intermediate certs go in the "untrusted" stack; the trust store
    // holds the root(s) discovered via HashDir lookup in `ca_path`.
    let mut untrusted = Stack::<X509>::new()?;
    for c in &chain {
        untrusted.push(c.clone())?;
    }

    let mut builder = X509StoreBuilder::new()?;
    let lookup = builder.add_lookup(X509Lookup::hash_dir())?;
    lookup.add_dir(&ca_path.display().to_string(), SslFiletype::PEM)?;
    // PARTIAL_CHAIN: trust any cert in the store, not only self-signed roots.
    // NO_CHECK_TIME: the runtime mode gate doesn't fail a device simply
    // because its clock skewed — we mirror KOS's posture here.
    builder.set_flags(X509VerifyFlags::PARTIAL_CHAIN | X509VerifyFlags::NO_CHECK_TIME)?;
    let trust = builder.build();

    let mut ctx = X509StoreContext::new()?;
    let trusted = ctx.init(&trust, leaf, &untrusted, |c| c.verify_cert())?;
    if !trusted {
        return Err(ValidateError::UntrustedChain(
            ca_path.display().to_string(),
        ));
    }
    Ok(())
}

// ── 2. leaf-CN policy (kos_bootcert port) ─────────────────────────────

fn extract_leaf_cn(value: &Value) -> Result<String, ValidateError> {
    use base64::Engine as _;
    let leaf_b64 = value
        .get("meta")
        .and_then(|m| m.get("auth"))
        .and_then(|a| a.get("certificates"))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .ok_or(ValidateError::EmptyCertChain)?;
    let der = base64::engine::general_purpose::STANDARD
        .decode(leaf_b64)
        .map_err(ValidateError::Base64)?;
    let cert = X509::from_der(&der).map_err(ValidateError::CertParse)?;
    // X509::subject_name() gives an X509Name; iterate entries looking for CN (NID 13).
    let subj = cert.subject_name();
    for entry in subj.entries_by_nid(openssl::nid::Nid::COMMONNAME) {
        if let Ok(s) = entry.data().as_utf8() {
            return Ok(s.to_string());
        }
    }
    Err(ValidateError::NoCn)
}

fn read_mode() -> String {
    // `/run/kos/mode` is the single source of truth. Missing / unreadable
    // → fail closed to prod (stricter policy, fewer accepted CNs).
    fs::read_to_string(MODE_FILE)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| s == "test" || s == "prod")
        .unwrap_or_else(|| "prod".to_string())
}

fn read_mfg_authority() -> Option<String> {
    fs::read_to_string(MFG_AUTHORITY_FILE)
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
}

fn enforce_cn_policy(cn: &str) -> Result<(), ValidateError> {
    let mode = read_mode();
    let is_test = mode == "test";
    let mfg = read_mfg_authority();

    // Build the same glob set kos_bootcert builds.
    let mut patterns: Vec<glob::Pattern> = Vec::with_capacity(4);
    if let Some(ref m) = mfg {
        patterns.push(glob::Pattern::new(&format!("prod.{m}")).expect("valid glob"));
        patterns.push(glob::Pattern::new(&format!("prod.*.{m}")).expect("valid glob"));
        if is_test {
            patterns.push(glob::Pattern::new(&format!("test.{m}")).expect("valid glob"));
            patterns.push(glob::Pattern::new(&format!("test.*.{m}")).expect("valid glob"));
        }
    } else {
        patterns.push(glob::Pattern::new("prod.*").expect("valid glob"));
        if is_test {
            patterns.push(glob::Pattern::new("test.*").expect("valid glob"));
        }
    }

    if patterns.iter().any(|p| p.matches(cn)) {
        Ok(())
    } else {
        Err(ValidateError::CnRejected(cn.to_string(), mode, mfg))
    }
}

// ── 3. component presence ─────────────────────────────────────────────

fn verify_components_present(
    manifest: &RuntimeManifest,
    base_dir: &Path,
) -> Result<(), ValidateError> {
    for comp in &manifest.components {
        let p = comp.resolve_path(base_dir);
        if !p.exists() {
            return Err(ValidateError::ComponentMissing {
                name: comp.name.clone(),
                path: p.display().to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_strips_meta_and_compacts() {
        // Matches the JSON produced by avocado-cli's signing block.
        let v: Value = serde_json::from_str(
            r#"{
                "manifest_version": 3,
                "id": "abc",
                "components": [],
                "meta": { "auth": { "signature": "sig", "certificates": [] } }
            }"#,
        )
        .unwrap();
        let s = String::from_utf8(canonical_bytes(&v)).unwrap();
        assert!(!s.contains("meta"));
        assert!(!s.contains(' '));
        assert!(s.starts_with(r#"{"manifest_version":3"#));
    }

    #[test]
    fn test_cn_policy_prod_no_mfg() {
        // Emulate mode=prod with no mfgAuthority by temporarily running
        // the logic directly rather than going through read_mode (which
        // reads /run/kos). We just verify the glob logic.
        let patterns = vec![glob::Pattern::new("prod.*").unwrap()];
        assert!(patterns.iter().any(|p| p.matches("prod.peridio")));
        assert!(!patterns.iter().any(|p| p.matches("test.peridio")));
    }

    #[test]
    fn test_cn_policy_test_mode_accepts_both() {
        let patterns = vec![
            glob::Pattern::new("prod.*").unwrap(),
            glob::Pattern::new("test.*").unwrap(),
        ];
        assert!(patterns.iter().any(|p| p.matches("prod.peridio")));
        assert!(patterns.iter().any(|p| p.matches("test.peridio")));
    }

    #[test]
    fn test_cn_policy_mfg_scoped() {
        let patterns = vec![
            glob::Pattern::new("prod.coke").unwrap(),
            glob::Pattern::new("prod.*.coke").unwrap(),
        ];
        assert!(patterns.iter().any(|p| p.matches("prod.coke")));
        assert!(patterns.iter().any(|p| p.matches("prod.peridio.coke")));
        assert!(!patterns.iter().any(|p| p.matches("prod.peridio")));
        assert!(!patterns.iter().any(|p| p.matches("prod.peridio.other")));
    }
}
