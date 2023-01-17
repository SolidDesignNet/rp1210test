use anyhow::*;
use std::sync::atomic::*;
use std::sync::*;

use crate::multiqueue::*;
use crate::packet::*;

pub struct Rp1210 {
    pub bus: MultiQueue<J1939Packet>,
    pub running: Arc<AtomicBool>,
    pub id: String,
    pub device: i16,
    pub connection_string: String,
}
impl Rp1210 {
    pub fn new(
        _id: &str,
        _device: i16,
        _connection_string: &str,
        _address: u8,
        _bus: MultiQueue<J1939Packet>,
    ) -> Result<Rp1210> {
        stdext::compile_warning!("RP1210 is only usable on 32 bit Windows.  This compilation will only be usable for unit tests.");
        todo!()
    }
    /// background thread to read all packets into queue
    pub fn run(&mut self) {
        todo!()
    }

    /// Send packet and return packet echoed back from adapter
    pub fn send(&self, _packet: &J1939Packet) -> Result<J1939Packet> {
        todo!()
    }
}
