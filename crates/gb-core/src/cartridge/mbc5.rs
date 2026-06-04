//! MBC5 — up to 8 MiB ROM + 128 KiB RAM. See Pan Docs §MBC5.
//!
//! Address map:
//! * `0x0000–0x3FFF` — fixed ROM bank 0.
//! * `0x4000–0x7FFF` — switchable ROM bank 0–511 (9 bits, **bank 0 selectable**).
//! * `0xA000–0xBFFF` — switchable 8 KiB RAM bank (0x00–0x0F).
//!
//! Register writes (to the ROM region):
//! * `0x0000–0x1FFF` — RAM enable: low nibble == 0xA enables RAM.
//! * `0x2000–0x2FFF` — ROM bank low 8 bits.
//! * `0x3000–0x3FFF` — ROM bank high bit (bit 0 only).
//! * `0x4000–0x5FFF` — RAM bank (low 4 bits).
//! * `0x6000–0x7FFF` — ignored.

use super::Mapper;

#[derive(Debug)]
pub struct Mbc5 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    rom_bank_count: usize,
    ram_bank_count: usize,
    ram_enable: bool,
    rom_bank_lo: u8, // 8 bits
    rom_bank_hi: u8, // 1 bit (bit 0)
    ram_bank: u8,    // 4 bits
}

impl Mbc5 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        let rom_bank_count = (rom.len() / 0x4000).max(1);
        let ram_bank_count = ram_size / 0x2000;
        Self {
            rom,
            ram: vec![0; ram_size],
            rom_bank_count,
            ram_bank_count,
            ram_enable: false,
            rom_bank_lo: 1, // post-boot default; ROMs always re-set this anyway
            rom_bank_hi: 0,
            ram_bank: 0,
        }
    }

    fn high_bank(&self) -> usize {
        let bank = ((self.rom_bank_hi as usize) << 8) | (self.rom_bank_lo as usize);
        bank & (self.rom_bank_count - 1)
    }

    fn current_ram_bank(&self) -> usize {
        if self.ram_bank_count == 0 {
            0
        } else {
            (self.ram_bank as usize) & (self.ram_bank_count - 1)
        }
    }
}

impl Mapper for Mbc5 {
    fn read_rom(&self, addr: u16) -> u8 {
        let bank = if addr < 0x4000 { 0 } else { self.high_bank() };
        let off = bank * 0x4000 + (addr as usize & 0x3FFF);
        *self.rom.get(off).unwrap_or(&0xFF)
    }

    fn write_rom(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enable = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x2FFF => {
                self.rom_bank_lo = val;
            }
            0x3000..=0x3FFF => {
                self.rom_bank_hi = val & 0x01;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = val & 0x0F;
            }
            _ => {} // 0x6000–0x7FFF is unused on MBC5
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enable || self.ram.is_empty() {
            return 0xFF;
        }
        let off = self.current_ram_bank() * 0x2000 + (addr as usize - 0xA000);
        *self.ram.get(off).unwrap_or(&0xFF)
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_enable || self.ram.is_empty() {
            return;
        }
        let off = self.current_ram_bank() * 0x2000 + (addr as usize - 0xA000);
        if let Some(slot) = self.ram.get_mut(off) {
            *slot = val;
        }
    }

    fn ram(&self) -> Option<&[u8]> {
        if self.ram.is_empty() { None } else { Some(&self.ram) }
    }

    fn load_ram(&mut self, data: &[u8]) {
        let n = data.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&data[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic ROM with N 16-KiB banks. Each byte is `bank_number as u8`,
    /// so banks 0..256 are distinguishable by reading any byte; bank N % 256 for higher.
    fn rom_with_n_banks(n: usize) -> Vec<u8> {
        let mut rom = vec![0u8; n * 0x4000];
        for bank in 0..n {
            for i in 0..0x4000 {
                rom[bank * 0x4000 + i] = bank as u8;
            }
        }
        rom
    }

    #[test]
    fn defaults_select_bank_0_and_bank_1() {
        let m = Mbc5::new(rom_with_n_banks(4), 0);
        assert_eq!(m.read_rom(0x0000), 0);
        assert_eq!(m.read_rom(0x4000), 1);
    }

    #[test]
    fn bank_0_can_be_selected() {
        // Unlike MBC1, writing 0 to the bank register selects bank 0 verbatim.
        let mut m = Mbc5::new(rom_with_n_banks(4), 0);
        m.write_rom(0x2000, 0x00);
        m.write_rom(0x3000, 0x00);
        assert_eq!(m.read_rom(0x4000), 0);
    }

    #[test]
    fn rom_bank_high_bit_extends_to_9_bits() {
        // 512 banks = 8 MiB. Select bank 0x101 (= 257).
        let mut m = Mbc5::new(rom_with_n_banks(512), 0);
        m.write_rom(0x2000, 0x01);
        m.write_rom(0x3000, 0x01);
        // Byte at any addr in 0x4000–0x7FFF equals (257 % 256) = 1.
        assert_eq!(m.read_rom(0x4000), 0x101u16 as u8);
        // And bank 0x100 (256).
        m.write_rom(0x2000, 0x00);
        m.write_rom(0x3000, 0x01);
        assert_eq!(m.read_rom(0x4000), 0x100u16 as u8);
    }

    #[test]
    fn ram_bank_switching_16_banks() {
        // 128 KiB RAM = 16 banks.
        let mut m = Mbc5::new(rom_with_n_banks(2), 16 * 0x2000);
        m.write_rom(0x0000, 0x0A);
        for b in 0u8..16 {
            m.write_rom(0x4000, b);
            m.write_ram(0xA000, 0xC0 + b);
        }
        for b in 0u8..16 {
            m.write_rom(0x4000, b);
            assert_eq!(m.read_ram(0xA000), 0xC0 + b, "bank {b}");
        }
        // Upper nibble of the ram-bank write is masked.
        m.write_rom(0x4000, 0xF3);
        assert_eq!(m.read_ram(0xA000), 0xC0 + 3, "0xF3 → low 4 bits = 3");
    }

    #[test]
    fn ram_writes_only_when_enabled() {
        let mut m = Mbc5::new(rom_with_n_banks(2), 0x2000);
        m.write_ram(0xA000, 0x42);
        assert_eq!(m.read_ram(0xA000), 0xFF);
        m.write_rom(0x0000, 0x0A);
        m.write_ram(0xA000, 0x42);
        assert_eq!(m.read_ram(0xA000), 0x42);
        m.write_rom(0x0000, 0x00);
        assert_eq!(m.read_ram(0xA000), 0xFF);
    }
}
