//! The SBE message header: four `uint16` in the schema's byte order, 8 bytes
//! (SBE 1.0 RC4, "Message header schema", the recommended encoding).
//!
//! Only the recommended header is supported. A schema whose `messageHeader`
//! composite differs (another width for `blockLength`, extra members) is
//! rejected by the generator, never decoded here with the wrong offsets.

use crate::error::SbeError;
use crate::wire::{self, ByteOrder};

/// Length of the message header on the wire.
pub const HEADER_LEN: usize = 8;

/// A decoded message header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageHeader {
    /// Length of the root block on the wire, excluding the header, groups and
    /// `varData`. May exceed the schema's value when the encoder's schema
    /// version is newer (SBE 1.0 RC4 §5, "Block size").
    pub block_length: u16,
    /// Which message layout of the schema encodes the body.
    pub template_id: u16,
    /// Which schema the template belongs to.
    pub schema_id: u16,
    /// The schema version the encoder used.
    pub version: u16,
}

impl MessageHeader {
    /// Decodes the header from the first 8 bytes of `buf`.
    ///
    /// # Errors
    /// `Truncated` when `buf` is shorter than [`HEADER_LEN`].
    #[inline]
    pub fn decode(buf: &[u8], order: ByteOrder) -> Result<Self, SbeError> {
        Ok(Self {
            block_length: wire::read_u16(buf, 0, order)?,
            template_id: wire::read_u16(buf, 2, order)?,
            schema_id: wire::read_u16(buf, 4, order)?,
            version: wire::read_u16(buf, 6, order)?,
        })
    }

    /// Writes the header into the first 8 bytes of `out`.
    ///
    /// # Errors
    /// `BufferTooSmall` when `out` is shorter than [`HEADER_LEN`]; nothing is
    /// written in that case.
    #[inline]
    pub fn encode(&self, out: &mut [u8], order: ByteOrder) -> Result<(), SbeError> {
        if out.len() < HEADER_LEN {
            return Err(SbeError::BufferTooSmall);
        }
        wire::write_u16(out, 0, self.block_length, order)?;
        wire::write_u16(out, 2, self.template_id, order)?;
        wire::write_u16(out, 4, self.schema_id, order)?;
        wire::write_u16(out, 6, self.version, order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: MessageHeader = MessageHeader {
        block_length: 54,
        template_id: 99,
        schema_id: 100,
        version: 0,
    };

    #[test]
    fn decodes_little_endian_as_the_spec_example_lays_it_out() {
        // SBE 1.0 RC4 §7, "Wire format of an order message", header bytes.
        let b = [0x36, 0x00, 0x63, 0x00, 0x64, 0x00, 0x00, 0x00];
        assert_eq!(MessageHeader::decode(&b, ByteOrder::Little), Ok(H));
    }

    #[test]
    fn decodes_big_endian() {
        let b = [0x00, 0x36, 0x00, 0x63, 0x00, 0x64, 0x00, 0x00];
        assert_eq!(MessageHeader::decode(&b, ByteOrder::Big), Ok(H));
    }

    #[test]
    fn a_short_buffer_is_truncated_not_a_panic() {
        let b = [0x36, 0x00, 0x63, 0x00, 0x64, 0x00, 0x00];
        assert_eq!(
            MessageHeader::decode(&b, ByteOrder::Little),
            Err(SbeError::Truncated)
        );
        assert_eq!(
            MessageHeader::decode(&[], ByteOrder::Big),
            Err(SbeError::Truncated)
        );
    }

    #[test]
    fn encode_then_decode_is_identity_in_both_orders() -> Result<(), SbeError> {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let mut out = [0u8; 8];
            H.encode(&mut out, order)?;
            assert_eq!(MessageHeader::decode(&out, order), Ok(H));
        }
        let mut le = [0u8; 8];
        H.encode(&mut le, ByteOrder::Little)?;
        assert_eq!(le, [0x36, 0x00, 0x63, 0x00, 0x64, 0x00, 0x00, 0x00]);
        Ok(())
    }

    #[test]
    fn encode_into_a_short_buffer_writes_nothing() {
        let mut out = [0xAAu8; 7];
        assert_eq!(
            H.encode(&mut out, ByteOrder::Little),
            Err(SbeError::BufferTooSmall)
        );
        assert_eq!(out, [0xAA; 7]);
    }
}
