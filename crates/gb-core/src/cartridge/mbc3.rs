//! MBC3 — up to 2 MiB ROM + 32 KiB RAM, optional RTC.

use super::Mapper;

#[derive(Debug)]
pub struct Mbc3 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    // TODO: bank regs, RAM/RTC enable, RTC registers.
}

impl Mbc3 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        Self { rom, ram: vec![0; ram_size] }
    }
}

impl Mapper for Mbc3 {
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
