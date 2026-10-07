//! The STUN message format (RFC 8489) as far as a TURN client (RFC 8656) needs it.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use hmac::{Hmac, KeyInit, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;

pub(crate) const MAGIC_COOKIE: u32 = 0x2112_A442;
const HEADER: usize = 20;
const INTEGRITY_LENGTH: usize = 20;

pub(crate) const ALLOCATE: u16 = 0x0003;
pub(crate) const REFRESH: u16 = 0x0004;
pub(crate) const SEND: u16 = 0x0006;
pub(crate) const DATA: u16 = 0x0007;
pub(crate) const CREATE_PERMISSION: u16 = 0x0008;

/// The message class, encoded into the type's C0/C1 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Request,
    Indication,
    Success,
    Error,
}

pub(crate) mod attr {
    pub(crate) const USERNAME: u16 = 0x0006;
    pub(crate) const MESSAGE_INTEGRITY: u16 = 0x0008;
    pub(crate) const ERROR_CODE: u16 = 0x0009;
    pub(crate) const LIFETIME: u16 = 0x000D;
    pub(crate) const XOR_PEER_ADDRESS: u16 = 0x0012;
    pub(crate) const DATA: u16 = 0x0013;
    pub(crate) const REALM: u16 = 0x0014;
    pub(crate) const NONCE: u16 = 0x0015;
    pub(crate) const XOR_RELAYED_ADDRESS: u16 = 0x0016;
    pub(crate) const REQUESTED_TRANSPORT: u16 = 0x0019;
}

pub(crate) type TransactionId = [u8; 12];

/// A parsed or to-be-encoded STUN message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) method: u16,
    pub(crate) class: Class,
    pub(crate) transaction: TransactionId,
    pub(crate) attributes: Vec<(u16, Vec<u8>)>,
}

impl Message {
    pub(crate) fn new(method: u16, class: Class, transaction: TransactionId) -> Self {
        Self {
            method,
            class,
            transaction,
            attributes: Vec::new(),
        }
    }

    pub(crate) fn with(mut self, kind: u16, value: Vec<u8>) -> Self {
        self.attributes.push((kind, value));
        self
    }

    pub(crate) fn attribute(&self, kind: u16) -> Option<&[u8]> {
        self.attributes
            .iter()
            .find(|(found, _)| *found == kind)
            .map(|(_, value)| value.as_slice())
    }

    pub(crate) fn text(&self, kind: u16) -> Option<String> {
        self.attribute(kind)
            .map(|value| String::from_utf8_lossy(value).into_owned())
    }

    pub(crate) fn address(&self, kind: u16) -> Option<SocketAddr> {
        decode_xor_address(self.attribute(kind)?, &self.transaction)
    }

    /// The error code of an error response (e.g. 401).
    pub(crate) fn error_code(&self) -> Option<u16> {
        let value = self.attribute(attr::ERROR_CODE)?;
        let class = u16::from(*value.get(2)? & 0x07);
        let number = u16::from(*value.get(3)?);
        Some(class * 100 + number)
    }

    pub(crate) fn lifetime(&self) -> Option<u32> {
        let value: [u8; 4] = self.attribute(attr::LIFETIME)?.try_into().ok()?;
        Some(u32::from_be_bytes(value))
    }

    /// The encoded message, with a MESSAGE-INTEGRITY attribute keyed by `key` when given.
    pub(crate) fn encode(&self, key: Option<&[u8]>) -> Vec<u8> {
        let mut body = Vec::new();
        for (kind, value) in &self.attributes {
            push_attribute(&mut body, *kind, value);
        }
        let mut message = Vec::with_capacity(HEADER + body.len() + 24);
        message.extend_from_slice(&message_type(self.method, self.class).to_be_bytes());
        message.extend_from_slice(&[0, 0]);
        message.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        message.extend_from_slice(&self.transaction);
        message.extend_from_slice(&body);
        if let Some(key) = key {
            // The length covers the integrity attribute it is computed over.
            set_length(&mut message, body.len() + 4 + INTEGRITY_LENGTH);
            let integrity = hmac_sha1(key, &message);
            push_attribute(&mut message, attr::MESSAGE_INTEGRITY, &integrity);
        }
        let length = message.len().saturating_sub(HEADER);
        set_length(&mut message, length);
        message
    }

    /// Parses a STUN message; `None` for anything else (e.g. RTP).
    pub(crate) fn decode(data: &[u8]) -> Option<Self> {
        let header = data.get(..HEADER)?;
        let kind = u16::from_be_bytes([*header.first()?, *header.get(1)?]);
        if kind & 0xC000 != 0 {
            return None;
        }
        let length = usize::from(u16::from_be_bytes([*header.get(2)?, *header.get(3)?]));
        let cookie: [u8; 4] = header.get(4..8)?.try_into().ok()?;
        if u32::from_be_bytes(cookie) != MAGIC_COOKIE {
            return None;
        }
        let transaction: TransactionId = header.get(8..HEADER)?.try_into().ok()?;
        let mut body = data.get(HEADER..HEADER + length)?;
        let mut attributes = Vec::new();
        while !body.is_empty() {
            let kind = u16::from_be_bytes([*body.first()?, *body.get(1)?]);
            let size = usize::from(u16::from_be_bytes([*body.get(2)?, *body.get(3)?]));
            let value = body.get(4..4 + size)?.to_vec();
            attributes.push((kind, value));
            let padded = (size + 3) & !3;
            body = body.get(4 + padded..).unwrap_or(&[]);
        }
        let method = (kind & 0x000F) | ((kind & 0x00E0) >> 1) | ((kind & 0x3E00) >> 2);
        let class = match kind & 0x0110 {
            0x0000 => Class::Request,
            0x0010 => Class::Indication,
            0x0100 => Class::Success,
            _ => Class::Error,
        };
        Some(Self {
            method,
            class,
            transaction,
            attributes,
        })
    }
}

/// Whether `data` carries a valid MESSAGE-INTEGRITY for `key` (used by the test server).
#[cfg(test)]
pub(crate) fn verify_integrity(data: &[u8], key: &[u8]) -> bool {
    let Some(message) = Message::decode(data) else {
        return false;
    };
    let Some(integrity) = message.attribute(attr::MESSAGE_INTEGRITY) else {
        return false;
    };
    // Re-encode everything before the integrity attribute with the same key.
    let mut without = message.clone();
    without
        .attributes
        .retain(|(kind, _)| *kind != attr::MESSAGE_INTEGRITY);
    let encoded = without.encode(Some(key));
    encoded.get(encoded.len() - INTEGRITY_LENGTH..) == Some(integrity)
}

fn message_type(method: u16, class: Class) -> u16 {
    let class_bits = match class {
        Class::Request => 0x0000,
        Class::Indication => 0x0010,
        Class::Success => 0x0100,
        Class::Error => 0x0110,
    };
    (method & 0x000F) | ((method & 0x0070) << 1) | ((method & 0x0F80) << 2) | class_bits
}

fn set_length(message: &mut [u8], length: usize) {
    let bytes = u16::try_from(length).unwrap_or(u16::MAX).to_be_bytes();
    if let Some(slot) = message.get_mut(2..4) {
        slot.copy_from_slice(&bytes);
    }
}

fn push_attribute(buffer: &mut Vec<u8>, kind: u16, value: &[u8]) {
    buffer.extend_from_slice(&kind.to_be_bytes());
    buffer.extend_from_slice(&u16::try_from(value.len()).unwrap_or(u16::MAX).to_be_bytes());
    buffer.extend_from_slice(value);
    let padding = (4 - value.len() % 4) % 4;
    buffer.extend(std::iter::repeat_n(0, padding));
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> Vec<u8> {
    match <Hmac<Sha1> as KeyInit>::new_from_slice(key) {
        Ok(mut mac) => {
            mac.update(data);
            mac.finalize().into_bytes().to_vec()
        }
        Err(_) => vec![0; INTEGRITY_LENGTH],
    }
}

/// The long-term credential key: `MD5(username ":" realm ":" password)`.
pub(crate) fn long_term_key(username: &str, realm: &str, password: &str) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(format!("{username}:{realm}:{password}").as_bytes());
    hasher.finalize().to_vec()
}

pub(crate) fn encode_xor_address(address: SocketAddr, transaction: &TransactionId) -> Vec<u8> {
    let cookie = MAGIC_COOKIE.to_be_bytes();
    let port = address.port() ^ u16::try_from(MAGIC_COOKIE >> 16).unwrap_or(0);
    let mut value = Vec::with_capacity(20);
    value.push(0);
    match address.ip() {
        IpAddr::V4(ip) => {
            value.push(0x01);
            value.extend_from_slice(&port.to_be_bytes());
            value.extend(
                ip.octets()
                    .iter()
                    .zip(cookie.iter())
                    .map(|(byte, mask)| byte ^ mask),
            );
        }
        IpAddr::V6(ip) => {
            value.push(0x02);
            value.extend_from_slice(&port.to_be_bytes());
            let mask = cookie.iter().chain(transaction.iter());
            value.extend(ip.octets().iter().zip(mask).map(|(byte, mask)| byte ^ mask));
        }
    }
    value
}

fn decode_xor_address(value: &[u8], transaction: &TransactionId) -> Option<SocketAddr> {
    let cookie = MAGIC_COOKIE.to_be_bytes();
    let family = *value.get(1)?;
    let port = u16::from_be_bytes([*value.get(2)?, *value.get(3)?])
        ^ u16::try_from(MAGIC_COOKIE >> 16).ok()?;
    let ip = match family {
        0x01 => {
            let bytes: [u8; 4] = value.get(4..8)?.try_into().ok()?;
            let mut octets = [0; 4];
            for ((out, byte), mask) in octets.iter_mut().zip(bytes).zip(cookie) {
                *out = byte ^ mask;
            }
            IpAddr::V4(Ipv4Addr::from(octets))
        }
        0x02 => {
            let bytes: [u8; 16] = value.get(4..20)?.try_into().ok()?;
            let mut octets = [0; 16];
            for ((out, byte), mask) in octets
                .iter_mut()
                .zip(bytes)
                .zip(cookie.iter().chain(transaction.iter()))
            {
                *out = byte ^ mask;
            }
            IpAddr::V6(Ipv6Addr::from(octets))
        }
        _ => return None,
    };
    Some(SocketAddr::new(ip, port))
}

pub(crate) fn new_transaction() -> TransactionId {
    let mut id = [0; 12];
    fastrand::fill(&mut id);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_with_integrity_and_xor_addresses() {
        let transaction = new_transaction();
        let peer: SocketAddr = "203.0.113.9:61000".parse().unwrap();
        let peer6: SocketAddr = "[2001:db8::1]:3478".parse().unwrap();
        let message = Message::new(CREATE_PERMISSION, Class::Request, transaction)
            .with(
                attr::XOR_PEER_ADDRESS,
                encode_xor_address(peer, &transaction),
            )
            .with(
                attr::XOR_RELAYED_ADDRESS,
                encode_xor_address(peer6, &transaction),
            )
            .with(attr::USERNAME, b"user".to_vec());
        let key = long_term_key("user", "realm", "pass");
        let encoded = message.encode(Some(&key));
        assert_eq!(encoded.len() % 4, 0);
        assert!(verify_integrity(&encoded, &key));
        assert!(!verify_integrity(&encoded, b"wrong"));

        let decoded = Message::decode(&encoded).unwrap();
        assert_eq!(
            (decoded.method, decoded.class),
            (CREATE_PERMISSION, Class::Request)
        );
        assert_eq!(decoded.address(attr::XOR_PEER_ADDRESS), Some(peer));
        assert_eq!(decoded.address(attr::XOR_RELAYED_ADDRESS), Some(peer6));
        assert_eq!(decoded.text(attr::USERNAME).as_deref(), Some("user"));
    }

    #[test]
    fn the_long_term_key_is_the_md5_of_the_credentials() {
        assert_eq!(
            long_term_key("user", "realm", "pass"),
            [
                132, 147, 251, 197, 59, 165, 130, 251, 76, 4, 76, 69, 107, 220, 64, 235
            ]
        );
    }

    #[test]
    fn error_codes_and_non_stun_data() {
        let transaction = new_transaction();
        let error = Message::new(ALLOCATE, Class::Error, transaction)
            .with(attr::ERROR_CODE, vec![0, 0, 4, 1]);
        assert_eq!(
            Message::decode(&error.encode(None)).unwrap().error_code(),
            Some(401)
        );
        assert_eq!(Message::decode(&[0x80, 0x60, 0, 0]), None);
    }
}
