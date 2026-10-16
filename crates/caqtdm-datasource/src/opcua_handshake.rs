//! OPC UA OpenSecureChannel for SecurityPolicy#Basic256Sha256.
//!
//! The OPN chunk carries the sender certificate and the receiver certificate
//! thumbprint (SHA-1). The sequence header and the OpenSecureChannel body are
//! signed with RSA PKCS#1 v1.5 SHA-256 and encrypted with RSA-OAEP SHA-1, which
//! is the asymmetric pair Basic256Sha256 specifies. Both sides then run
//! P_SHA256 over the nonces to derive the AES-256 and HMAC-SHA256 keys.

use rand::rngs::OsRng;
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::EncodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use x509_parser::prelude::*;
use x509_parser::public_key::PublicKey;

use crate::error::DsError;

const POLICY: &str = crate::opcua_sec::POLICY_BASIC256SHA256;
const OPEN_REQUEST: u32 = 446;
const OPEN_RESPONSE: u32 = 449;
const RSA_MODULUS: usize = 256;
const OAEP_OVERHEAD: usize = 42;
const PLAIN_BLOCK: usize = RSA_MODULUS - OAEP_OVERHEAD;

#[derive(Clone)]
pub struct Identity {
    pub cert_der: Vec<u8>,
    pub key: RsaPrivateKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelKeys {
    pub channel_id: u32,
    pub token_id: u32,
    pub client_sign: [u8; 32],
    pub client_enc: [u8; 32],
    pub client_iv: [u8; 16],
    pub server_sign: [u8; 32],
    pub server_enc: [u8; 32],
    pub server_iv: [u8; 16],
}

pub fn identity_named(name: &str) -> Result<Identity, DsError> {
    let key = RsaPrivateKey::new(&mut OsRng, 2048)
        .map_err(|err| DsError::new(format!("certificate key: {err}")))?;
    let pkcs8 = key
        .to_pkcs8_der()
        .map_err(|err| DsError::new(format!("certificate key: {err}")))?;
    let key_pair =
        rcgen::KeyPair::from_der_and_sign_algo(pkcs8.as_bytes(), &rcgen::PKCS_RSA_SHA256)
            .map_err(|err| DsError::new(format!("certificate key: {err}")))?;
    let mut params = rcgen::CertificateParams::new(vec![name.to_string()]);
    params.alg = &rcgen::PKCS_RSA_SHA256;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, name);
    params.key_pair = Some(key_pair);
    let cert = rcgen::Certificate::from_params(params)
        .map_err(|err| DsError::new(format!("certificate: {err}")))?;
    let cert_der = cert
        .serialize_der()
        .map_err(|err| DsError::new(format!("certificate der: {err}")))?;
    Ok(Identity { cert_der, key })
}

pub fn thumbprint(der: &[u8]) -> [u8; 20] {
    let digest = Sha1::digest(der);
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest);
    out
}

pub fn public_from_cert(der: &[u8]) -> Result<RsaPublicKey, DsError> {
    let (_, cert) = X509Certificate::from_der(der)
        .map_err(|_| DsError::new("opc ua certificate did not parse"))?;
    let key = cert
        .public_key()
        .parsed()
        .map_err(|_| DsError::new("opc ua certificate public key"))?;
    let PublicKey::RSA(rsa) = key else {
        return Err(DsError::new("opc ua certificate is not RSA"));
    };
    RsaPublicKey::new(
        rsa::BigUint::from_bytes_be(rsa.modulus),
        rsa::BigUint::from_bytes_be(rsa.exponent),
    )
    .map_err(|_| DsError::new("opc ua RSA public key"))
}

pub fn client_open(
    client: &Identity,
    server_cert: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, DsError> {
    let body = open_request_body(nonce);
    let server_public = public_from_cert(server_cert)?;
    seal_opn(
        &client.key,
        &client.cert_der,
        &thumbprint(server_cert),
        0,
        &body,
        &server_public,
    )
}

pub fn server_accept(
    server: &Identity,
    message: &[u8],
    server_nonce: &[u8],
) -> Result<(Vec<u8>, ChannelKeys), DsError> {
    let (channel, client_cert, plain) = open_opn(&server.key, message)?;
    if channel != 0 {
        return Err(DsError::new("open channel id is not zero"));
    }
    let client_nonce = nonce_after_security_mode(&plain)?;
    let client_public = public_from_cert(&client_cert)?;
    let channel_id = 7u32;
    let token_id = 3u32;
    let body = open_response_body(channel_id, token_id, server_nonce);
    let reply = seal_opn(
        &server.key,
        &server.cert_der,
        &thumbprint(&client_cert),
        channel_id,
        &body,
        &client_public,
    )?;
    let keys = derive_keys(&client_nonce, server_nonce, channel_id, token_id);
    Ok((reply, keys))
}

pub fn client_finish(
    client: &Identity,
    response: &[u8],
    client_nonce: &[u8],
) -> Result<ChannelKeys, DsError> {
    let (_channel, _cert, plain) = open_opn(&client.key, response)?;
    let (channel_id, token_id, server_nonce) = parse_open_response(&plain)?;
    Ok(derive_keys(
        client_nonce,
        &server_nonce,
        channel_id,
        token_id,
    ))
}

fn derive_keys(
    client_nonce: &[u8],
    server_nonce: &[u8],
    channel_id: u32,
    token_id: u32,
) -> ChannelKeys {
    let client = p_sha256(server_nonce, client_nonce, 80);
    let server = p_sha256(client_nonce, server_nonce, 80);
    ChannelKeys {
        channel_id,
        token_id,
        client_sign: slice32(&client, 0),
        client_enc: slice32(&client, 32),
        client_iv: slice16(&client, 64),
        server_sign: slice32(&server, 0),
        server_enc: slice32(&server, 32),
        server_iv: slice16(&server, 64),
    }
}

fn slice32(bytes: &[u8], at: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[at..at + 32]);
    out
}

fn slice16(bytes: &[u8], at: usize) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&bytes[at..at + 16]);
    out
}

/// TLS 1.2 P_SHA256, the construction OPC UA Part 6 uses for key derivation.
pub fn p_sha256(secret: &[u8], seed: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut a = hmac_sha256(secret, seed);
    while out.len() < len {
        let mut data = a.clone();
        data.extend_from_slice(seed);
        out.extend_from_slice(&hmac_sha256(secret, &data));
        a = hmac_sha256(secret, &a);
    }
    out.truncate(len);
    out
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn open_request_body(nonce: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    write_node_id(&mut body, OPEN_REQUEST);
    write_request_header(&mut body);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&3u32.to_le_bytes());
    write_bytes(&mut body, nonce);
    body.extend_from_slice(&3_600_000u32.to_le_bytes());
    body
}

fn open_response_body(channel_id: u32, token_id: u32, nonce: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    write_node_id(&mut body, OPEN_RESPONSE);
    body.extend_from_slice(&0i64.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.push(0);
    body.extend_from_slice(&0i32.to_le_bytes());
    write_node_id(&mut body, 0);
    body.push(0);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&channel_id.to_le_bytes());
    body.extend_from_slice(&token_id.to_le_bytes());
    body.extend_from_slice(&0i64.to_le_bytes());
    body.extend_from_slice(&60_000u32.to_le_bytes());
    write_bytes(&mut body, nonce);
    body
}

fn parse_open_response(plain: &[u8]) -> Result<(u32, u32, Vec<u8>), DsError> {
    let nonce = last_byte_string(plain)?;
    let nonce_at = plain.len() - nonce.len() - 4;
    if nonce_at < 16 {
        return Err(DsError::new("open response token"));
    }
    let token_at = nonce_at - 4 - 8 - 4;
    let channel_at = token_at - 4;
    let channel_id = u32::from_le_bytes(
        plain[channel_at..channel_at + 4]
            .try_into()
            .unwrap_or([0; 4]),
    );
    let token_id = u32::from_le_bytes(plain[token_at..token_at + 4].try_into().unwrap_or([0; 4]));
    Ok((channel_id, token_id, nonce))
}

fn last_byte_string(bytes: &[u8]) -> Result<Vec<u8>, DsError> {
    for size in [32usize, 16, 48] {
        if bytes.len() >= 4 + size {
            let at = bytes.len() - size - 4;
            let n = i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap_or([0; 4]));
            if n == size as i32 {
                return Ok(bytes[at + 4..].to_vec());
            }
        }
    }
    Err(DsError::new("open response nonce"))
}

fn nonce_after_security_mode(bytes: &[u8]) -> Result<Vec<u8>, DsError> {
    // Client nonce is the byte string written after SecurityMode = 3.
    let marker = 3u32.to_le_bytes();
    let Some(at) = bytes.windows(4).position(|window| window == marker) else {
        return Err(DsError::new("security mode missing"));
    };
    let rest = &bytes[at + 4..];
    let (len, rest) = take_i32(rest)?;
    if len < 0 || rest.len() < len as usize {
        return Err(DsError::new("client nonce"));
    }
    Ok(rest[..len as usize].to_vec())
}

fn seal_opn(
    key: &RsaPrivateKey,
    cert: &[u8],
    receiver_thumb: &[u8; 20],
    channel_id: u32,
    body: &[u8],
    peer: &RsaPublicKey,
) -> Result<Vec<u8>, DsError> {
    let mut secret = Vec::new();
    secret.extend_from_slice(&1u32.to_le_bytes());
    secret.extend_from_slice(&1u32.to_le_bytes());
    secret.extend_from_slice(body);
    let padded = sign_and_pad(key, &secret)?;
    let cipher = rsa_oaep_encrypt(peer, &padded)?;
    let mut header = b"OPNF".to_vec();
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&channel_id.to_le_bytes());
    write_string(&mut header, POLICY);
    write_bytes(&mut header, cert);
    write_bytes(&mut header, receiver_thumb);
    header.extend_from_slice(&cipher);
    let size = header.len() as u32;
    header[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(header)
}

fn open_opn(key: &RsaPrivateKey, message: &[u8]) -> Result<(u32, Vec<u8>, Vec<u8>), DsError> {
    if message.len() < 12 || &message[..4] != b"OPNF" {
        return Err(DsError::new("not an OPN chunk"));
    }
    let size = u32::from_le_bytes(message[4..8].try_into().unwrap_or([0; 4])) as usize;
    if size != message.len() {
        return Err(DsError::new("OPN size mismatch"));
    }
    let channel = u32::from_le_bytes(message[8..12].try_into().unwrap_or([0; 4]));
    let (policy, rest) = take_opc_string(&message[12..])?;
    if policy != POLICY {
        return Err(DsError::new("OPN policy is not Basic256Sha256"));
    }
    let (cert_len, rest) = take_i32(rest)?;
    if cert_len < 0 || rest.len() < cert_len as usize {
        return Err(DsError::new("OPN sender certificate"));
    }
    let cert = &rest[..cert_len as usize];
    let sender = public_from_cert(cert)?;
    let rest = &rest[cert_len as usize..];
    let (thumb_len, rest) = take_i32(rest)?;
    if thumb_len != 20 || rest.len() < 20 {
        return Err(DsError::new("OPN receiver thumbprint"));
    }
    let cipher = &rest[20..];
    let plain = rsa_oaep_decrypt(key, cipher)?;
    let content = verify_and_unpad(&sender, &plain)?;
    if content.len() < 8 {
        return Err(DsError::new("OPN sequence header"));
    }
    Ok((channel, cert.to_vec(), content[8..].to_vec()))
}

fn sign_and_pad(key: &RsaPrivateKey, data: &[u8]) -> Result<Vec<u8>, DsError> {
    let raw = data.len() + RSA_MODULUS;
    let pad = PLAIN_BLOCK - (raw % PLAIN_BLOCK);
    let mut signed = data.to_vec();
    signed.extend(std::iter::repeat_n((pad - 1) as u8, pad));
    let signature = sign_sha256(key, &signed)?;
    signed.extend_from_slice(&signature);
    if signed.len() % PLAIN_BLOCK != 0 {
        return Err(DsError::new("OPN padding is not a whole RSA block"));
    }
    Ok(signed)
}

fn verify_and_unpad(public: &RsaPublicKey, plain: &[u8]) -> Result<Vec<u8>, DsError> {
    if plain.len() < RSA_MODULUS + 1 {
        return Err(DsError::new("OPN plaintext is short"));
    }
    let (content, sig) = plain.split_at(plain.len() - RSA_MODULUS);
    verify_sha256(public, content, sig)?;
    let mark = *content.last().unwrap_or(&0);
    let pad = mark as usize + 1;
    if pad > content.len() || content[content.len() - pad..].iter().any(|b| *b != mark) {
        return Err(DsError::new("OPN padding"));
    }
    Ok(content[..content.len() - pad].to_vec())
}

fn sign_sha256(key: &RsaPrivateKey, data: &[u8]) -> Result<Vec<u8>, DsError> {
    let signing = SigningKey::<Sha256>::new(key.clone());
    let signature = signing.sign(data);
    Ok(signature.to_bytes().to_vec())
}

fn verify_sha256(public: &RsaPublicKey, data: &[u8], sig: &[u8]) -> Result<(), DsError> {
    let verifying = VerifyingKey::<Sha256>::new(public.clone());
    let signature = Signature::try_from(sig).map_err(|_| DsError::new("OPN signature length"))?;
    verifying
        .verify(data, &signature)
        .map_err(|_| DsError::new("OPN signature mismatch"))
}

fn rsa_oaep_encrypt(public: &RsaPublicKey, data: &[u8]) -> Result<Vec<u8>, DsError> {
    let mut out = Vec::new();
    for chunk in data.chunks(PLAIN_BLOCK) {
        let block = public
            .encrypt(&mut OsRng, Oaep::new::<Sha1>(), chunk)
            .map_err(|_| DsError::new("OPN RSA-OAEP encrypt"))?;
        out.extend_from_slice(&block);
    }
    Ok(out)
}

fn rsa_oaep_decrypt(key: &RsaPrivateKey, data: &[u8]) -> Result<Vec<u8>, DsError> {
    if data.len() % RSA_MODULUS != 0 {
        return Err(DsError::new("OPN ciphertext is not a whole RSA block"));
    }
    let mut out = Vec::new();
    for chunk in data.chunks(RSA_MODULUS) {
        let block = key
            .decrypt(Oaep::new::<Sha1>(), chunk)
            .map_err(|_| DsError::new("OPN RSA-OAEP decrypt"))?;
        out.extend_from_slice(&block);
    }
    Ok(out)
}

fn write_node_id(out: &mut Vec<u8>, id: u32) {
    out.push(0x01);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
}

fn write_request_header(out: &mut Vec<u8>) {
    write_node_id(out, 0);
    out.extend_from_slice(&0i64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(-1i32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    write_node_id(out, 0);
    out.push(0);
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    write_bytes(out, text.as_bytes());
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn take_i32(bytes: &[u8]) -> Result<(i32, &[u8]), DsError> {
    if bytes.len() < 4 {
        return Err(DsError::new("opc ua integer"));
    }
    let value = i32::from_le_bytes(bytes[..4].try_into().unwrap_or([0; 4]));
    Ok((value, &bytes[4..]))
}

fn take_opc_string(bytes: &[u8]) -> Result<(String, &[u8]), DsError> {
    let (len, rest) = take_i32(bytes)?;
    if len < 0 {
        return Ok((String::new(), rest));
    }
    if rest.len() < len as usize {
        return Err(DsError::new("opc ua string"));
    }
    let text = std::str::from_utf8(&rest[..len as usize])
        .map_err(|_| DsError::new("opc ua string utf-8"))?
        .to_string();
    Ok((text, &rest[len as usize..]))
}
