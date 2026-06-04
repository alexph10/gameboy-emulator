//! MBC1 — up to 2 MiB ROM + 32 KiB RAM. See Pan Docs §MBC1.
//!
//! Address map:
//! * `0x0000–0x3FFF` — ROM bank 0 (or `bank1<<5` in advanced mode if ROM≥1 MiB)
//! * `0x4000–0x7FFF` — ROM bank `(bank2<<5) | bank1` (with the usual
//!   "any bank ending in 0 → +1" quirk)
//! * `0xA000–0xBFFF` — RAM bank (gated by `ram_enable`)
//!
//! Register writes:
//! * `0x0000–0x1FFF` — `ram_enable`: low nibble == 0xA enables RAM, else disables.
//! * `0x2000–0x3FFF` — `bank1`: low 5 bits of the ROM bank number; writing 0 → 1.
//! * `0x4000–0x5FFF` — `bank2`: 2 bits; selects RAM bank (mode=1) or upper
//!   ROM bank bits (mode=0).
//! * `0x6000–0x7FFF` — `mode`: 0 = ROM-banking, 1 = RAM-banking.

use super::Mapper;

#[derive(Debug)]
pub struct Mbc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    rom_bank_count: usize,
    ram_bank_count: usize,
    ram_enable: bool,
    bank1: u8, // 5 bits
    bank2: u8, // 2 bits
    mode: u8,  // 1 bit
}

impl Mbc1 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        let rom_bank_count = (rom.len() / 0x4000).max(1);
        let ram_bank_count = (ram_size / 0x2000).max(0);
        Self {
            rom,
            ram: vec![0; ram_size],
            rom_bank_count,
            ram_bank_count,
            ram_enable: false,
            bank1: 1,
            bank2: 0,
            mode: 0,
        }
    }

    /// Effective bank number for the 0x0000–0x3FFF region.
    fn low_bank(&self) -> usize {
        if self.mode == 1 {
            ((self.bank2 as usize) << 5) & (self.rom_bank_count - 1)
        } else {
            0
        }
    }

    /// Effective bank number for the 0x4000–0x7FFF region.
    fn high_bank(&self) -> usize {
        let bank = ((self.bank2 as usize) << 5) | (self.bank1 as usize);
        bank & (self.rom_bank_count - 1)
    }

    /// Effective RAM bank (only meaningful in mode 1).
    fn ram_bank(&self) -> usize {
        if self.mode == 1 && self.ram_bank_count > 1 {
            (self.bank2 as usize) & (self.ram_bank_count - 1)
        } else {
            0
        }
    }
}

impl Mapper for Mbc1 {
    fn read_rom(&self, addr: u16) -> u8 {
        let bank = if addr < 0x4000 { self.low_bank() } else { self.high_bank() };
        let off = bank * 0x4000 + (addr as usize & 0x3FFF);
        *self.rom.get(off).unwrap_or(&0xFF)
    }

    fn write_rom(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enable = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                let v = val & 0x1F;
                // Writing 0 to `bank1` actually selects bank 1.
                self.bank1 = if v == 0 { 1 } else { v };
            }
            0x4000..=0x5FFF => {
                self.bank2 = val & 0b11;
            }
            0x6000..=0x7FFF => {
                self.mode = val & 1;
            }
            _ => {}
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enable || self.ram.is_empty() {
            return 0xFF;
        }
        let off = self.ram_bank() * 0x2000 + (addr as usize - 0xA000);
        *self.ram.get(off).unwrap_or(&0xFF)
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_enable || self.ram.is_empty() {
            return;
        }
        let off = self.ram_bank() * 0x2000 + (addr as usize - 0xA000);
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

    /// Build a synthetic ROM with N 16-KiB banks, each filled with its bank number.
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
        let m = Mbc1::new(rom_with_n_banks(4), 0);
        assert_eq!(m.read_rom(0x0000), 0);
        assert_eq!(m.read_rom(0x4000), 1);
    }

    #[test]
    fn bank1_zero_maps_to_one() {
        let mut m = Mbc1::new(rom_with_n_banks(4), 0);
        m.write_rom(0x2000, 0x00);
        assert_eq!(m.read_rom(0x4000), 1);
    }

    #[test]
    fn switching_rom_banks_through_bank1() {
        let mut m = Mbc1::new(rom_with_n_banks(8), 0);
        m.write_rom(0x2000, 3);
        assert_eq!(m.read_rom(0x4000), 3);
        m.write_rom(0x2000, 7);
        assert_eq!(m.read_rom(0x4000), 7);
    }

    #[test]
    fn bank2_extends_rom_bank_address() {
        // 64-bank ROM (1 MiB). With bank2=1, bank1=1, expect bank 0x21 = 33.
        let mut m = Mbc1::new(rom_with_n_banks(64), 0);
        m.write_rom(0x2000, 1);
        m.write_rom(0x4000, 1);
        assert_eq!(m.read_rom(0x4000), 0x21);
    }

    #[test]
    fn mode1_remaps_low_region_to_upper_bank() {
        // 64-bank ROM, mode=1, bank2=1 → low region maps to bank 0x20 = 32.
        let mut m = Mbc1::new(rom_with_n_banks(64), 0);
        m.write_rom(0x4000, 1);
        m.write_rom(0x6000, 1);
        assert_eq!(m.read_rom(0x0000), 0x20);
    }

    #[test]
    fn ram_writes_only_when_enabled() {
        let mut m = Mbc1::new(rom_with_n_banks(2), 0x2000);
        m.write_ram(0xA000, 0x42);
        assert_eq!(m.read_ram(0xA000), 0xFF, "disabled RAM reads 0xFF");
        m.write_rom(0x0000, 0x0A); // enable
        m.write_ram(0xA000, 0x42);
        assert_eq!(m.read_ram(0xA000), 0x42);
        m.write_rom(0x0000, 0x00); // disable again
        assert_eq!(m.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn ram_banking_uses_bank2_in_mode1() {
        let mut m = Mbc1::new(rom_with_n_banks(2), 0x8000); // 32 KiB RAM = 4 banks
        m.write_rom(0x0000, 0x0A); // enable RAM
        m.write_rom(0x6000, 1); // mode 1
        m.write_rom(0x4000, 0); // bank2 = 0
        m.write_ram(0xA000, 0x11);
        m.write_rom(0x4000, 1); // bank2 = 1
        m.write_ram(0xA000, 0x22);
        m.write_rom(0x4000, 0);
        assert_eq!(m.read_ram(0xA000), 0x11);
        m.write_rom(0x4000, 1);
        assert_eq!(m.read_ram(0xA000), 0x22);
    }
}
