//! MBC2 — up to 256 KiB ROM + 512×4-bit built-in RAM.

use super::Mapper;

#[derive(Debug)]
pub struct Mbc2 {
    rom: Vec<u8>,
    ram: [u8; 512], // 4-bit values
}

impl Mbc2 {
    pub fn new(rom: Vec<u8>) -> Self {
        Self { rom, ram: [0; 512] }
    }
}

impl Mapper for Mbc2 {
    fn read_rom(&self, addr: u16) -> u8 {
        *self.rom.get(addr as usize).unwrap_or(&0xFF)
    }
    fn write_rom(&mut self, _addr: u16, _val: u8) {}
    fn read_ram(&self, addr: u16) -> u8 {
        let idx = (addr - 0xA000) as usize & 0x1FF;
        0xF0 | (self.ram[idx] & 0x0F)
    }
    fn write_ram(&mut self, addr: u16, val: u8) {
        let idx = (addr - 0xA000) as usize & 0x1FF;
        self.ram[idx] = val & 0x0F;
    }

    fn ram(&self) -> Option<&[u8]> { Some(&self.ram) }

    fn load_ram(&mut self, data: &[u8]) {
        let n = data.len().min(self.ram.len());
        for (slot, src) in self.ram.iter_mut().zip(data.iter()).take(n) {
            *slot = *src & 0x0F;
        }
    }
}
