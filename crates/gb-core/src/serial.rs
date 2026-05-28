//! Serial transfer — registers `SB` (`FF01`) and `SC` (`FF02`).
//!
//! Many test ROMs (Blargg) emit results over serial, so even a stub that
//! captures bytes written to `SB` when `SC` is `0x81` is useful.

#[derive(Debug, Default)]
pub struct Serial {
    pub sb: u8,
    pub sc: u8,
    /// Bytes "transmitted" to a hypothetical link partner. Tests scrape this.
    pub output_log: Vec<u8>,
}

impl Serial {
    pub fn new() -> Self { Self::default() }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF01 => self.sb,
            0xFF02 => 0x7E | self.sc,
            _ => 0xFF,
        }
    }
    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF01 => self.sb = val,
            0xFF02 => {
                self.sc = val;
                if val & 0x81 == 0x81 {
                    self.output_log.push(self.sb);
                    self.sc &= !0x80;
                }
            }
            _ => {}
        }
    }
}
