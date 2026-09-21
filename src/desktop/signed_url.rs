//! Short-lived signed URLs for resources that an `<img>` tag loads.
//!
//! An `<img>` tag cannot send an `Authorization` header. The old reader put
//! the master session token in the query string, and the handler ignored it.
//! A signed URL grants one resource for a short time. It never carries the
//! master token, so a copied or logged URL cannot reach the rest of the API.

use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// How long a signed URL works.
pub const SIGNED_URL_TTL_SECONDS: u64 = 10 * 60;

const BLOCK_SIZE: usize = 64;

/// HMAC-SHA256 as defined in RFC 2104.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|byte| byte ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(block.map(|byte| byte ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Compares two strings in time that does not depend on where they differ.
fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Signs and checks URLs with a random key that lives for one process. A
/// restart invalidates every signed URL, and that is intended.
pub struct UrlSigner {
    key: [u8; 32],
}

impl Default for UrlSigner {
    fn default() -> Self {
        Self::new()
    }
}

impl UrlSigner {
    pub fn new() -> Self {
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        key[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        Self { key }
    }

    /// Signs one resource `scope` until `expires_at` (Unix seconds).
    pub fn sign(&self, scope: &str, expires_at: u64) -> String {
        to_hex(&hmac_sha256(
            &self.key,
            format!("{scope}|{expires_at}").as_bytes(),
        ))
    }

    /// Returns the expiry and signature for a new URL that lasts the default
    /// time.
    pub fn grant(&self, scope: &str) -> (u64, String) {
        let expires_at = unix_now() + SIGNED_URL_TTL_SECONDS;
        (expires_at, self.sign(scope, expires_at))
    }

    /// True when `signature` is valid for `scope` and `expires_at` has not
    /// passed at `now`.
    pub fn verify(&self, scope: &str, expires_at: u64, signature: &str, now: u64) -> bool {
        now <= expires_at && constant_time_eq(&self.sign(scope, expires_at), signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 32]) -> String {
        to_hex(&bytes)
    }

    #[test]
    fn hmac_matches_rfc_4231_test_case_1() {
        let key = [0x0b; 20];
        assert_eq!(
            hex(hmac_sha256(&key, b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn hmac_matches_rfc_4231_test_case_2() {
        assert_eq!(
            hex(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_hashes_a_key_that_is_longer_than_one_block() {
        let key = [0xaa; 131];
        assert_eq!(
            hex(hmac_sha256(
                &key,
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn a_valid_signature_verifies_until_it_expires() {
        let signer = UrlSigner::new();
        let sig = signer.sign("reader|c1|m1|0", 1_000);
        assert!(signer.verify("reader|c1|m1|0", 1_000, &sig, 999));
        assert!(signer.verify("reader|c1|m1|0", 1_000, &sig, 1_000));
        assert!(!signer.verify("reader|c1|m1|0", 1_000, &sig, 1_001));
    }

    #[test]
    fn a_signature_is_bound_to_its_scope_and_expiry() {
        let signer = UrlSigner::new();
        let sig = signer.sign("reader|c1|m1|0", 1_000);
        assert!(!signer.verify("reader|c1|m1|1", 1_000, &sig, 1));
        assert!(!signer.verify("reader|c1|m1|0", 2_000, &sig, 1));
        assert!(!signer.verify("reader|c1|m1|0", 1_000, "", 1));
        assert!(!signer.verify("reader|c1|m1|0", 1_000, &sig[..10], 1));
    }

    #[test]
    fn two_signers_do_not_accept_each_other() {
        let sig = UrlSigner::new().sign("scope", 1_000);
        assert!(!UrlSigner::new().verify("scope", 1_000, &sig, 1));
    }
}
