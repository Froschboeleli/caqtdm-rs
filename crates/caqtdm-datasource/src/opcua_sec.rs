//! Symmetric OPC UA chunk for SecurityPolicy#Basic256Sha256.
//!
//! The chunk is a policy URI, a token id, a sequence number, a request id,
//! an AES-256-CBC body (PKCS7, IV prepended), and an HMAC-SHA256 of
//! everything before the tag. Certificate OpenSecureChannel and the
//! asymmetric handshake are not implemented. SecurityPolicy#None stays in
//! `opcua_bin`.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::DsError;

pub const POLICY_NONE: &str = "http://opcfoundation.org/UA/SecurityPolicy#None";
pub const POLICY_BASIC256SHA256: &str = "http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256";

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type HmacSha256 = Hmac<Sha256>;

pub fn seal(
    token_id: u32,
    sequence: u32,
    request_id: u32,
    body: &[u8],
    key: &[u8; 32],
    sig_key: &[u8; 32],
    iv: &[u8; 16],
) -> Result<Vec<u8>, DsError> {
    let mut buf = vec![0u8; body.len() + 16];
    buf[..body.len()].copy_from_slice(body);
    let cipher = Aes256CbcEnc::new(key.into(), iv.into())
        .encrypt_padded_mut::<Pkcs7>(&mut buf, body.len())
        .map_err(|_| DsError::new("secure chunk pad failed"))?
        .to_vec();
    let mut out = header(POLICY_BASIC256SHA256, token_id, sequence, request_id);
    out.extend_from_slice(iv);
    out.extend_from_slice(&cipher);
    let tag = mac(sig_key, &out)?;
    out.extend_from_slice(&tag);
    Ok(out)
}

pub fn open(
    bytes: &[u8],
    key: &[u8; 32],
    sig_key: &[u8; 32],
) -> Result<(u32, u32, u32, Vec<u8>), DsError> {
    if bytes.len() < 32 + 16 {
        return Err(DsError::new("secure chunk is short"));
    }
    let (signed, tag) = bytes.split_at(bytes.len() - 32);
    let expect = mac(sig_key, signed)?;
    if expect.as_slice() != tag {
        return Err(DsError::new("secure chunk signature mismatch"));
    }
    let (policy, rest) = take_string(signed)?;
    if policy != POLICY_BASIC256SHA256 {
        return Err(DsError::new(format!("unexpected security policy {policy}")));
    }
    if rest.len() < 12 + 16 {
        return Err(DsError::new("secure chunk header is short"));
    }
    let token_id = u32::from_le_bytes(rest[0..4].try_into().unwrap_or([0; 4]));
    let sequence = u32::from_le_bytes(rest[4..8].try_into().unwrap_or([0; 4]));
    let request_id = u32::from_le_bytes(rest[8..12].try_into().unwrap_or([0; 4]));
    let iv: [u8; 16] = rest[12..28]
        .try_into()
        .map_err(|_| DsError::new("secure chunk iv is short"))?;
    let cipher = &rest[28..];
    let mut buf = cipher.to_vec();
    let plain = Aes256CbcDec::new(key.into(), (&iv).into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| DsError::new("secure chunk decrypt failed"))?
        .to_vec();
    Ok((token_id, sequence, request_id, plain))
}

fn header(policy: &str, token_id: u32, sequence: u32, request_id: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(policy.len() as u32).to_le_bytes());
    out.extend_from_slice(policy.as_bytes());
    out.extend_from_slice(&token_id.to_le_bytes());
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out
}

fn take_string(bytes: &[u8]) -> Result<(String, &[u8]), DsError> {
    if bytes.len() < 4 {
        return Err(DsError::new("policy length missing"));
    }
    let len = u32::from_le_bytes(bytes[0..4].try_into().unwrap_or([0; 4])) as usize;
    if bytes.len() < 4 + len {
        return Err(DsError::new("policy uri truncated"));
    }
    let text = std::str::from_utf8(&bytes[4..4 + len])
        .map_err(|_| DsError::new("policy uri is not utf-8"))?
        .to_string();
    Ok((text, &bytes[4 + len..]))
}

fn mac(key: &[u8], data: &[u8]) -> Result<[u8; 32], DsError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| DsError::new("bad hmac key"))?;
    mac.update(data);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}
