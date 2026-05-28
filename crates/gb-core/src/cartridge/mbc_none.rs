//! Type 0x00: 32 KiB ROM, no banking, no RAM.

use super::Mapper;

#[derive(Debug)]
pub struct MbcNone {
    rom: Vec<u8>,
}

impl MbcNone {
    pub fn new(rom: Vec<u8>) -> Self { Self { rom } }
}

impl Mapper for MbcNone {
    fn read_rom(&self, addr: u16) -> u8 {
        *self.rom.get(addr as usize).unwrap_or(&0xFF)
    }
    fn write_rom(&mut self, _addr: u16, _val: u8) {}
    fn read_ram(&self, _addr: u16) -> u8 { 0xFF }
    fn write_ram(&mut self, _addr: u16, _val: u8) {}
}
