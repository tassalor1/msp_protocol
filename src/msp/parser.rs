
use std::fmt::Debug;
use std::mem;
use crc_any::CRCu8;

use crate::msp::{
    packet::{MspPacketDirection,MspPacket, MspPacketParseError},
    data::MspPacketData
};

#[derive(Copy, Clone, PartialEq, Debug)]
enum MspParserState {
    Header1,
    Header2,
    Direction,
    FlagV2,
    DataLength,
    DataLengthV2,
    Command,
    CommandV2,
    Data,
    DataV2,
    Crc,
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum MspVersion {
    V1,
    V2,
}

#[derive(Debug)]
/// Parser that can find packets from a raw byte stream
pub struct MspParser {
    state: MspParserState,
    packet_version: MspVersion,
    packet_direction: MspPacketDirection,
    packet_cmd: u16,
    packet_data_length_remaining: usize,
    packet_data: MspPacketData,
    packet_crc: u8,
    packet_crc_v2: CRCu8,
}

impl MspParser {
    /// Create a new parser
    pub fn new(dir:MspPacketDirection ) -> MspParser {
        Self {
            state: MspParserState::Header1,
            packet_version: MspVersion::V1,
            packet_direction: dir,
            packet_data_length_remaining: 0,
            packet_cmd: 0,
            packet_data: MspPacketData::new(),
            packet_crc: 0,
            packet_crc_v2: CRCu8::crc8dvb_s2(),
        }
    }

    pub fn from_fc() -> Self {
        Self::new(MspPacketDirection::FromFlightController)
    }
    pub fn to_fc() -> Self {
        Self::new(MspPacketDirection::ToFlightController)
    }

    /// Are we waiting for the header of a brand new packet?
    pub fn state_is_between_packets(&self) -> bool {
        self.state == MspParserState::Header1
    }

    /// Parse the next input byte. Returns a valid packet whenever a full packet is received, otherwise
    /// restarts the state of the parser.
    pub fn parse(&mut self, input: u8) -> Result<Option<MspPacket>, MspPacketParseError> {
        match self.state {
            MspParserState::Header1 => {
                if input == b'$' {
                    self.state = MspParserState::Header2;
                } else {
                    self.reset();
                }
            }

            MspParserState::Header2 => {
                self.packet_version = match input as char {
                    'M' => MspVersion::V1,
                    'X' => MspVersion::V2,
                    _ => {
                        self.reset();
                        return Err(MspPacketParseError::InvalidHeader2);
                    }
                };

                self.state = MspParserState::Direction;
            }

            MspParserState::Direction => {
                match input {
                    60 => self.packet_direction = MspPacketDirection::ToFlightController, // '>'
                    62 => self.packet_direction = MspPacketDirection::FromFlightController, // '<'
                    33 => self.packet_direction = MspPacketDirection::Unsupported, // '!' error
                    _ => {
                        self.reset();
                        return Err(MspPacketParseError::InvalidDirection);
                    }
                }

                self.state = match self.packet_version {
                    MspVersion::V1 => MspParserState::DataLength,
                    MspVersion::V2 => MspParserState::FlagV2,
                };
            }

            MspParserState::FlagV2 => {
                // uint8, flag, usage to be defined (set to zero)
                self.state = MspParserState::CommandV2;
                self.packet_data = MspPacketData::new();
                self.packet_crc_v2.digest(&[input]);
            }

            MspParserState::CommandV2 => {
                let MspPacketData(data) = &mut self.packet_data;
                data.push(input);

                if data.len() == 2 {
                    let mut s = [0u8; size_of::<u16>()];
                    s.copy_from_slice(data);
                    self.packet_cmd = u16::from_le_bytes(s);

                    self.packet_crc_v2.digest(&data);
                    data.clear();
                    self.state = MspParserState::DataLengthV2;
                }
            }

            MspParserState::DataLengthV2 => {

                let MspPacketData(data) = &mut self.packet_data;
                data.push(input);

                if data.len() == 2 {
                    let mut s = [0u8; size_of::<u16>()];
                    s.copy_from_slice(data);
                    self.packet_data_length_remaining = u16::from_le_bytes(s).into();
                    self.packet_crc_v2.digest(data);
                    data.clear();
                    if self.packet_data_length_remaining == 0 {
                        self.state = MspParserState::Crc;
                    } else {
                        self.state = MspParserState::DataV2;
                    }
                }
            }

            MspParserState::DataV2 => {
                let MspPacketData(data) = &mut self.packet_data;
                data.push(input);
                self.packet_data_length_remaining -= 1;

                if self.packet_data_length_remaining == 0 {
                    self.state = MspParserState::Crc;
                }
            }

            MspParserState::DataLength => {
                let MspPacketData(data) = &mut self.packet_data;
                self.packet_data_length_remaining = input as usize;
                self.state = MspParserState::Command;
                self.packet_crc ^= input;
                data.clear();
            }

            MspParserState::Command => {
                self.packet_cmd = input as u16;

                if self.packet_data_length_remaining == 0 {
                    self.state = MspParserState::Crc;
                } else {
                    self.state = MspParserState::Data;
                }

                self.packet_crc ^= input;
            }

            MspParserState::Data => {
                let MspPacketData(data) = &mut self.packet_data;
                data.push(input);
                self.packet_data_length_remaining -= 1;

                self.packet_crc ^= input;

                if self.packet_data_length_remaining == 0 {
                    self.state = MspParserState::Crc;
                }
            }

            MspParserState::Crc => {
                let MspPacketData(data) = &mut self.packet_data;
                if self.packet_version == MspVersion::V2 {
                    self.packet_crc_v2.digest(data);
                    self.packet_crc = self.packet_crc_v2.get_crc();
                }

                let packet_crc = self.packet_crc;
                if input != packet_crc {
                    self.reset();
                    return Err(MspPacketParseError::CrcMismatch {
                        expected: input,
                        calculated: packet_crc,
                    });
                }

                let mut n = MspPacketData::new();
                mem::swap(&mut self.packet_data, &mut n);

                let packet = MspPacket {
                    cmd: self.packet_cmd,
                    direction: self.packet_direction,
                    data: n,
                };

                self.reset();

                return Ok(Some(packet));
            }
        }

        Ok(None)
    }

    pub fn reset(&mut self) {
        let MspPacketData(data) = &mut self.packet_data;
        self.state = MspParserState::Header1;
        self.packet_direction = MspPacketDirection::ToFlightController;
        self.packet_data_length_remaining = 0;
        self.packet_cmd = 0;
        data.clear();
        self.packet_crc = 0;
        self.packet_crc_v2.reset();
    }
}

impl Default for MspParser {
    fn default() -> Self {
        Self::to_fc()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::msp::parser::{self, MspParser};
    use crate::msp::packet::MspPacket;
    use crate::msp::commands::MspCommandCode;
    use smallvec::smallvec;
    
    #[test]
    fn parse_v1() {
        // build packet we epxect from drone
        let pkt = MspPacket {
            cmd: MspCommandCode::MSP_FC_VARIANT as u16,
            direction: MspPacketDirection::FromFlightController,
            data: MspPacketData(smallvec![0xbe, 0xef]),
        };

        //seralize this packet 
        let mut buf = vec![0u8; pkt.packet_size_bytes()];
        pkt.serialize(&mut buf).unwrap();

        //parse the seralized packet
        let mut parser = MspParser::from_fc();
        let mut result = None;
        for byte in buf{
            if let Ok(Some(pkt)) = parser.parse( byte) {
                result = Some(pkt);
            }
        }
        //assert the parsed pkt against the original packet
        assert_eq!(Some(pkt), result);
    }

    #[test]
    fn parse_v2() {
        // build packet we epxect from drone
        let pkt = MspPacket {
            cmd: MspCommandCode::MSP_FC_VARIANT as u16,
            direction: MspPacketDirection::FromFlightController,
            data: MspPacketData(smallvec![0xbe, 0xef]),
        };

        //seralize this packet 
        let mut buf = vec![0u8; pkt.packet_size_bytes_v2()];
        pkt.serialize_v2(&mut buf).unwrap();

        //parse the seralized packet
        let mut parser = MspParser::from_fc();
        let mut result = None;
        for byte in buf{
            if let Ok(Some(pkt)) = parser.parse( byte) {
                result = Some(pkt);
            }
        }
        //assert the parsed pkt against the original packet
        assert_eq!(Some(pkt), result);
    }
    #[test]
    fn bad_crc_header() {
        // build packet we epxect from drone
        let pkt = MspPacket {
            cmd: MspCommandCode::MSP_FC_VARIANT as u16,
            direction: MspPacketDirection::FromFlightController,
            data: MspPacketData(smallvec![0xbe, 0xef]),
        };

        //seralize this packet 
        let mut buf = vec![0u8; pkt.packet_size_bytes()];
        pkt.serialize(&mut buf).unwrap();
        //take last byte and corrupt
        let bad_crc = buf.last_mut().unwrap();
        *bad_crc ^= 0xFF;

        //parse all but last bad byte
        let mut parser = MspParser::from_fc();
        for &b in &buf[..buf.len() - 1] {
            assert_eq!(parser.parse(b).unwrap(), None);
        }
        // parse last value
        let err = parser.parse(buf[buf.len() - 1]).unwrap_err();

        // calculate what the parser *would* have computed
        let expected_crc = {
            let mut crc = buf[3] ^ buf[4];
            for &d in &buf[5..buf.len()-1] { crc ^= d; }
            crc
        };
        
        //assert the parsed pkt against the original packet
        assert_eq!(err, MspPacketParseError::CrcMismatch {
                expected: buf[buf.len() - 1],
                calculated: expected_crc
            });
    }


}