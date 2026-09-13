//! Publisher signatures: a manifest's `trust` table held against a release.
//!
//! A checksum says the download is the file the release lists. A signature
//! says who published it. Three verifiers, each fed only from the manifest:
//!
//! - **sigstore** — a bundle checked offline against the public-good trusted
//!   root embedded in ketch, including the Rekor signed entry timestamp,
//!   which sigstore-rs does not check yet and which is what makes the
//!   log's timestamp (and so the certificate's validity window) mean anything;
//! - **minisign** — against the key the manifest pins;
//! - **gpg** — against the key block the manifest carries, pinned by its full
//!   fingerprint. No keyring is ever read: a key imported for some other
//!   reason is not this publisher's authorisation.
//!
//! Nothing a signature file says about itself reaches the terminal or the
//! log — minisign comments, OpenPGP user IDs, certificate fields and the
//! verifiers' own error detail are all chosen by whoever made the file. What
//! is reported is what the manifest pinned.

use crate::error::{Error, Result};
use crate::model::{Provenance, Release, ReleaseAsset, TrustMode, TrustPolicy, Verifier};
use crate::source::Source;
use crate::ui;
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::packet::{Signature, SignatureType};
use pgp::types::KeyDetails;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

/// Largest sidecar or signed checksum list ketch reads. Real ones are a few
/// KiB; a bundle with a full certificate chain is still far below this.
const MAX_SIDECAR: u64 = 1 << 20;

/// Sigstore's public-good trusted root, as sigstore-rs 0.14 ships it.
// ponytail: embedded, never refreshed over TUF. Fulcio and Rekor keys rotate
// rarely and a rotation fails verification rather than passing it; replace
// this file from sigstore/root-signing when that happens.
const SIGSTORE_TRUSTED_ROOT: &str = include_str!("sigstore-trusted-root.json");

/// Check what serde cannot about a `trust` table. Called from
/// `Manifest::validate`, so a registry entry with a broken policy is refused
/// before anyone tries to install it.
pub fn check_policy(policy: &TrustPolicy) -> Result<()> {
    let applies: &[&str] = match policy.verifier {
        Verifier::Sigstore => &["issuer", "identity", "repository"],
        Verifier::Minisign => &["public_key"],
        Verifier::Gpg => &["public_key", "fingerprint"],
    };
    // A key the verifier ignores would read as a check and be none.
    for (key, present) in [
        ("issuer", policy.issuer.is_some()),
        ("identity", policy.identity.is_some()),
        ("repository", policy.repository.is_some()),
        ("public_key", policy.public_key.is_some()),
        ("fingerprint", policy.fingerprint.is_some()),
    ] {
        if present && !applies.contains(&key) {
            return Err(Error::msg(format!(
                "`trust.{key}` does not apply to the {} verifier",
                policy.verifier
            )));
        }
    }
    match policy.verifier {
        Verifier::Sigstore => sigstore_policy(policy),
        Verifier::Minisign => minisign_key(policy).map(|_| ()),
        Verifier::Gpg => gpg_key(policy).map(|_| ()),
    }
}

/// Hold a downloaded asset to the manifest's `trust` table.
///
/// `Ok(None)` when there is no policy, or when a `warn` policy could not be
/// satisfied — the install then records itself as checksum-only. A `require`
/// policy that cannot be satisfied is an error whatever the reason: no
/// sidecar, a bad signature, the wrong signer, or a verifier that could not
/// finish. Fail closed.
pub fn verify(
    policy: Option<&TrustPolicy>,
    source: &dyn Source,
    release: &Release,
    asset: &ReleaseAsset,
    download: &Path,
    sha256: &str,
    staging: &Path,
) -> Result<Option<Provenance>> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    match check(policy, source, release, asset, download, sha256, staging) {
        Ok(provenance) => {
            ui::step(
                "verified",
                &format!(
                    "{} {} signature by {}",
                    asset.name, policy.verifier, provenance.identity
                ),
            );
            Ok(Some(provenance))
        }
        Err(reason) => refuse(policy, &asset.name, &reason.to_string()),
    }
}

/// A policy that could not be satisfied: an error, or a warning when the
/// manifest asked for one.
pub fn refuse(policy: &TrustPolicy, what: &str, reason: &str) -> Result<Option<Provenance>> {
    match policy.mode {
        TrustMode::Require => Err(Error::msg(format!(
            "{what}: the {} signature the manifest requires could not be verified: {reason}",
            policy.verifier
        ))),
        TrustMode::Warn => {
            ui::warn(&format!(
                "{what}: {} signature not verified ({reason}); installing on its checksum alone",
                policy.verifier
            ));
            Ok(None)
        }
    }
}

fn check(
    policy: &TrustPolicy,
    source: &dyn Source,
    release: &Release,
    asset: &ReleaseAsset,
    download: &Path,
    sha256: &str,
    staging: &Path,
) -> Result<Provenance> {
    // Sidecars get a directory of their own, so no name they carry can land
    // on the downloaded asset.
    let dir = staging.join("trust");
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;

    // What the signature covers: the asset, or a checksum list naming it.
    let (signed_name, signed_path, signed) = match &policy.signed {
        None => (asset.name.clone(), download.to_path_buf(), None),
        Some(pattern) => {
            let list = only_asset(
                release,
                |a| a.name != asset.name && crate::model::glob_match(pattern, &a.name),
                &format!("checksum list matching `{pattern}`"),
            )?;
            let (path, _) = fetch(source, list, &dir)?;
            let body = read_capped(&path)?;
            let listed =
                crate::source::github::parse_checksum_file(&String::from_utf8_lossy(&body));
            match listed.get(&asset.name) {
                Some(hex) if hex.eq_ignore_ascii_case(sha256) => {}
                Some(_) => {
                    return Err(Error::msg(format!(
                        "{} lists a different checksum for {}",
                        list.name, asset.name
                    )))
                }
                None => {
                    return Err(Error::msg(format!(
                        "{} does not list {}",
                        list.name, asset.name
                    )))
                }
            }
            (list.name.clone(), path, Some(list.name.clone()))
        }
    };

    let wanted = policy.signature_name(&signed_name);
    let sidecar = only_asset(release, |a| a.name == wanted, &format!("`{wanted}`"))?;
    let (sidecar_path, sidecar_sha256) = fetch(source, sidecar, &dir)?;
    let bytes = read_capped(&sidecar_path)?;

    let (identity, log_index) = match policy.verifier {
        Verifier::Sigstore => sigstore_check(policy, &bytes, &signed_path)?,
        Verifier::Minisign => (minisign_check(policy, &bytes, &signed_path)?, None),
        Verifier::Gpg => (
            gpg_check(policy, &bytes, &signed_path, crate::model::now_unix())?,
            None,
        ),
    };
    Ok(Provenance {
        verifier: policy.verifier,
        // Pinned by a manifest ketch did not write, and bound for the log
        // file, which `ui` does not filter: cleaned once, here.
        identity: ui::printable(&identity),
        signature: sidecar.name.clone(),
        signature_sha256: sidecar_sha256,
        signed,
        log_index,
    })
}

/// The one release asset `wanted` picks. Two would leave the choice of which
/// file to trust to the release, so that is refused too.
fn only_asset<'a>(
    release: &'a Release,
    wanted: impl Fn(&ReleaseAsset) -> bool,
    what: &str,
) -> Result<&'a ReleaseAsset> {
    let mut hits = release.assets.iter().filter(|a| wanted(a));
    match (hits.next(), hits.next()) {
        (Some(one), None) => Ok(one),
        (None, _) => Err(Error::msg(format!(
            "release {} publishes no {what}",
            release.tag
        ))),
        (Some(_), Some(_)) => Err(Error::msg(format!(
            "release {} publishes more than one {what}",
            release.tag
        ))),
    }
}

/// Download a small release asset into `dir`; returns its path and SHA-256.
fn fetch(source: &dyn Source, asset: &ReleaseAsset, dir: &Path) -> Result<(PathBuf, String)> {
    if asset.size > MAX_SIDECAR {
        return Err(Error::msg(format!(
            "{} is {} bytes, far more than a signature or checksum list",
            asset.name, asset.size
        )));
    }
    let path = dir.join(crate::config::sanitize_component(&asset.name));
    let sha256 = source.download(asset, &path, &ui::SilentProgress)?;
    Ok((path, sha256))
}

fn read_capped(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut bytes = Vec::new();
    file.take(MAX_SIDECAR + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io(path, e))?;
    if bytes.len() as u64 > MAX_SIDECAR {
        return Err(Error::msg(format!(
            "{} is larger than a signature or checksum list ever is",
            path.display()
        )));
    }
    Ok(bytes)
}

fn open(path: &Path) -> Result<BufReader<std::fs::File>> {
    std::fs::File::open(path)
        .map(BufReader::new)
        .map_err(|e| Error::io(path, e))
}

// ---------------------------------------------------------------------------
// sigstore
// ---------------------------------------------------------------------------

fn sigstore_policy(policy: &TrustPolicy) -> Result<()> {
    let issuer = policy.issuer.as_deref().unwrap_or_default();
    if issuer.len() <= "https://".len() || !issuer.starts_with("https://") {
        return Err(Error::msg(
            "the sigstore verifier needs `trust.issuer`, the https URL of the OIDC issuer — \
             https://token.actions.githubusercontent.com for GitHub Actions",
        ));
    }
    if policy
        .identity
        .as_deref()
        .is_some_and(|id| id.is_empty() || id.trim() != id)
    {
        return Err(Error::msg(
            "`trust.identity` must be the certificate identity exactly, with no surrounding space",
        ));
    }
    if let Some(repo) = &policy.repository {
        let normal = crate::config::validate_repo("trust.repository", repo.clone())?;
        if &normal != repo {
            return Err(Error::msg(format!(
                "write `trust.repository` as `{normal}`"
            )));
        }
    }
    if policy.identity.is_none() && policy.repository.is_none() {
        return Err(Error::msg(
            "the sigstore verifier needs `trust.identity`, `trust.repository`, or both: \
             an issuer alone admits anyone it issues certificates to",
        ));
    }
    Ok(())
}

/// Verify a Sigstore bundle over the file at `signed`. Returns who signed,
/// as pinned, and the Rekor log index.
fn sigstore_check(
    policy: &TrustPolicy,
    sidecar: &[u8],
    signed: &Path,
) -> Result<(String, Option<u64>)> {
    use sigstore::bundle::verify::blocking::Verifier as BundleVerifier;
    use sigstore::bundle::verify::policy::{
        AllOf, GitHubWorkflowRepository, Identity, OIDCIssuer, SingleX509ExtPolicy,
        VerificationPolicy,
    };

    let bundle: sigstore::bundle::Bundle = serde_json::from_slice(sidecar)
        .map_err(|_| Error::msg("the signature file is not a Sigstore bundle"))?;
    let root = TrustedRoot::embedded()?;
    let log_index = check_signed_entry_timestamp(&bundle, &root)?;

    let issuer = policy.issuer.as_deref().unwrap_or_default();
    let issued_by = OIDCIssuer::new(issuer);
    let identity = policy.identity.as_ref().map(|id| Identity::new(id, issuer));
    let repository = policy
        .repository
        .as_ref()
        .map(GitHubWorkflowRepository::new);
    let mut rules: Vec<&dyn VerificationPolicy> = vec![&issued_by];
    if let Some(rule) = &identity {
        rules.push(rule);
    }
    if let Some(rule) = &repository {
        rules.push(rule);
    }
    let rules =
        AllOf::new(rules).ok_or_else(|| Error::msg("internal error: an empty sigstore policy"))?;

    let verifier = BundleVerifier::new(Default::default(), root.manual())
        .map_err(|_| Error::msg("the embedded Sigstore trusted root is unusable"))?;
    verifier
        .verify(open(signed)?, bundle, &rules, true)
        .map_err(sigstore_reason)?;

    let who = match (&policy.identity, &policy.repository) {
        (Some(identity), _) => identity.clone(),
        (None, Some(repo)) => format!("a workflow in {repo}"),
        (None, None) => String::from("?"),
    };
    Ok((format!("{who} via {issuer}"), log_index))
}

/// Say why a bundle failed without repeating anything it contains: policy
/// errors quote the certificate's own values, bundle errors its media type.
fn sigstore_reason(error: sigstore::bundle::verify::VerificationError) -> Error {
    use sigstore::bundle::verify::VerificationError as E;
    Error::msg(match error {
        E::Policy(_) => {
            "the signing certificate was issued to someone other than the manifest names"
        }
        E::Certificate(_) => {
            "the signing certificate does not chain to Sigstore's certificate authority, \
             or was not valid when it signed"
        }
        E::Signature(_) => "the signature does not cover this file",
        E::Bundle(_) => "the Sigstore bundle is malformed, or of a kind ketch cannot check offline",
        E::Input(_) => "the file to check could not be read",
    })
}

/// Verify the Rekor signed entry timestamp: the log's signature over the
/// entry, time and index. Without it `integratedTime` is just a number in a
/// file the signer wrote, and the certificate's ten-minute validity window
/// is checked against nothing.
// ponytail: the SET only, not the Merkle inclusion proof. The SET is Rekor's
// promise to include the entry; checking the proof needs a checkpoint and is
// the upgrade if a bundle ever arrives with a proof and no promise.
fn check_signed_entry_timestamp(
    bundle: &sigstore::bundle::Bundle,
    root: &TrustedRoot,
) -> Result<Option<u64>> {
    let entry = bundle
        .verification_material
        .as_ref()
        .and_then(|m| m.tlog_entries.first())
        .ok_or_else(|| Error::msg("the Sigstore bundle carries no transparency-log entry"))?;
    let promise = entry.inclusion_promise.as_ref().ok_or_else(|| {
        Error::msg(
            "the Sigstore bundle has no signed entry timestamp, which ketch needs \
             to check the transparency log offline",
        )
    })?;
    let log_id = entry
        .log_id
        .as_ref()
        .map(|id| id.key_id.as_slice())
        .unwrap_or_default();
    let key = root
        .rekor
        .iter()
        .find(|(id, _)| id.as_slice() == log_id)
        .map(|(_, key)| key)
        .ok_or_else(|| {
            Error::msg("the transparency log that recorded this signature is not one ketch trusts")
        })?;
    // The body exactly as Rekor signed it: base64, which is how the bundle's
    // own serde form writes bytes.
    let json = serde_json::to_value(entry)
        .map_err(|_| Error::msg("the Sigstore bundle's log entry is malformed"))?;
    let body = json["canonicalizedBody"]
        .as_str()
        .ok_or_else(|| Error::msg("the Sigstore bundle's log entry has no body"))?;
    // Rekor's canonical JSON: keys sorted, no whitespace.
    let payload = format!(
        r#"{{"body":"{body}","integratedTime":{},"logID":"{}","logIndex":{}}}"#,
        entry.integrated_time,
        hex::encode(log_id),
        entry.log_index
    );
    let key = sigstore::crypto::CosignVerificationKey::try_from_der(key)
        .map_err(|_| Error::msg("the embedded Rekor key is unusable"))?;
    key.verify_signature(
        sigstore::crypto::Signature::Raw(&promise.signed_entry_timestamp),
        payload.as_bytes(),
    )
    .map_err(|_| {
        Error::msg("the transparency log's signed entry timestamp does not match the entry")
    })?;
    Ok(u64::try_from(entry.log_index).ok())
}

/// The key material from the embedded trusted root.
struct TrustedRoot {
    /// Every Fulcio certificate, roots and intermediates alike.
    fulcio: Vec<Vec<u8>>,
    /// Rekor logs: (log id, SPKI DER).
    rekor: Vec<(Vec<u8>, Vec<u8>)>,
    /// Certificate-transparency log keys, SPKI DER.
    ctfe: Vec<Vec<u8>>,
}

impl TrustedRoot {
    fn embedded() -> Result<TrustedRoot> {
        TrustedRoot::parse(SIGSTORE_TRUSTED_ROOT).map_err(|what| {
            Error::msg(format!(
                "the embedded Sigstore trusted root is unusable: {what}"
            ))
        })
    }

    fn parse(json: &str) -> std::result::Result<TrustedRoot, &'static str> {
        let root: serde_json::Value = serde_json::from_str(json).map_err(|_| "not JSON")?;
        let logs = |section: &str| {
            root[section]
                .as_array()
                .ok_or("a log section is missing")?
                .iter()
                .map(|log| {
                    Ok((
                        base64(&log["logId"]["keyId"])?,
                        base64(&log["publicKey"]["rawBytes"])?,
                    ))
                })
                .collect::<std::result::Result<Vec<_>, &'static str>>()
        };
        let mut fulcio = Vec::new();
        for ca in root["certificateAuthorities"]
            .as_array()
            .ok_or("no certificate authorities")?
        {
            for cert in ca["certChain"]["certificates"]
                .as_array()
                .ok_or("a certificate chain is missing")?
            {
                fulcio.push(base64(&cert["rawBytes"])?);
            }
        }
        Ok(TrustedRoot {
            fulcio,
            rekor: logs("tlogs")?,
            ctfe: logs("ctlogs")?.into_iter().map(|(_, key)| key).collect(),
        })
    }

    fn manual(&self) -> sigstore::trust::ManualTrustRoot<'static> {
        let numbered = |keys: Vec<Vec<u8>>| {
            keys.into_iter()
                .enumerate()
                .map(|(i, key)| (i.to_string(), key))
                .collect()
        };
        sigstore::trust::ManualTrustRoot {
            fulcio_certs: self.fulcio.iter().map(|der| der.clone().into()).collect(),
            rekor_keys: numbered(self.rekor.iter().map(|(_, key)| key.clone()).collect()),
            ctfe_keys: numbered(self.ctfe.clone()),
        }
    }
}

fn base64(value: &serde_json::Value) -> std::result::Result<Vec<u8>, &'static str> {
    let text = value.as_str().ok_or("key material is missing")?;
    let mut out = Vec::new();
    pgp::base64::Base64Decoder::new(text.as_bytes())
        .read_to_end(&mut out)
        .map_err(|_| "key material is not base64")?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// minisign
// ---------------------------------------------------------------------------

/// The pinned key: the bare key line, or the whole `minisign.pub`.
fn minisign_key(policy: &TrustPolicy) -> Result<minisign_verify::PublicKey> {
    let text = policy
        .public_key
        .as_deref()
        .ok_or_else(|| Error::msg("the minisign verifier needs `trust.public_key`"))?;
    minisign_verify::PublicKey::from_base64(minisign_key_line(text))
        .map_err(|_| Error::msg("`trust.public_key` is not a minisign public key"))
}

/// The key line of a `minisign.pub`; its comment line says nothing ketch uses.
fn minisign_key_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default()
}

fn minisign_check(policy: &TrustPolicy, sidecar: &[u8], signed: &Path) -> Result<String> {
    let key = minisign_key(policy)?;
    let signature = std::str::from_utf8(sidecar)
        .ok()
        .and_then(|text| minisign_verify::Signature::decode(text).ok())
        .ok_or_else(|| Error::msg("the signature file is not a minisign signature"))?;
    // Legacy (non-prehashed) signatures are refused here too: they sign the
    // raw bytes, which cannot be streamed, and minisign stopped making them.
    let mut stream = key.verify_stream(&signature).map_err(|_| {
        Error::msg("the signature was not made by the pinned key, or uses the legacy format")
    })?;
    let mut file = open(signed)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| Error::io(signed, e))?;
        if n == 0 {
            break;
        }
        stream.update(&buf[..n]);
    }
    stream
        .finalize()
        .map_err(|_| Error::msg("the signature does not cover this file"))?;
    Ok(format!(
        "minisign key {}",
        minisign_key_line(policy.public_key.as_deref().unwrap_or_default())
    ))
}

// ---------------------------------------------------------------------------
// OpenPGP
// ---------------------------------------------------------------------------

/// Parse the inline key and hold it to the pinned fingerprint.
fn gpg_key(policy: &TrustPolicy) -> Result<(SignedPublicKey, String)> {
    let armored = policy.public_key.as_deref().ok_or_else(|| {
        Error::msg("the gpg verifier needs `trust.public_key`, an armored key block")
    })?;
    let pinned: String = policy
        .fingerprint
        .as_deref()
        .ok_or_else(|| {
            Error::msg(
                "the gpg verifier needs `trust.fingerprint`: the key block alone could be anyone's",
            )
        })?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if !matches!(pinned.len(), 40 | 64) || !pinned.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::msg(
            "`trust.fingerprint` must be the full 40- or 64-digit fingerprint, not a key id",
        ));
    }
    let (key, _) = SignedPublicKey::from_string(armored)
        .map_err(|_| Error::msg("`trust.public_key` is not an armored OpenPGP public key"))?;
    key.verify_bindings()
        .map_err(|_| Error::msg("`trust.public_key` has self-signatures that do not verify"))?;
    let actual = format!("{:X}", key.primary_key.fingerprint());
    if actual != pinned {
        return Err(Error::msg(format!(
            "`trust.public_key` is key {actual}, not the pinned {pinned}"
        )));
    }
    if !key.details.revocation_signatures.is_empty() {
        return Err(Error::msg(format!("the pinned key {actual} is revoked")));
    }
    Ok((key, actual))
}

fn gpg_check(policy: &TrustPolicy, sidecar: &[u8], signed: &Path, now: u64) -> Result<String> {
    let (key, fingerprint) = gpg_key(policy)?;
    let parsed = if sidecar.starts_with(b"-----BEGIN") {
        std::str::from_utf8(sidecar)
            .ok()
            .and_then(|text| DetachedSignature::from_string(text).ok())
            .map(|(sig, _)| sig)
    } else {
        DetachedSignature::from_bytes(sidecar).ok()
    };
    let signature = parsed
        .ok_or_else(|| Error::msg("the signature file is not an OpenPGP signature"))?
        .signature;

    // The primary key, then each subkey bound to it for signing and not
    // revoked. Whichever verifies decides the expiry that applies.
    let primary = &key.primary_key;
    let primary_expiry = expiry(
        primary.created_at().as_secs(),
        key.details
            .users
            .iter()
            .flat_map(|user| &user.signatures)
            .chain(&key.details.direct_signatures)
            .filter(|sig| issued_by(sig, primary)),
    );
    let mut verified = None;
    if signature.verify(primary, open(signed)?).is_ok() {
        verified = Some(primary_expiry);
    } else {
        for sub in &key.public_subkeys {
            let revoked = sub
                .signatures
                .iter()
                .any(|sig| sig.typ() == Some(SignatureType::SubkeyRevocation));
            let binding = sub
                .signatures
                .iter()
                .filter(|sig| sig.typ() == Some(SignatureType::SubkeyBinding))
                .max_by_key(|sig| sig.created().map(|t| t.as_secs()));
            let Some(binding) = binding else { continue };
            if revoked || !binding.key_flags().sign() {
                continue;
            }
            if signature.verify(&sub.key, open(signed)?).is_ok() {
                let own = expiry(sub.key.created_at().as_secs(), std::iter::once(binding));
                verified = Some(match (own, primary_expiry) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                });
                break;
            }
        }
    }
    let key_expiry = verified.ok_or_else(|| {
        Error::msg("the signature does not cover this file, or was not made by the pinned key")
    })?;

    let created = u64::from(
        signature
            .created()
            .ok_or_else(|| Error::msg("the signature carries no creation time"))?
            .as_secs(),
    );
    if let Some(lifetime) = signature
        .signature_expiration_time()
        .map(|d| u64::from(d.as_secs()))
        .filter(|secs| *secs > 0)
    {
        if created.saturating_add(lifetime) <= now {
            return Err(Error::msg("the signature has expired"));
        }
    }
    if key_expiry.is_some_and(|expires| created >= expires) {
        return Err(Error::msg(
            "the signature was made after the pinned key expired",
        ));
    }
    Ok(format!("OpenPGP key {fingerprint}"))
}

/// When a key made at `created` stops being valid, from the most recent of
/// its self-signatures. `None`: it does not expire.
fn expiry<'a>(created: u32, signatures: impl Iterator<Item = &'a Signature>) -> Option<u64> {
    let latest = signatures.max_by_key(|sig| sig.created().map(|t| t.as_secs()))?;
    latest
        .key_expiration_time()
        .map(|d| u64::from(d.as_secs()))
        .filter(|secs| *secs > 0)
        .map(|secs| u64::from(created) + secs)
}

/// Whether `sig` was issued by `key` itself, rather than a third party
/// certifying one of its user IDs.
fn issued_by(sig: &Signature, key: &impl KeyDetails) -> bool {
    let fingerprint = key.fingerprint();
    sig.issuer_fingerprint().iter().any(|f| **f == fingerprint)
        || (sig.issuer_fingerprint().is_empty()
            && sig
                .issuer_key_id()
                .iter()
                .any(|id| **id == key.legacy_key_id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/trust")
            .join(name)
    }

    fn bytes(name: &str) -> Vec<u8> {
        std::fs::read(fixture(name)).unwrap()
    }

    fn text(name: &str) -> String {
        std::fs::read_to_string(fixture(name)).unwrap()
    }

    fn policy(verifier: Verifier) -> TrustPolicy {
        TrustPolicy {
            verifier,
            mode: TrustMode::Require,
            signature: None,
            signed: None,
            issuer: None,
            identity: None,
            repository: None,
            public_key: None,
            fingerprint: None,
        }
    }

    fn sigstore(issuer: &str, identity: Option<&str>, repository: Option<&str>) -> TrustPolicy {
        TrustPolicy {
            issuer: Some(issuer.into()),
            identity: identity.map(Into::into),
            repository: repository.map(Into::into),
            ..policy(Verifier::Sigstore)
        }
    }

    fn minisign(key: &str) -> TrustPolicy {
        TrustPolicy {
            public_key: Some(text(key)),
            ..policy(Verifier::Minisign)
        }
    }

    fn gpg(key: &str, fingerprint: &str) -> TrustPolicy {
        TrustPolicy {
            public_key: Some(text(key)),
            fingerprint: Some(text(fingerprint).trim().to_string()),
            ..policy(Verifier::Gpg)
        }
    }

    fn err<T: std::fmt::Debug>(result: Result<T>) -> String {
        result.unwrap_err().to_string()
    }

    const GITHUB_OAUTH: &str = "https://github.com/login/oauth";
    const ACTIONS: &str = "https://token.actions.githubusercontent.com";

    #[test]
    #[cfg(unix)]
    fn a_real_sigstore_bundle_verifies_offline_and_names_its_log_entry() {
        // a.txt and its bundle are the public test vector sigstore's own
        // protobuf-specs crate ships: signed against production Sigstore.
        let pinned = sigstore(GITHUB_OAUTH, Some("a@tny.town"), None);
        check_policy(&pinned).unwrap();
        let (who, index) =
            sigstore_check(&pinned, &bytes("a.txt.sigstore.json"), &fixture("a.txt")).unwrap();
        assert_eq!(who, "a@tny.town via https://github.com/login/oauth");
        assert_eq!(index, Some(66_794_718));
    }

    #[test]
    fn a_sigstore_bundle_is_held_to_the_bytes_it_signed() {
        let pinned = sigstore(GITHUB_OAUTH, Some("a@tny.town"), None);
        let refused = err(sigstore_check(
            &pinned,
            &bytes("a.txt.sigstore.json"),
            &fixture("signedtool.tar.gz"),
        ));
        assert!(refused.contains("does not cover"), "{refused}");
    }

    #[test]
    fn a_sigstore_certificate_issued_to_someone_else_is_refused() {
        for pinned in [
            sigstore(GITHUB_OAUTH, Some("b@tny.town"), None),
            sigstore("https://accounts.google.com", Some("a@tny.town"), None),
        ] {
            let refused = err(sigstore_check(
                &pinned,
                &bytes("a.txt.sigstore.json"),
                &fixture("a.txt"),
            ));
            assert!(refused.contains("someone other"), "{refused}");
        }
    }

    #[test]
    fn a_github_workflow_is_matched_by_its_repository_and_no_other() {
        // A real GitHub Actions bundle whose subject (a container image) is
        // not available offline: the right repository gets past the identity
        // check and stops at the bytes, a near-miss stops at the identity.
        let bundle = bytes("kubewarden.sigstore.json");
        let file = fixture("a.txt");
        let right = sigstore(ACTIONS, None, Some("kubewarden/kubewarden-controller"));
        let refused = err(sigstore_check(&right, &bundle, &file));
        assert!(refused.contains("does not cover"), "{refused}");

        let near = sigstore(ACTIONS, None, Some("kubewarden/kubewarden-controllers"));
        let refused = err(sigstore_check(&near, &bundle, &file));
        assert!(refused.contains("someone other"), "{refused}");
    }

    #[test]
    fn a_transparency_log_entry_with_a_forged_time_or_no_promise_is_refused() {
        let pinned = sigstore(GITHUB_OAUTH, Some("a@tny.town"), None);
        let original: serde_json::Value =
            serde_json::from_slice(&bytes("a.txt.sigstore.json")).unwrap();

        let mut forged = original.clone();
        forged["verificationMaterial"]["tlogEntries"][0]["integratedTime"] = "1706297731".into();
        let refused = err(sigstore_check(
            &pinned,
            &serde_json::to_vec(&forged).unwrap(),
            &fixture("a.txt"),
        ));
        assert!(
            refused.contains("signed entry timestamp does not match"),
            "{refused}"
        );

        let mut bare = original;
        bare["verificationMaterial"]["tlogEntries"][0]
            .as_object_mut()
            .unwrap()
            .remove("inclusionPromise");
        let refused = err(sigstore_check(
            &pinned,
            &serde_json::to_vec(&bare).unwrap(),
            &fixture("a.txt"),
        ));
        assert!(refused.contains("no signed entry timestamp"), "{refused}");
    }

    #[test]
    fn a_minisign_signature_by_the_pinned_key_verifies_and_its_comments_stay_unread() {
        let pinned = minisign("minisign.pub");
        check_policy(&pinned).unwrap();
        let who = minisign_check(
            &pinned,
            &bytes("signedtool.tar.gz.minisig"),
            &fixture("signedtool.tar.gz"),
        )
        .unwrap();
        let key_line = minisign_key_line(&text("minisign.pub")).to_string();
        assert_eq!(who, format!("minisign key {key_line}"));
        assert!(!who.contains("MARKER"));

        // The bare key line pins the same key.
        let bare = TrustPolicy {
            public_key: Some(key_line),
            ..policy(Verifier::Minisign)
        };
        assert!(minisign_check(
            &bare,
            &bytes("signedtool.tar.gz.minisig"),
            &fixture("signedtool.tar.gz")
        )
        .is_ok());
    }

    #[test]
    fn a_minisign_signature_by_another_key_or_over_other_bytes_is_refused() {
        let pinned = minisign("minisign.pub");
        let file = fixture("signedtool.tar.gz");
        let refused = err(minisign_check(
            &pinned,
            &bytes("signedtool.tar.gz.other.minisig"),
            &file,
        ));
        assert!(refused.contains("not made by the pinned key"), "{refused}");
        let refused = err(minisign_check(&pinned, &bytes("mismatch.minisig"), &file));
        assert!(refused.contains("does not cover"), "{refused}");
        let refused = err(minisign_check(&pinned, b"garbage", &file));
        assert!(refused.contains("not a minisign signature"), "{refused}");
    }

    #[test]
    fn an_openpgp_signature_by_a_signing_subkey_of_the_pinned_key_verifies() {
        let pinned = gpg("publisher.asc", "publisher.fpr");
        check_policy(&pinned).unwrap();
        let file = fixture("signedtool.tar.gz");
        let fingerprint = text("publisher.fpr").trim().to_string();
        for sidecar in ["signedtool.tar.gz.asc", "signedtool.tar.gz.sig"] {
            let who = gpg_check(&pinned, &bytes(sidecar), &file, crate::model::now_unix())
                .unwrap_or_else(|e| panic!("{sidecar}: {e}"));
            assert_eq!(who, format!("OpenPGP key {fingerprint}"));
        }
    }

    #[test]
    fn an_impostor_with_the_same_user_id_or_other_bytes_is_refused() {
        let pinned = gpg("publisher.asc", "publisher.fpr");
        let file = fixture("signedtool.tar.gz");
        let now = crate::model::now_unix();
        for sidecar in ["signedtool.tar.gz.impostor.asc", "mismatch.asc"] {
            let refused = err(gpg_check(&pinned, &bytes(sidecar), &file, now));
            assert!(
                refused.contains("not made by the pinned key"),
                "{sidecar}: {refused}"
            );
        }
    }

    #[test]
    fn expired_openpgp_signatures_and_keys_are_refused() {
        let file = fixture("signedtool.tar.gz");
        let now = crate::model::now_unix();
        let refused = err(gpg_check(
            &gpg("publisher.asc", "publisher.fpr"),
            &bytes("signedtool.tar.gz.expired.asc"),
            &file,
            now,
        ));
        assert!(refused.contains("has expired"), "{refused}");

        let refused = err(gpg_check(
            &gpg("expired-publisher.asc", "expired-publisher.fpr"),
            &bytes("signedtool.tar.gz.expired-key.asc"),
            &file,
            now,
        ));
        assert!(
            refused.contains("after the pinned key expired"),
            "{refused}"
        );
    }

    #[test]
    fn a_gpg_policy_must_pin_the_key_it_carries_by_full_fingerprint() {
        let wrong = gpg("publisher.asc", "impostor.fpr");
        assert!(err(check_policy(&wrong)).contains("not the pinned"));

        let short = TrustPolicy {
            fingerprint: Some(text("publisher.fpr").trim()[24..].to_string()),
            ..gpg("publisher.asc", "publisher.fpr")
        };
        assert!(err(check_policy(&short)).contains("not a key id"));

        let unpinned = TrustPolicy {
            fingerprint: None,
            ..gpg("publisher.asc", "publisher.fpr")
        };
        assert!(err(check_policy(&unpinned)).contains("trust.fingerprint"));
    }

    #[test]
    fn a_policy_keeps_to_the_keys_its_verifier_reads() {
        let stray = TrustPolicy {
            fingerprint: Some("0".repeat(40)),
            ..minisign("minisign.pub")
        };
        assert!(err(check_policy(&stray)).contains("does not apply"));

        let issuer_only = sigstore(ACTIONS, None, None);
        assert!(err(check_policy(&issuer_only)).contains("admits anyone"));

        let no_issuer = TrustPolicy {
            identity: Some("a@tny.town".into()),
            ..policy(Verifier::Sigstore)
        };
        assert!(err(check_policy(&no_issuer)).contains("trust.issuer"));

        let prefixed = sigstore(ACTIONS, None, Some("github:o/r"));
        assert!(err(check_policy(&prefixed)).contains("`o/r`"));

        let garbage = TrustPolicy {
            public_key: Some("not a key".into()),
            ..policy(Verifier::Minisign)
        };
        assert!(err(check_policy(&garbage)).contains("not a minisign public key"));
    }

    #[test]
    fn the_embedded_trusted_root_has_every_kind_of_key() {
        let root = TrustedRoot::embedded().unwrap();
        assert!(root.fulcio.len() >= 2);
        assert!(!root.rekor.is_empty());
        assert!(!root.ctfe.is_empty());
    }
}
