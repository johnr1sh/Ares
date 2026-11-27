//! Versioned, bounded ARES application-frame codec.
//!
//! This module only frames bytes. It does not authenticate, encrypt, authorize,
//! or protect a frame in transit. Callers must use it inside an authenticated,
//! encrypted transport and apply a replay policy after authentication.

use core::fmt;

pub const FRAME_MAGIC: [u8; 4] = *b"ARES";
pub const PROTOCOL_VERSION: u8 = 1;
pub const HEADER_LEN: usize = 36;
pub const MAX_PAYLOAD_LEN: usize = 64 * 1024;

/// Supported application-frame kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageKind {
    Probe = 1,
    Acknowledgement = 2,
    Data = 3,
}

impl TryFrom<u8> for MessageKind {
    type Error = FrameError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Probe),
            2 => Ok(Self::Acknowledgement),
            3 => Ok(Self::Data),
            other => Err(FrameError::UnknownMessageKind(other)),
        }
    }
}

/// A borrowed view of a validated frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub kind: MessageKind,
    pub session_id: [u8; 16],
    pub sequence: u64,
    pub payload: &'a [u8],
}

/// Errors returned by the bounded frame codec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TruncatedHeader { actual: usize, required: usize },
    InvalidMagic,
    UnsupportedVersion(u8),
    UnknownMessageKind(u8),
    ReservedFlagsSet(u16),
    PayloadTooLarge { actual: usize, maximum: usize },
    PayloadLengthMismatch { declared: usize, actual: usize },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader { actual, required } => {
                write!(f, "truncated frame header: got {actual} bytes, need {required}")
            }
            Self::InvalidMagic => write!(f, "invalid frame magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported protocol version: {version}")
            }
            Self::UnknownMessageKind(kind) => write!(f, "unknown message kind: {kind}"),
            Self::ReservedFlagsSet(flags) => {
                write!(f, "reserved frame flags must be zero, got {flags:#06x}")
            }
            Self::PayloadTooLarge { actual, maximum } => {
                write!(f, "payload too large: {actual} bytes, maximum is {maximum}")
            }
            Self::PayloadLengthMismatch { declared, actual } => {
                write!(f, "payload length mismatch: declared {declared}, received {actual}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// Encode one frame. This function enforces the format's allocation limit.
pub fn encode_frame(
    kind: MessageKind,
    session_id: [u8; 16],
    sequence: u64,
    payload: &[u8],
) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(FrameError::PayloadTooLarge {
            actual: payload.len(),
            maximum: MAX_PAYLOAD_LEN,
        });
    }

    let payload_len = u32::try_from(payload.len()).map_err(|_| FrameError::PayloadTooLarge {
        actual: payload.len(),
        maximum: MAX_PAYLOAD_LEN,
    })?;

    let mut encoded = Vec::with_capacity(HEADER_LEN + payload.len());
    encoded.extend_from_slice(&FRAME_MAGIC);
    encoded.push(PROTOCOL_VERSION);
    encoded.push(kind as u8);
    encoded.extend_from_slice(&0_u16.to_be_bytes());
    encoded.extend_from_slice(&session_id);
    encoded.extend_from_slice(&sequence.to_be_bytes());
    encoded.extend_from_slice(&payload_len.to_be_bytes());
    encoded.extend_from_slice(payload);
    Ok(encoded)
}

/// Validate a complete frame without allocating or copying its payload.
///
/// The caller must authenticate the enclosing transport before trusting the
/// returned fields. This decoder deliberately performs no cryptography.
pub fn decode_frame(bytes: &[u8]) -> Result<Frame<'_>, FrameError> {
    if bytes.len() < HEADER_LEN {
        return Err(FrameError::TruncatedHeader {
            actual: bytes.len(),
            required: HEADER_LEN,
        });
    }

    if bytes.get(0..4) != Some(&FRAME_MAGIC[..]) {
        return Err(FrameError::InvalidMagic);
    }

    if bytes[4] != PROTOCOL_VERSION {
        return Err(FrameError::UnsupportedVersion(bytes[4]));
    }

    let kind = MessageKind::try_from(bytes[5])?;
    let flags = u16::from_be_bytes([bytes[6], bytes[7]]);
    if flags != 0 {
        return Err(FrameError::ReservedFlagsSet(flags));
    }

    let mut session_id = [0_u8; 16];
    session_id.copy_from_slice(&bytes[8..24]);

    let sequence = u64::from_be_bytes([
        bytes[24], bytes[25], bytes[26], bytes[27],
        bytes[28], bytes[29], bytes[30], bytes[31],
    ]);
    let declared_len = u32::from_be_bytes([
        bytes[32], bytes[33], bytes[34], bytes[35],
    ]) as usize;

    if declared_len > MAX_PAYLOAD_LEN {
        return Err(FrameError::PayloadTooLarge {
            actual: declared_len,
            maximum: MAX_PAYLOAD_LEN,
        });
    }

    let actual_len = bytes.len() - HEADER_LEN;
    if declared_len != actual_len {
        return Err(FrameError::PayloadLengthMismatch {
            declared: declared_len,
            actual: actual_len,
        });
    }

    Ok(Frame {
        kind,
        session_id,
        sequence,
        payload: &bytes[HEADER_LEN..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip_preserves_all_fields() {
        let session_id = *b"0123456789abcdef";
        let payload = b"health-probe";
        let encoded = encode_frame(MessageKind::Probe, session_id, 42, payload).unwrap();
        let decoded = decode_frame(&encoded).unwrap();

        assert_eq!(decoded.kind, MessageKind::Probe);
        assert_eq!(decoded.session_id, session_id);
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn empty_payload_is_valid() {
        let encoded = encode_frame(MessageKind::Acknowledgement, [7; 16], 0, &[]).unwrap();
        let decoded = decode_frame(&encoded).unwrap();
        assert_eq!(decoded.kind, MessageKind::Acknowledgement);
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn truncated_header_is_rejected() {
        assert!(matches!(
            decode_frame(&[0; HEADER_LEN - 1]),
            Err(FrameError::TruncatedHeader { .. })
        ));
    }

    #[test]
    fn invalid_magic_and_version_are_rejected() {
        let mut encoded = encode_frame(MessageKind::Data, [0; 16], 1, b"x").unwrap();
        encoded[0] = b'X';
        assert_eq!(decode_frame(&encoded), Err(FrameError::InvalidMagic));

        encoded[0..4].copy_from_slice(&FRAME_MAGIC);
        encoded[4] = PROTOCOL_VERSION + 1;
        assert_eq!(
            decode_frame(&encoded),
            Err(FrameError::UnsupportedVersion(PROTOCOL_VERSION + 1))
        );
    }

    #[test]
    fn unknown_kind_and_reserved_flags_are_rejected() {
        let mut encoded = encode_frame(MessageKind::Probe, [0; 16], 1, &[]).unwrap();
        encoded[5] = 255;
        assert_eq!(
            decode_frame(&encoded),
            Err(FrameError::UnknownMessageKind(255))
        );

        encoded[5] = MessageKind::Probe as u8;
        encoded[7] = 1;
        assert_eq!(
            decode_frame(&encoded),
            Err(FrameError::ReservedFlagsSet(1))
        );
    }

    #[test]
    fn mismatched_payload_length_is_rejected() {
        let mut encoded = encode_frame(MessageKind::Data, [0; 16], 1, b"abc").unwrap();
        encoded[35] = 2;
        assert_eq!(
            decode_frame(&encoded),
            Err(FrameError::PayloadLengthMismatch {
                declared: 2,
                actual: 3
            })
        );

        encoded.truncate(HEADER_LEN + 2);
        encoded[35] = 3;
        assert_eq!(
            decode_frame(&encoded),
            Err(FrameError::PayloadLengthMismatch {
                declared: 3,
                actual: 2
            })
        );
    }

    #[test]
    fn oversized_payload_is_rejected_before_encoding() {
        let payload = vec![0; MAX_PAYLOAD_LEN + 1];
        assert!(matches!(
            encode_frame(MessageKind::Data, [0; 16], 1, &payload),
            Err(FrameError::PayloadTooLarge { .. })
        ));
    }
}
