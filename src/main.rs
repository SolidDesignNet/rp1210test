mod multiqueue;
mod packet;
#[cfg_attr(not(target_os = "windows"), path = "sim.rs")]
#[cfg_attr(target_os = "windows", path = "rp1210.rs")]
mod rp1210;
mod rp1210_parsing;
use anyhow::Error;
use multiqueue::*;
use packet::*;
use rp1210::*;

const PING_CMD: u8 = 1;
const RX_CMD: u8 = 2;
const TX_CMD: u8 = 3;

pub fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();

    //create abstract CAN bus
    let bus: MultiQueue<J1939Packet> = MultiQueue::new();

    // UI
    // create a new adapter
    let adapter = args[1].as_str();
    let dev = args[2].parse()?;
    let address = args[3].parse()?;

    let mut rp1210 = Rp1210::new(&adapter, bus.clone())?;
    let closer = rp1210.run(dev, "J1939:Baud=Auto", address)?;

    let command: &str = args[4].as_str();
    match command {
        "log" => {
            // log everything
            bus.iter().for_each(|p| println!("{}", p));
        }
        "server" => {
            let addr: u32 = args[4].parse()?;
            // respond to a ping
            bus.iter().for_each(|p| {
                match p.data()[0] {
                    PING_CMD => {
                        // pong
                        rp1210
                            .send(&J1939Packet::new(0x18FFFF00 | addr, &p.data()))
                            .expect("what?");
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
        }
        "ping" => {
            let dest: u32 = args[5].parse()?;
            let id = 0x18_FFAA_00 | dest;

            let count: u8 = args[6].parse()?;
            let mut buf = [0 as u8; 8];
            let len = buf.len();
            for i in 1..count {
                let i_as_bytes = i.to_be_bytes();
                buf[(len - i_as_bytes.len())..len].copy_from_slice(&i_as_bytes);
                rp1210.send(&J1939Packet::new(id, &buf))?;
            }
        }
        "rx" => {
            let dest: u8 = args[5].parse()?;
            let count = args[6].parse()?;

            let rx_packets = bus.iter();

            let request = or([TX_CMD, 0, 0, 0, 0, 0, 0, 0], count);
            rp1210.send(&J1939Packet::new(0x18_FFAA_00 | (dest as u32), &request))?;

            rx(rx_packets, dest, count)?;
        }
        "tx" => {
            let dest: u8 = args[5].parse()?;
            let count = args[6].parse()?;

            let request = or([RX_CMD, 0, 0, 0, 0, 0, 0, 0], count);
            rp1210.send(&J1939Packet::new(0x18_FFAA_00 | (dest as u32), &request))?;

            tx(&rp1210, dest, count)?;
        }
        &_ => {
            println!("Unknown command: {}", command);
            println!(
                "Usage {} {{adapter}} {{device}} {{address}} (log|server|(ping|rx|tx {{dest}} {{count}})",
                args[0]
            );
        }
    }
    closer();
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
