use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

pub fn encode_webhook_secret(secret: &[u8; 32]) -> String {
    format!("whsec_{}", URL_SAFE_NO_PAD.encode(secret))
}

pub fn decode_webhook_secret(s: &str) -> anyhow::Result<[u8; 32]> {
    let raw = s
        .strip_prefix("whsec_")
        .ok_or_else(|| anyhow::anyhow!("bad webhook secret"))?;
    let b = URL_SAFE_NO_PAD.decode(raw)?;
    anyhow::ensure!(b.len() == 32, "bad webhook secret length");
    Ok(b.try_into().unwrap())
}

pub fn sign_webhook(secret: &[u8], timestamp: i64, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC key");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("v1={}", hex::encode(mac.finalize().into_bytes()))
}

pub fn verify_webhook(
    secret: &[u8],
    timestamp: i64,
    body: &[u8],
    signature: &str,
    now: i64,
) -> anyhow::Result<()> {
    anyhow::ensure!(now.abs_diff(timestamp) <= 300, "stale webhook");
    anyhow::ensure!(
        signature.len() == 67
            && signature.starts_with("v1=")
            && signature[3..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid signature encoding"
    );
    let tag = hex::decode(
        signature
            .strip_prefix("v1=")
            .ok_or_else(|| anyhow::anyhow!("invalid signature"))?,
    )?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret).map_err(|_| anyhow::anyhow!("invalid key"))?;
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&tag)
        .map_err(|_| anyhow::anyhow!("invalid signature"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signatures_fail_closed() {
        let key = [9; 32];
        let body = b"{\"amount\":\"10\"}";
        let signature = sign_webhook(&key, 1000, body);
        assert!(verify_webhook(&key, 1000, body, &signature, 1000).is_ok());
        for (key, timestamp, body, signature, now) in [
            ([8; 32], 1000, &body[..], signature.clone(), 1000),
            ([9; 32], 1000, &b"edited"[..], signature.clone(), 1000),
            ([9; 32], 1000, &body[..], signature.clone(), 1301),
            ([9; 32], 1000, &body[..], signature.to_uppercase(), 1000),
            (
                [9; 32],
                1000,
                &body[..],
                signature.replace("v1=", "v2="),
                1000,
            ),
            ([9; 32], i64::MIN, &body[..], signature.clone(), i64::MAX),
        ] {
            assert!(verify_webhook(&key, timestamp, body, &signature, now).is_err());
        }
        assert!(decode_webhook_secret("whsec_bad").is_err());
    }
}
