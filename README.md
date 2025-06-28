# msp_protocol

**Rust implementation of the MultiWii Serial Protocol (MSP v1 + v2).**

[![crates.io](https://img.shields.io/crates/v/msp_protocol)](https://crates.io/crates/msp_protocol)
[![docs.rs](https://docs.rs/msp_protocol/badge.svg)](https://docs.rs/msp_protocol)

---

## Features

- **Streaming parser** (`MspParser`) — 1 byte → state machine → optional packet
- **Serializer** — build & transmit MSP v1/v2 frames
- Zero-copy payload access (`decode_as<T>()`) using `packed_struct`
- Tiny footprint (`smallvec` payload buffer)
 




## Quick start

```rust
use msp_protocol::{
    helpers::{send_request, wait_for_port},
    msp::{codes::MspCommandCode, structs::MspRawImu, MspParser},
};

let mut port   = wait_for_port("/dev/ttyUSB0", 115_200, 200);
let mut parser = MspParser::new();
let mut buf    = [0u8; 256];

send_request(&mut *port, MspCommandCode::MSP_RAW_IMU as u16, &[])?;

loop {
    let n = port.read(&mut buf)?;
    for &b in &buf[..n] {
        if let Ok(Some(pkt)) = parser.parse(b) {
            if pkt.cmd == MspCommandCode::MSP_RAW_IMU as u16 {
                let imu = pkt.decode_as::<MspRawImu>()?;
                println!("{imu:?}");
            }
        }
    }
}
