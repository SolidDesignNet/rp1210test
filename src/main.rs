mod multiqueue;
mod packet;
mod rp1210_parsing;

#[cfg_attr(not(target_os = "windows"), path = "sim.rs")]
#[cfg_attr(target_os = "windows", path = "rp1210.rs")]
mod rp1210;

use anyhow::Error;
use clap::{arg, Parser};
use multiqueue::*;
use packet::*;
use rp1210::*;
use std::time::{Duration, SystemTime};

const PING_CMD: u8 = 1;
const RX_CMD: u8 = 2;
const TX_CMD: u8 = 3;

#[derive(Parser, Debug, Default, Clone)]
struct ConnectionDescriptor {
    /// RP1210 Adapter Identifier
    adapter: String,

    /// RP1210 Device ID
    device: u8,

    #[arg(long, default_value = "J1939:Baud=Auto")]
    /// RP1210 Connection String
    connection_string: String,

    #[arg(long, default_value = "F9",value_parser=hex8)]
    /// RP1210 Adapter Address (used for packets send and transport protocol)
    address: u8,
}

fn hex8(str: &str) -> Result<u8, std::num::ParseIntError> {
    u8::from_str_radix(str, 16)
}

fn hex32(str: &str) -> Result<u32, std::num::ParseIntError> {
    u32::from_str_radix(str, 16)
}

impl ConnectionDescriptor {
    fn connect(&self, bus: &MultiQueue<J1939Packet>) -> Result<Rp1210, Error> {
        let mut rp1210 = Rp1210::new(
            &self.adapter,
            self.device as i16,
            &self.connection_string,
            self.address,
            bus.clone(),
        )?;
        rp1210.run();
        Ok(rp1210)
    }
}

#[derive(Parser, Debug, Clone)]
enum RPCommand {
    /// List available RP1210 adapters
    List,
    /// Log all traffic on specified adapter
    Log {
        #[command(flatten)]
        connection: ConnectionDescriptor,
    },
    /// Respond to commands from other instances of rp1210test
    Server {
        #[command(flatten)]
        connection: ConnectionDescriptor,
        #[arg(long, default_value = "FFF1",value_parser=hex32)]
        pgn: u32,
    },
    /// Test latency
    Ping {
        #[command(flatten)]
        connection: ConnectionDescriptor,
        #[arg(long, default_value = "00",value_parser=hex8)]
        dest: u8,
        #[arg(short, long, default_value = "10")]
        count: u64,
        #[arg(long, default_value = "FFF1",value_parser=hex32)]
        pgn: u32,
    },
    /// Composite
    Composite {
        #[command(flatten)]
        connection: ConnectionDescriptor,
        #[arg(long, default_value = "00",value_parser=hex8)]
        dest: u8,
        #[arg(short, long, default_value = "10")]
        count: u64,
        #[arg(long, default_value = "FFF1",value_parser=hex32)]
        pgn: u32,
    },
    /// Test sending bandwidth
    Tx {
        #[command(flatten)]
        connection: ConnectionDescriptor,
        #[arg(long, default_value = "00",value_parser=hex8)]
        dest: u8,
        #[arg(short, long)]
        count: u64,
        #[arg(long, default_value = "FFF1",value_parser=hex32)]
        pgn: u32,
    },
    /// Test receiving bandwidth
    Rx {
        #[command(flatten)]
        connection: ConnectionDescriptor,
        #[arg(long, default_value = "00",value_parser=hex8)]
        dest: u8,
        #[arg(short, long)]
        count: u64,
        #[arg(long, default_value = "FFF1",value_parser=hex32)]
        pgn: u32,
    },
}

pub fn main() -> Result<(), Error> {
    let args = RPCommand::parse();

    let bus: MultiQueue<J1939Packet> = MultiQueue::new();
    match args {
        RPCommand::List => list_adapters()?,
        RPCommand::Log { connection } => {
            let _connect = connection.connect(&bus);
            log(&bus);
        }
        RPCommand::Server { connection, pgn } => {
            server(&connection.connect(&bus)?, connection.address, pgn)
        }
        RPCommand::Ping {
            connection,
            dest,
            count,
            pgn,
        } => {
            ping(
                &connection.connect(&bus)?,
                count,
                connection.address,
                pgn,
                dest,
            )?;
        }
        RPCommand::Composite {
            connection,
            dest,
            count,
            pgn,
        } => {
            let rp1210 = connection.connect(&bus)?;
            ping(&rp1210, count, connection.address, pgn, dest)?;
            tx_bandwidth(&rp1210, count, pgn, dest)?;
            rx_bandwidth(&rp1210, count, pgn, dest)?
        }
        RPCommand::Rx {
            connection,
            dest,
            count,
            pgn,
        } => rx_bandwidth(&connection.connect(&bus)?, count, pgn, dest)?,
        RPCommand::Tx {
            connection,
            dest,
            count,
            pgn,
        } => tx_bandwidth(&connection.connect(&bus)?, count, pgn, dest)?,
    }
    Ok(())
}

fn tx_bandwidth(rp1210: &Rp1210, count: u64, pgn: u32, dest: u8) -> Result<(), Error> {
    let request = or([RX_CMD, 0, 0, 0, 0, 0, 0, 0], count);
    rp1210.send(&J1939Packet::new(0x18_FFF1_00 | (dest as u32), &request))?;
    tx(&rp1210, dest, count)?;
    Ok(())
}

fn rx_bandwidth(rp1210: &Rp1210, count: u64, pgn: u32, dest: u8) -> Result<(), Error> {
    let rx_packets = rp1210.bus.iter();
    let request = or([TX_CMD, 0, 0, 0, 0, 0, 0, 0], count);
    rp1210.send(&J1939Packet::new(0x18_FFF1_00 | (dest as u32), &request))?;
    rx(rx_packets, dest, count)?;
    Ok(())
}

fn ping(rp1210: &Rp1210, count: u64, address: u8, pgn: u32, dest: u8) -> Result<(), Error> {
    const LEN: usize = 8;
    let mut buf = [0 as u8; LEN];
    let mut sum = 0.0;
    let mut min = 0.0;
    let mut max = 0.0;
    for i in 1..count {
        let i_as_bytes = i.to_be_bytes();
        buf[(LEN - i_as_bytes.len())..LEN].copy_from_slice(&i_as_bytes);
        buf[0] = PING_CMD;

        let ping = J1939Packet::new_packet(0x18, pgn, dest, address, &buf);
        let mut stream = rp1210.bus.iter_for(Duration::from_secs(2));
        let echo = rp1210.send(&ping)?;
        match stream.find(|p| p.source() == dest && p.pgn() == pgn && p.data()[0] == PING_CMD) {
            Some(p) => {
                let time = p.time() - echo.time();
                sum += time;
                if time < min {
                    min = time;
                }
                if time > max {
                    max = time;
                }
                eprintln!("{:?}\t{} -> {}", time, echo, p)
            }
            None => eprintln!("{} no response", ping),
        }
    }
    println!("Average: {} max: {} min: {}", sum / count as f64, max, min);
    Ok(())
}

fn server(rp1210: &Rp1210, address: u8, pgn: u32) {
    println!("SERVER: address: {:02X} pgn: {:04X}", address, pgn);
    rp1210
        .bus
        .iter()
        .filter(|p| p.pgn() == pgn && p.source() != address)
        .for_each(|p| {
            match p.data()[0] {
                PING_CMD => {
                    println!("PING: {:02X} {}", p.source(), p);
                    // pong
                    rp1210
                        .send(&J1939Packet::new_packet(
                            0x18,
                            pgn,
                            p.source(),
                            address,
                            &p.data(),
                        ))
                        .unwrap();
                }
                RX_CMD => {
                    println!("RX");
                    // receive sequence
                    let count = to_u64(p.data()) & 0xFFFFFF_FFFFFFFF;
                    let rx_packets = rp1210.bus.iter();
                    rx(rx_packets, p.source(), count).unwrap();
                }
                TX_CMD => {
                    println!("TX");
                    // send sequence
                    let count = to_u64(p.data()) & 0xFFFFFF_FFFFFFFF;
                    tx(&rp1210, p.source(), count).unwrap();
                }
                _ => {
                    println!("Unknown command: {}", p);
                }
            }
        });
    eprintln!("Server exited!");
}

fn log(bus: &MultiQueue<J1939Packet>) {
    let mut count: u64 = 0;
    let mut start = SystemTime::now();
    bus.iter().for_each(|p| {
        println!("{}", p);
        count += 1;
        let millis = start.elapsed().unwrap().as_millis();
        if millis > 10000 {
            eprintln!("{} packet/s", 1000 * count / millis as u64);
            start = SystemTime::now();
            count = 0;
        }
    });
}

fn list_adapters() -> Result<(), Error> {
    println!();
    for n in rp1210_parsing::list_all_products()? {
        println!("{}", n)
    }
    Ok(())
}

fn tx(rp1210: &Rp1210, dest: u8, count: u64) -> Result<(), Error> {
    let head = 0x18_FFF1_00 | (dest as u32);
    for seq in 0..count {
        rp1210.send(&J1939Packet::new(head, &or([0; 8], seq)))?;
    }
    Ok(())
}

fn rx(rx_packets: impl Iterator<Item = J1939Packet>, source: u8, count: u64) -> Result<(), Error> {
    let mut seq = 0;
    rx_packets.filter(|p| p.source() == source).for_each(|p| {
        let bytes: [u8; 8] = p.data()[0..8].try_into().expect("Not 8 bytes!");
        let rx_seq = u64::from_be_bytes(bytes) | 0x00FFFFFF_FFFFFFFF;
        if rx_seq != seq {
            println!("Invalid seq. expected {} received {}", seq, rx_seq);
        }
        seq = rx_seq + 1;
        if seq >= count {
            return;
        }
    });
    Ok(())
}

fn or(data: [u8; 8], value: u64) -> [u8; 8] {
    (u64::from_be_bytes(data) | value).to_be_bytes()
}

fn to_u64(data: &[u8]) -> u64 {
    let u: [u8; 8] = data.try_into().unwrap();
    u64::from_be_bytes(u)
}
