
use std::f64::consts::PI;
use std::io;
use std::io::Write;
use std::thread::sleep;
use anyhow::{Error, Result};
use std::time::{Duration, Instant};
use packed_struct::PackedStruct;
use serialport::SerialPort;
use msp_protocol::msp::{MspPacket, MspPacketData, MspParser};
use msp_protocol::msp::commands::MspCommandCode;
use msp_protocol::msp::MspPacketDirection::ToFlightController;
use msp_protocol::msp::structs::{MspBatteryState, MspRawImu, MspRc, MspRcChannel};

fn send_request(port: &mut dyn SerialPort, cmd: u16, payload:&[u8]) -> Result<()> {
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

fn read_until_response(port: &mut dyn SerialPort, cmd: u16) -> Result<MspPacket> {
    let mut parser = MspParser::new();
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

fn wait_for_port(port_name: &str, baud_rate: u32, timeout_ms: u64) -> Box<dyn SerialPort> {
    loop {
        match serialport::new(port_name, baud_rate)
            .timeout(Duration::from_millis(200))
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

fn main() -> Result<()> {
    let port_name = "/dev/ttyACM0";
    let baud_rate = 1_000_000;
    // let port_name = "/dev/ttyUSB0";
    // let baud_rate = 115200;

    let mut port = wait_for_port(port_name, baud_rate, 200);
    let mut response: MspPacketData = MspPacketData::new();

    let mut raw_cr = MspRc::new();

    let start_time = Instant::now();
    let frequency = 0.5;
    let amplitude = 100.0;
    let offset = 1100.0;

    loop {
        let mut parser = MspParser::new(); // need to be new

        //send_request(&mut *port, MspCommandCode::MSP_RAW_IMU as u16, &[])?;
        send_request(&mut *port, MspCommandCode::MSP_BATTERY_STATE as u16, &[])?;
        send_request(&mut *port, MspCommandCode::MSP_RC as u16, &[])?;

        let sine_value = offset + amplitude * (2.0 * PI * frequency * start_time.elapsed().as_secs_f64()).sin();
        let throttle = sine_value.round() as u16;
        if throttle == 1000 || throttle == 1200 {
            println!("Throttle {}", throttle);
        }
        raw_cr.set_throttle(throttle);
        let to_send = raw_cr.pack();
        send_request(&mut *port, MspCommandCode::MSP_SET_RAW_RC as u16, to_send.unwrap().as_slice())?;

        let resp = port.read(response.as_mut_slice());

        if resp.is_err() {
            println!("Error reading from port: {:?}", resp);
            continue;
        }
        // println!("Received: {:?}", response);

        for b in response.as_slice() {
            let s = parser.parse(*b);
            if let Ok(Some(p)) = s {
                match p {
                    MspCommandCode::MSP_RAW_IMU => {
                        let imu = p.decode_as::<MspRawImu>()?;
                        println!("Imu: {:?}", imu);
                    },
                    MspCommandCode::MSP_BATTERY_STATE => {
                        let b = p.decode_as::<MspBatteryState>()?;
                        // println!("Cell V: {}", b.cell_voltage());
                    },
                    MspCommandCode::MSP_RC => {
                        let b = p.decode_as::<MspRc>()?;
                        print!("\rRC: {:?}    ", b);
                        io::stdout().flush()?;
                    },
                }}
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
