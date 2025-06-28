
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
    pub fn new() -> MspParser {
        Self {
            state: MspParserState::Header1,
            packet_version: MspVersion::V1,
            packet_direction: MspPacketDirection::ToFlightController,
            packet_data_length_remaining: 0,
            packet_cmd: 0,
            packet_data: MspPacketData::new(),
            packet_crc: 0,
            packet_crc_v2: CRCu8::crc8dvb_s2(),
        }
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
        Self::new()
    }
}


