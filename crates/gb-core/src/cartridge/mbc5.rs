//! MBC5 — up to 8 MiB ROM + 128 KiB RAM. Required for most GBC titles.

use super::Mapper;

#[derive(Debug)]
pub struct Mbc5 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    // TODO: 9-bit ROM bank register, RAM bank, RAM enable.
}

impl Mbc5 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        Self { rom, ram: vec![0; ram_size] }
    }
}

impl Mapper for Mbc5 {
    fn read_rom(&self, addr: u16) -> u8 {
        *self.rom.get(addr as usize).unwrap_or(&0xFF)
    }
    fn write_rom(&mut self, _addr: u16, _val: u8) {}
    fn read_ram(&self, addr: u16) -> u8 {
        let idx = (addr - 0xA000) as usize;
        *self.ram.get(idx).unwrap_or(&0xFF)
    }
    fn write_ram(&mut self, addr: u16, val: u8) {
        let idx = (addr - 0xA000) as usize;
        if let Some(slot) = self.ram.get_mut(idx) { *slot = val; }
    }
}
