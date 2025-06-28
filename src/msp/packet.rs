use std::fmt::Debug;
use crc_any::CRCu8;
use packed_struct::PackedStruct;
use smallvec::{SmallVec, smallvec};

use crate::msp::{
    data::MspPacketData
};

#[derive(Debug, Clone, PartialEq)]
/// A decoded MSP packet, with a command code, direction and payload
pub struct MspPacket {
    pub cmd: u16,
    pub direction: MspPacketDirection,
    pub data: MspPacketData,
}

/// Packet parsing error
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MspPacketParseError {
    OutputBufferSizeMismatch,
    CrcMismatch { expected: u8, calculated: u8 },
    InvalidData,
    InvalidHeader1,
    InvalidHeader2,
    InvalidDirection,
    InvalidDataLength,
}

/// Packet's desired destination
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MspPacketDirection {
    /// Network byte '<'
    ToFlightController,
    /// Network byte '>'
    FromFlightController,
    /// Network byte '!'
    Unsupported,
}

impl MspPacketDirection {
    /// To network byte
    pub fn to_byte(&self) -> u8 {
        let b = match *self {
            MspPacketDirection::ToFlightController => '<',
            MspPacketDirection::FromFlightController => '>',
            MspPacketDirection::Unsupported => '!',
        };
        b as u8
    }
}

impl MspPacket {
    /// Number of bytes that this packet requires to be packed
    pub fn packet_size_bytes(&self) -> usize {

        let MspPacketData(data) = &self.data;
        6 + data.len()
    }

    /// Number of bytes that this packet requires to be packed
    pub fn packet_size_bytes_v2(&self) -> usize {
        let MspPacketData(data) = &self.data;
        9 + data.len()
    }

    /// Serialize to network bytes
    pub fn serialize(&self, output: &mut [u8]) -> Result<(), MspPacketParseError> {
        let MspPacketData(data) = &self.data;
        let l = output.len();

        if l != self.packet_size_bytes() {
            return Err(MspPacketParseError::OutputBufferSizeMismatch);
        }

        output[0] = b'$';
        output[1] = b'M';
        output[2] = self.direction.to_byte();
        output[3] = data.len() as u8;
        output[4] = self.cmd as u8;

        output[5..l - 1].copy_from_slice(data);

        let mut crc = output[3] ^ output[4];
        for b in data {
            crc ^= *b;
        }
        output[l - 1] = crc;

        Ok(())
    }

    /// Serialize to network bytes
    pub fn serialize_v2(&self, output: &mut [u8]) -> Result<(), MspPacketParseError> {

        let MspPacketData(data) = &self.data;
        let l = output.len();

        if l != self.packet_size_bytes_v2() {
            return Err(MspPacketParseError::OutputBufferSizeMismatch);
        }

        output[0] = b'$';
        output[1] = b'X';
        output[2] = self.direction.to_byte();
        output[3] = 0;
        output[4..6].copy_from_slice(&self.cmd.to_le_bytes());
        output[6..8].copy_from_slice(&(data.len() as u16).to_le_bytes());

        output[8..l - 1].copy_from_slice(data);

        let mut crc = CRCu8::crc8dvb_s2();
        crc.digest(&output[3..l - 1]);
        output[l - 1] = crc.get_crc();

        Ok(())
    }

    pub fn decode_as<T: PackedStruct>(&self) -> Result<T, packed_struct::PackingError> {
        let expected_size = size_of::<T::ByteArray>();

        if self.data.0.len() < expected_size {
            return Err(packed_struct::PackingError::BufferSizeMismatch {
                expected: expected_size,
                actual: self.data.0.len(),
            });
        }

        let byte_array: &T::ByteArray = unsafe {
            &*(self.data.0.as_ptr() as *const T::ByteArray)
        };

        T::unpack(byte_array)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::msp::parser::MspParser;

    #[test]
    fn test_serialize() {
        let packet = MspPacket {
            cmd: 2,
            direction: MspPacketDirection::ToFlightController,
            data: MspPacketData(smallvec![0xbe, 0xef]),
        };

        let size = packet.packet_size_bytes();
        assert_eq!(8, size);

        let mut output = vec![0; size];
        packet.serialize(&mut output).unwrap();
        let expected = ['$' as u8, 'M' as u8, '<' as u8, 2, 2, 0xbe, 0xef, 81];
        assert_eq!(&expected, output.as_slice());

        let crc = 2 ^ 2 ^ 0xBE ^ 0xEF;
        assert_eq!(81, crc);

        let mut packet_parsed = None;
        let mut parser = MspParser::new();
        for b in output {
            let s = parser.parse(b);
            if let Ok(Some(p)) = s {
                packet_parsed = Some(p);
            }
        }

        assert_eq!(packet, packet_parsed.unwrap());
    }

    #[test]
    fn test_roundtrip() {
        fn roundtrip(packet: &MspPacket) {
            let size = packet.packet_size_bytes();
            let mut output = vec![0; size];

            packet.serialize(&mut output).unwrap();

            let mut parser = MspParser::new();
            let mut packet_parsed = None;
            for b in output {
                let s = parser.parse(b);
                if let Ok(Some(p)) = s {
                    packet_parsed = Some(p);
                }
            }
            assert_eq!(packet, &packet_parsed.unwrap());
        }

        {
            let packet = MspPacket {
                cmd: 1,
                direction: MspPacketDirection::ToFlightController,
                data: MspPacketData(smallvec![0x00, 0x00, 0x00]),
            };
            roundtrip(&packet);
        }

        {
            let packet = MspPacket {
                cmd: 200,
                direction: MspPacketDirection::FromFlightController,
                data: MspPacketData::new(),
            };
            roundtrip(&packet);
        }

        {
            let packet = MspPacket {
                cmd: 100,
                direction: MspPacketDirection::Unsupported,
                data: MspPacketData(smallvec![0x44, 0x20, 0x00, 0x80]),
            };
            roundtrip(&packet);
        }
    }
}