//! Double-submit CSRF tokens.
//!
//! The server mints a token bound to the session id via HMAC-SHA256 keyed
//! on the configured `session_key`. The client echoes it in an
//! `X-CSRF-Token` header on unsafe-method requests; the server verifies
//! the HMAC matches the cookie. Tokens are NOT sent back to storage — the
//! HMAC IS the validation.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub fn mint(key: &[u8], session_id: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(session_id);
    let tag = mac.finalize().into_bytes();
    URL_SAFE_NO_PAD.encode(tag)
}

pub fn verify(key: &[u8], session_id: &[u8], token: &str) -> bool {
    let Ok(provided) = URL_SAFE_NO_PAD.decode(token) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(session_id);
    mac.verify_slice(&provided).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"test-key-32-bytes-aaaaaaaaaaaaaa";

    #[test]
    fn roundtrip() {
        let sid = b"sid";
        let t = mint(KEY, sid);
        assert!(verify(KEY, sid, &t));
    }

    #[test]
    fn rejects_different_session() {
        let t = mint(KEY, b"sid-1");
        assert!(!verify(KEY, b"sid-2", &t));
    }

    #[test]
    fn rejects_corrupt_token() {
        assert!(!verify(KEY, b"sid", "not-base64!"));
        assert!(!verify(KEY, b"sid", "AAA"));
    }

    #[test]
    fn rejects_different_key() {
        let t = mint(KEY, b"sid");
        let other_key: &[u8] = b"other-key-32-bytes-bbbbbbbbbbbbb";
        assert!(!verify(other_key, b"sid", &t));
    }
}
