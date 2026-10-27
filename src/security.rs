//! At-rest seal for privileged ledger bytes.
//!
//! The keystream is SHA-256 in counter mode. HMAC-SHA256 covers the nonce and
//! the ciphertext. Callers supply the key. Opening a blob checks the mac
//! before it returns plaintext.

use sha2::{Digest, Sha256};

use crate::core::field::hex_encode;
use crate::error::SealError;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SealedBlob {
    pub nonce_hex: String,
    pub ciphertext_hex: String,
    pub mac_hex: String,
}

pub fn seal(key: &[u8], plaintext: &[u8], nonce: &[u8; 16]) -> SealedBlob {
    let ciphertext = apply_keystream(key, nonce, plaintext);
    let mac = hmac_sha256(key, &mac_message(nonce, &ciphertext));
    SealedBlob {
        nonce_hex: hex_encode(nonce),
        ciphertext_hex: hex_encode(&ciphertext),
        mac_hex: hex_encode(&mac),
    }
}

pub fn open(key: &[u8], blob: &SealedBlob) -> Result<Vec<u8>, SealError> {
    let nonce = decode_hex_16(&blob.nonce_hex)?;
    let ciphertext = decode_hex(&blob.ciphertext_hex)?;
    let mac = decode_hex_32(&blob.mac_hex)?;
    let expected = hmac_sha256(key, &mac_message(&nonce, &ciphertext));
    if !mac_eq(&mac, &expected) {
        return Err(SealError::Authentication);
    }
    Ok(apply_keystream(key, &nonce, &ciphertext))
}

fn mac_message(nonce: &[u8; 16], ciphertext: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(16 + ciphertext.len());
    message.extend_from_slice(nonce);
    message.extend_from_slice(ciphertext);
    message
}

fn apply_keystream(key: &[u8], nonce: &[u8; 16], bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut counter = 0u32;
    let mut offset = 0;
    while offset < bytes.len() {
        let block = keystream_block(key, nonce, counter);
        let take = (bytes.len() - offset).min(block.len());
        for (index, input) in bytes[offset..offset + take].iter().enumerate() {
            out.push(input ^ block[index]);
        }
        offset += take;
        counter = counter.saturating_add(1);
    }
    out
}

fn keystream_block(key: &[u8], nonce: &[u8; 16], counter: u32) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(nonce);
    hasher.update(counter.to_le_bytes());
    let digest = hasher.finalize();
    let mut block = [0u8; 32];
    block.copy_from_slice(&digest);
    block
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        key_block[..32].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        ipad[index] ^= key_block[index];
        opad[index] ^= key_block[index];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    let digest = outer.finalize();
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&digest);
    mac
}

fn mac_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (one, other) in left.iter().zip(right.iter()) {
        diff |= one ^ other;
    }
    diff == 0
}

fn decode_hex(text: &str) -> Result<Vec<u8>, SealError> {
    if text.len() % 2 != 0 {
        return Err(SealError::Encoding);
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_value(bytes[index])?;
        let low = hex_value(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Ok(out)
}

fn decode_hex_16(text: &str) -> Result<[u8; 16], SealError> {
    let bytes = decode_hex(text)?;
    let mut out = [0u8; 16];
    if bytes.len() != 16 {
        return Err(SealError::Encoding);
    }
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn decode_hex_32(text: &str) -> Result<[u8; 32], SealError> {
    let bytes = decode_hex(text)?;
    let mut out = [0u8; 32];
    if bytes.len() != 32 {
        return Err(SealError::Encoding);
    }
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn hex_value(byte: u8) -> Result<u8, SealError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(SealError::Encoding),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_round_trips_and_rejects_tampering() {
        let key = b"ledger-key";
        let nonce = [7u8; 16];
        let blob = seal(key, b"field position", &nonce);
        assert_eq!(open(key, &blob).unwrap(), b"field position");
        assert!(open(b"other-key", &blob).is_err());
        let mut tampered = blob.clone();
        tampered.ciphertext_hex.pop();
        tampered.ciphertext_hex.push('0');
        assert!(open(key, &tampered).is_err());
    }
}
