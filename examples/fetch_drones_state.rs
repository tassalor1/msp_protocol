
use std::f64::consts::PI;
use std::io;
use std::io::Write;
use anyhow::{Result};
use std::time::{Instant};
use packed_struct::PackedStruct;

use msp_protocol::helpers::{wait_for_port, send_request};
use msp_protocol::msp::parser::MspParser;
use msp_protocol::msp::commands::MspCommandCode;

 



fn main() -> Result<()> {
    let port_name = "/dev/cu.usbmodem3754346A31331";
    let baud_rate = 1_000_000;

    let mut port = wait_for_port(port_name, baud_rate, 200);

    let mut raw_cr = MspRc::new();

    let start_time = Instant::now();
    let frequency = 0.5;
    let amplitude = 100.0;
    let offset = 1100.0;

    let mut buf = [0u8; 256];
    let mut parser_from = MspParser::new_from(); // need to be new

    loop {

        let _ = send_request(&mut *port, MspCommandCode::MSP_RAW_IMU as u16, &[]);
        let _ = send_request(&mut *port, MspCommandCode::MSP_BATTERY_STATE as u16, &[]);
        let _ = send_request(&mut *port, MspCommandCode::MSP_RC as u16, &[]);

        let sine_value = offset + amplitude * (2.0 * PI * frequency * start_time.elapsed().as_secs_f64()).sin();
        let throttle = sine_value.round() as u16;
 
        raw_cr.set_throttle(throttle);
        let to_send = raw_cr.pack();
        let _ = send_request(&mut *port, MspCommandCode::MSP_SET_RAW_RC as u16, to_send.unwrap().as_slice());

        let resp = match port.read(&mut buf){
            Ok(n)                                            => n,
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => 0, // nothing arrived
            Err(e)      => { eprintln!("read err: {e}"); continue; }
        };


        for &byte in &buf[..resp] {
            if let Ok(Some(pkt)) = parser_from.parse(byte) {
                match MspCommandCode::try_from(pkt.cmd) {
                    Ok(MspCommandCode::MSP_RAW_IMU) => {
                        let imu = pkt.decode_as::<MspRawImu>()?;
                        println!("Imu: {:?}", imu);
                    },
                    Ok(MspCommandCode::MSP_BATTERY_STATE) => {
                        let byte = pkt.decode_as::<MspBatteryState>()?;
                        println!("Cell V: {}", byte.cell_voltage());
                    },
                    Ok(MspCommandCode::MSP_RC) => {
                        let byte = pkt.decode_as::<MspRc>()?;
                        print!("\rRC: {:?}    ", byte);
                        io::stdout().flush()?;
                    },
                    _ => {}
                }
            }
        }
    }
}



fn parse_imu_data(data: &[u8]) -> ([i16; 3], [i16; 3], [i16; 3]) {
    let gyro = [
        i16::from_le_bytes([data[0], data[1]]),
        i16::from_le_bytes([data[2], data[3]]),
        i16::from_le_bytes([data[4], data[5]]),
    ];
    let accel = [
        i16::from_le_bytes([data[6], data[7]]),
        i16::from_le_bytes([data[8], data[9]]),
        i16::from_le_bytes([data[10], data[11]]),
    ];
    let mag = [
        i16::from_le_bytes([data[12], data[13]]),
        i16::from_le_bytes([data[14], data[15]]),
        i16::from_le_bytes([data[16], data[17]]),
    ];
    (gyro, accel, mag)
}
