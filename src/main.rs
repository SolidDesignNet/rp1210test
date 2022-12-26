mod multiqueue;
mod packet;
#[cfg_attr(not(target_os = "windows"), path = "sim.rs")]
#[cfg_attr(target_os = "windows", path = "rp1210.rs")]
mod rp1210;
mod rp1210_parsing;
use std::time::{Duration, SystemTime};

use anyhow::Error;
use multiqueue::*;
use packet::*;
use rp1210::*;

const PING_CMD: u8 = 1;
const RX_CMD: u8 = 2;
const TX_CMD: u8 = 3;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone)]
pub struct Connection {
    #[arg(short, long)]
    adapter: String,
    #[arg(short, long)]
    device: u8,
    #[arg(short, long)]
    connection_string: Option<String>,
    #[arg(long)]
    address: Option<u8>,
}

impl Connection {
    fn connect(&self, bus: &MultiQueue<J1939Packet>) -> Result<(Rp1210, Box<dyn Fn()>), Error> {
        let mut rp1210 = Rp1210::new(&self.adapter, bus.clone())?;
        let closer = rp1210.run(
            self.device as i16,
            &self
                .connection_string
                .unwrap_or("J1939:Baud=Auto".to_string()),
            self.address(),
        )?;
        Ok((rp1210, closer))
    }

    pub(crate) fn address(&self) -> u8 {
        self.address.unwrap_or(254)
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    List,
    Log(Connection),
    Server {
        connection: Connection,
        dest: u8,
    },
    Ping {
        connection: Connection,
        dest: u8,
        count: u64,
    },
    Tx {
        connection: Connection,
        dest: u8,
        count: u64,
    },
    Rx {
        connection: Connection,
        dest: u8,
        count: u64,
    },
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

pub fn main() -> Result<(), Error> {
    let args = Args::parse();

    //create abstract CAN bus
    let bus: MultiQueue<J1939Packet> = MultiQueue::new();

    match args.command {
        Command::List => {
            println!();
            for n in rp1210_parsing::list_all_products()? {
                println!("{}", n)
            }
        }
        Command::Log(adapter) => {
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

        Command::Server { connection, dest } => {
            let (rp1210, closer) = connection.connect(&bus).unwrap();
            // respond to a ping
            bus.iter().filter(|p| p.source() == dest).for_each(|p| {
                match p.data()[0] {
                    PING_CMD => {
                        // pong
                        rp1210
                            .send(&J1939Packet::new(
                                0x18FFFF00 | connection.address() as u32,
                                &p.data(),
                            ))
                            .unwrap();
                    }
                    RX_CMD => {
                        // receive sequence
                        let count = to_u64(p.data()) & 0xFFFFFF_FFFFFFFF;
                        let rx_packets = bus.iter();
                        rx(rx_packets, p.source(), count).unwrap();
                    }
                    TX_CMD => {
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
        Command::Ping {
            connection,
            dest,
            count,
        } => {
            let (rp1210, closer) = connection.connect(&bus).unwrap();

            let id = 0x18_FFAA_00 | (dest as u32);
            let mut buf = [0 as u8; 8];
            buf[0] = PING_CMD;
            let len = buf.len();
            eprintln!("buf1 {:?}", buf);
            for i in 1..count {
                let i_as_bytes = i.to_be_bytes();
                buf[(len - i_as_bytes.len())..len].copy_from_slice(&i_as_bytes);
                eprintln!("buf2 {:?}", buf);
                let ping = J1939Packet::new(id, &buf);
                eprintln!("ping {}", ping);

                let start = SystemTime::now();
                let mut i = bus.iter_for(Duration::from_secs(2));
                rp1210.send(&ping)?;
                let pong = i.find(|p| p.source() == dest && p.data()[0] == PING_CMD);
                match pong {
                    Some(p) => eprintln!("{} -> {:?} in {:?}", ping, p, start.elapsed()),
                    None => eprintln!("{} no response in {:?}", ping, start.elapsed()),
                }
            }
        }
        Command::Rx {
            connection,
            dest,
            count,
        } => {
            let (rp1210, closer) = connection.connect(&bus).unwrap();
            let rx_packets = bus.iter();

            let request = or([TX_CMD, 0, 0, 0, 0, 0, 0, 0], count);
            rp1210.send(&J1939Packet::new(0x18_FFAA_00 | (dest as u32), &request))?;

            rx(rx_packets, dest, count)?;
        }
        Command::Tx {
            connection,
            dest,
            count,
        } => {
            let (rp1210, closer) = connection.connect(&bus).unwrap();
            let request = or([RX_CMD, 0, 0, 0, 0, 0, 0, 0], count);
            rp1210.send(&J1939Packet::new(0x18_FFAA_00 | (dest as u32), &request))?;

            tx(&rp1210, dest, count)?;
        }
        Command::List => {}
    }
    // FIXME
    //    closer();
    Ok(())
}

fn tx(rp1210: &Rp1210, dest: u8, count: u64) -> Result<(), Error> {
    let head = 0x18_FFAA_00 | (dest as u32);
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
