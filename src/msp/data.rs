use std::fmt::{Debug, Formatter};
use std::fmt;
use smallvec::{SmallVec, smallvec};

#[derive(Clone, PartialEq)]
pub struct MspPacketData(pub(crate) SmallVec<[u8; 256]>);

impl MspPacketData {
    pub fn new() -> MspPacketData {
        MspPacketData(SmallVec::new())
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.0.as_mut_slice()
    }
}
// By definition an MSP packet cannot be larger than 255 bytes

impl Debug for MspPacketData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in &self.0 {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

impl From<&[u8]> for MspPacketData {
    fn from(data: &[u8]) -> Self {
        MspPacketData(SmallVec::from_slice(data))
    }
}

impl MspPacketData{
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}