use std::fmt::*;

#[derive(Debug, Clone)]
pub struct Packet {
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct J1939Packet {
    pub packet: Packet,
    pub tx: bool,
    time_stamp_weight: f64,
}

impl Display for J1939Packet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "{:12.4} {} [{}] {}{}",
            self.time(),
            self.header(),
            self.length(),
            self.data_str(),
            if self.echo() { " (TX)" } else { "" }
        )
    }
}
fn as_hex(data: &[u8]) -> String {
    let mut s = String::new();
    for byte in data {
        write!(&mut s, " {:02X}", byte).expect("Unable to write");
    }
    s[1..].to_string()
}
impl Display for Packet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "{}",
            self.data
                .iter()
                .fold(String::new(), |a, &n| a + &n.to_string() + ", ")
        )
    }
}
impl Packet {
    #[allow(dead_code)]
    pub fn new_rp1210(data: &[u8]) -> Packet {
        Packet {
            data: data.to_vec(),
        }
    }
}
impl J1939Packet {
    #[allow(dead_code)]
    pub fn new_rp1210(data: &[u8], time_stamp_weight: f64) -> J1939Packet {
        J1939Packet {
            packet: Packet::new_rp1210(data),
            tx: false,
            time_stamp_weight,
        }
    }
    pub fn length(&self) -> usize {
        self.packet.data.len() - 6 - self.offset()
    }

    pub fn new_packet(priority: u8, pgn: u32, da: u8, sa: u8, data: &[u8]) -> J1939Packet {
        let da = if pgn >= 0xF000 { 0 } else { da };
        Self::new(
            ((priority as u32) << 24) | (pgn << 8) | ((da as u32) << 8) | (sa as u32),
            data,
        )
    }
    #[allow(dead_code)]
    pub fn new(head: u32, data: &[u8]) -> J1939Packet {
        let pgn = 0xFFFF & (head >> 8);
        let da = if pgn < 0xF000 { 0xFF & pgn } else { 0 } as u8;
        let hb = head.to_be_bytes();
        let buf = [&[hb[2], hb[1], hb[0] & 0x3, hb[0] >> 2, hb[3], da], data].concat();
        J1939Packet {
            packet: Packet::new_rp1210(&buf),
            tx: true,
            time_stamp_weight: 0.0,
        }
    }
    pub fn time(&self) -> f64 {
        if self.tx {
            0.0
        } else {
            // FIXME mask is probably not necessary
            ((0xFF000000 & (self.packet.data[0] as u64) << 24)
                | (0xFF0000 & (self.packet.data[1] as u64) << 16)
                | (0xFF00 & (self.packet.data[2] as u64) << 8)
                | (0xFF & (self.packet.data[3] as u64))) as f64
                * 0.001
                * self.time_stamp_weight
        }
    }

    /// offset into array for common data (tx and not tx)
    fn offset(&self) -> usize {
        if self.tx {
            0
        } else {
            5
        }
    }

    pub fn echo(&self) -> bool {
        self.tx || self.packet.data[4] != 0
    }

    pub fn source(&self) -> u8 {
        self.packet.data[4 + self.offset()]
    }
    pub fn pgn(&self) -> u32 {
        let mut pgn = ((self.packet.data[2 + self.offset()] as u32 & 0xFF) << 16)
            | ((self.packet.data[1 + self.offset()] as u32 & 0xFF) << 8)
            | (self.packet.data[self.offset()] as u32 & 0xFF);
        if pgn < 0xF000 {
            let destination = self.packet.data[5 + self.offset()] as u32;
            pgn |= destination;
        }
        pgn
    }
    pub fn priority(&self) -> u8 {
        self.packet.data[3 + self.offset()] & 0x07
    }
    pub fn header(&self) -> String {
        format!(
            "{:06X}{:02X}",
            ((self.priority() as u32) << 18) | self.pgn(),
            self.source()
        )
    }
    pub fn data_str(&self) -> String {
        as_hex(&self.data())
    }

    pub fn data(&self) -> &[u8] {
        &self.packet.data[self.offset() + 6..]
    }
}
#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_j1939packet_display() {
        assert_eq!(
            "      0.0000 18FFAAFA [3] 01 02 03 (TX)",
            J1939Packet::new(0x18FFAAFA, &[1, 2, 3]).to_string()
        );
        assert_eq!(
            "      0.0000 18FFAAF9 [8] 01 02 03 04 05 06 07 08 (TX)",
            J1939Packet::new(0x18FFAAF9, &[1, 2, 3, 4, 5, 6, 7, 8]).to_string()
        );
        assert_eq!(
            "      0.0000 18FFAAFB [8] FF 00 FF 00 FF 00 FF 00 (TX)",
            J1939Packet::new(0x18FFAAFB, &[0xFF, 00, 0xFF, 00, 0xFF, 00, 0xFF, 00]).to_string()
        );
    }
}
