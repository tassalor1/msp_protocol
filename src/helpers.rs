use std::thread::sleep;
use std::time::{Duration};
use anyhow::{Error, Result};
use serialport::SerialPort;

use crate::msp::{
    packet::{MspPacket, MspPacketDirection::ToFlightController},
    parser::MspParser,
};


pub fn send_request(port: &mut dyn SerialPort, cmd: u16, payload:&[u8]) -> Result<()> {
    let motor_req = MspPacket {
        direction: ToFlightController,
        cmd,
        data: payload.into(),
    };
    let mut packet_data = vec![0u8; payload.len() + 6];
    motor_req.serialize(&mut packet_data)
        .map_err(|e| Error::msg(format!("Serialization Error: {:?}", e)))?;
    port.write_all(&packet_data)?;
    Ok(())
}

pub fn read_until_response(port: &mut dyn SerialPort, cmd: u16) -> Result<MspPacket> {
    let mut parser = MspParser::from_fc();
    let mut response: Vec<u8> = vec![0; 64];
    loop {
        port.read(&mut response.as_mut_slice())?;
        for b in &response {
            let s = parser.parse(*b);
            if let Ok(Some(p)) = s {
                if cmd == p.cmd {
                    return Ok(p);
                }
            }
        }
    }
}

pub fn wait_for_port(port_name: &str, baud_rate: u32, timeout_ms: u64) -> Box<dyn SerialPort> {
    loop {
        match serialport::new(port_name, baud_rate)
            .open()
        {
            Ok(port) => return port, // Port opened successfully
            Err(_) => {
                println!("Waiting for {} to reappear...", port_name);
                sleep(Duration::from_millis(timeout_ms));
            }
        }
    }
}