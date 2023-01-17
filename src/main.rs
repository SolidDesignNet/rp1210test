mod multiqueue;
mod packet;
#[cfg_attr(
    not(all(target_pointer_width = "32", target_os = "windows")),
    path = "sim.rs"
)]
#[cfg_attr(
    all(target_pointer_width = "32", target_os = "windows"),
    path = "rp1210.rs"
)]
mod rp1210;
mod rp1210_parsing;

use anyhow::Error;
use clap::{arg, Parser};
use multiqueue::*;
use packet::*;
use rp1210::*;
use std::time::{Duration, SystemTime};

const PING_CMD: u8 = 1;
const RX_CMD: u8 = 2;
const TX_CMD: u8 = 3;
const DATA_CMD: u8 = 4;
const EXIT_CMD: u8 = 5;

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

    #[arg(long, short, default_value = "false")]
    verbose: bool,
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
    /// request server to exit
    Exit {
        #[command(flatten)]
        connection: ConnectionDescriptor,
        #[arg(long, default_value = "00",value_parser=hex8)]
        dest: u8,
        #[arg(long, default_value = "FFF1",value_parser=hex32)]
        pgn: u32,
    },
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
        count: u32,
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
        count: u32,
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
        count: u32,
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
        count: u32,
        #[arg(long, default_value = "FFF1",value_parser=hex32)]
        pgn: u32,
    },
}

pub fn main() -> Result<(), Error> {
    let args = RPCommand::parse();

    let bus: MultiQueue<J1939Packet> = MultiQueue::new();
    match args {
        RPCommand::List => list_adapters()?,
        RPCommand::Exit {
            connection,
            pgn,
            dest,
        } => request_exit(&connection.connect(&bus)?, pgn, dest, connection.address)?,
        RPCommand::Log { connection } => {
            let _connect = connection.connect(&bus)?;
            log(&bus);
        }
        RPCommand::Server { connection, pgn } => {
            server(&connection.connect(&bus)?, connection.address, pgn)?;
        }
        RPCommand::Ping {
            connection,
            dest,
            count,
            pgn,
        } => {
            ping(
                connection.verbose,
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
            ping(
                connection.verbose,
                &rp1210,
                count,
                connection.address,
                pgn,
                dest,
            )?;
            tx_bandwidth(
                connection.verbose,
                &rp1210,
                count,
                connection.address,
                pgn,
                dest,
            )?;
            rx_bandwidth(
                connection.verbose,
                &rp1210,
                count,
                connection.address,
                pgn,
                dest,
            )?;
        }
        RPCommand::Rx {
            connection,
            dest,
            count,
            pgn,
        } => {
            rx_bandwidth(
                connection.verbose,
                &connection.connect(&bus)?,
                count,
                connection.address,
                pgn,
                dest,
            )?;
        }
        RPCommand::Tx {
            connection,
            dest,
            count,
            pgn,
        } => {
            tx_bandwidth(
                connection.verbose,
                &connection.connect(&bus)?,
                count,
                connection.address,
                pgn,
                dest,
            )?;
        }
    }
    Ok(())
}

fn request_exit(connection: &Rp1210, pgn: u32, dest: u8, address: u8) -> Result<(), Error> {
    let request = [EXIT_CMD, 0, 0, 0, 0, 0, 0, 0];
    let sent = connection.send(&J1939Packet::new_packet(0x18, pgn, address, dest, &request))?;
    println!("EXIT requested {}", sent);
    Ok(())
}

fn tx_bandwidth(
    verbose: bool,
    rp1210: &Rp1210,
    count: u32,
    address: u8,
    pgn: u32,
    dest: u8,
) -> Result<(), Error> {
    let request: Vec<u8> = [RX_CMD, 0, 0, 0]
        .into_iter()
        .chain((count as u32).to_be_bytes().into_iter())
        .collect();
    let req = rp1210.send(&J1939Packet::new_packet(0x18, pgn, dest, address, &request))?;
    let last = tx(verbose, &rp1210, pgn, dest, address, count)?;
    let time = last.time() - req.time();
    eprintln!(
        "tx time: {:8.4} packet/s: {:8.4}",
        time,
        1000.0 * count as f64 / time
    );
    Ok(())
}

fn rx_bandwidth(
    verbose: bool,
    rp1210: &Rp1210,
    count: u32,
    address: u8,
    pgn: u32,
    dest: u8,
) -> Result<(), Error> {
    let rx_packets = rp1210.bus.iter();
    let request: Vec<u8> = [TX_CMD, 0, 0, 0]
        .into_iter()
        .chain((count as u32).to_be_bytes().into_iter())
        .collect();
    let req = rp1210.send(&J1939Packet::new_packet(0x18, pgn, dest, address, &request))?;
    let last = rx(verbose, rx_packets, pgn, dest, count)?;
    let time = last.time() - req.time();
    eprintln!(
        "rx time: {:8.4} packet/s: {:8.4}",
        time,
        1000.0 * count as f64 / time
    );
    Ok(())
}

fn ping(
    verbose: bool,
    rp1210: &Rp1210,
    count: u32,
    address: u8,
    pgn: u32,
    dest: u8,
) -> Result<(), Error> {
    const LEN: usize = 8;
    let mut buf = [0 as u8; LEN];
    let mut sum = 0.0;
    let mut min = f64::MAX;
    let mut max = 0.0;
    for i in 1..count {
        let i_as_bytes = i.to_be_bytes();
        buf[(LEN - i_as_bytes.len())..LEN].copy_from_slice(&i_as_bytes);
        buf[0] = PING_CMD;

        let ping = J1939Packet::new_packet(0x18, pgn, dest, address, &buf);
        let mut stream = rp1210.bus.iter_for(Duration::from_secs(2));
        let echo = rp1210.send(&ping)?;
        match stream.find(|p| p.source() == dest && p.pgn() == pgn && p.data()[0] == PING_CMD) {
            Some(pong) => {
                let time = pong.time() - echo.time();
                sum += time;
                if time < min {
                    min = time;
                }
                if time > max {
                    max = time;
                }
                if verbose {
                    eprintln!("{:8.4}\t{} -> {}", time, echo, pong)
                }
            }
            None => eprintln!("{} no response", echo),
        }
    }
    println!(
        "ping avg: {:8.4} max: {:8.4} min: {:8.4}",
        sum / count as f64,
        max,
        min
    );
    Ok(())
}

fn server(rp1210: &Rp1210, address: u8, pgn: u32) -> Result<(), Error> {
    println!("SERVER: address: {:02X} pgn: {:04X}", address, pgn);
    rp1210
        .bus
        .iter()
        .filter(|p| p.pgn() == pgn && p.source() != address)
        .try_for_each(|p| -> Result<(), Error> {
            match p.data()[0] {
                PING_CMD => {
                    println!("PING: {:02X} {}", p.source(), p);
                    // pong
                    rp1210.send(&J1939Packet::new_packet(
                        0x18,
                        pgn,
                        p.source(),
                        address,
                        p.data(),
                    ))?;
                }
                RX_CMD => {
                    // receive sequence
                    let count = u32::from_be_bytes(p.data()[4..8].try_into()?);
                    println!("RX {} {}", count, p);
                    rx(false, rp1210.bus.iter(), pgn, address, count)?;
                }
                TX_CMD => {
                    // send sequence
                    let count = u32::from_be_bytes(p.data()[4..8].try_into()?);
                    println!("TX {} {}", count, p);
                    tx(false, rp1210, pgn, address, p.source(), count)?;
                }
                DATA_CMD => {}
                EXIT_CMD => {
                    println!("EXIT: {:02X} {}", p.source(), p);
                    std::process::exit(0);
                }
                _ => {
                    println!("Unknown command: {}", p);
                }
            };
            Ok(())
        })?;
    eprintln!("Server exited!");
    Ok(())
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

/// send sequence of RX, 0, 0, 0, seq:u32
fn tx(
    verbose: bool,
    rp1210: &Rp1210,
    pgn: u32,
    address: u8,
    dest: u8,
    count: u32,
) -> Result<J1939Packet, Error> {
    let mut sent = J1939Packet::default();
    for seq in 0..count {
        let data: Vec<u8> = [DATA_CMD, 0, 0, 0]
            .into_iter()
            .chain(seq.to_be_bytes().into_iter())
            .collect();
        sent = rp1210.send(&J1939Packet::new_packet(0x18, pgn, dest, address, &data))?;
        if verbose {
            println!("tx: {}", sent);
        }
    }
    Ok(sent)
}

/// receive sequence of RX, 0, 0, 0, seq:u32
fn rx(
    verbose: bool,
    rx_packets: impl Iterator<Item = J1939Packet>,
    pgn: u32,
    source: u8,
    count: u32,
) -> Result<J1939Packet, Error> {
    let mut seq = 0;
    rx_packets
        .filter(|p| p.source() == source && p.pgn() == pgn && p.data()[0] == DATA_CMD)
        .take(count as usize)
        .map(|p| -> Result<J1939Packet, Error> {
            let rx_seq = u32::from_be_bytes(p.data()[4..8].try_into()?);
            if rx_seq != seq {
                println!("Invalid seq. expected {} received {}", seq, rx_seq);
            }
            seq = rx_seq + 1;
            if verbose {
                println!("rx: {}", p);
            }
            Ok(p)
        })
        .last()
        .unwrap()
}
