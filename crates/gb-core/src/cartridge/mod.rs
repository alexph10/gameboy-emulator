//! Cartridge header parsing and MBC dispatch.
//! See `docs/mbc.md`.

mod header;
mod mbc_none;
mod mbc1;
mod mbc2;
mod mbc3;
mod mbc5;

pub use header::Header;

use crate::Error;

/// Uniform interface all memory bank controllers implement.
pub trait Mapper: std::fmt::Debug {
    fn read_rom(&self, addr: u16) -> u8;
    fn write_rom(&mut self, addr: u16, val: u8);
    fn read_ram(&self, addr: u16) -> u8;
    fn write_ram(&mut self, addr: u16, val: u8);
}

#[derive(Debug)]
pub struct Cartridge {
    pub header: Header,
    mapper: Box<dyn Mapper + Send>,
}

impl Cartridge {
    pub fn from_rom(rom: Vec<u8>) -> Result<Self, Error> {
        if rom.len() < 0x0150 {
            return Err(Error::InvalidRom("ROM too small to contain header"));
        }
        let header = Header::parse(&rom)?;
        let mapper: Box<dyn Mapper + Send> = match header.cart_type {
            0x00 => Box::new(mbc_none::MbcNone::new(rom)),
            0x01..=0x03 => Box::new(mbc1::Mbc1::new(rom, header.ram_size_bytes)),
            0x05 | 0x06 => Box::new(mbc2::Mbc2::new(rom)),
            0x0F..=0x13 => Box::new(mbc3::Mbc3::new(rom, header.ram_size_bytes)),
            0x19..=0x1E => Box::new(mbc5::Mbc5::new(rom, header.ram_size_bytes)),
            other => return Err(Error::UnsupportedCartridge(other)),
        };
        Ok(Self { header, mapper })
    }

    pub fn read_rom(&self, addr: u16) -> u8 { self.mapper.read_rom(addr) }
    pub fn write_rom(&mut self, addr: u16, val: u8) { self.mapper.write_rom(addr, val) }
    pub fn read_ram(&self, addr: u16) -> u8 { self.mapper.read_ram(addr) }
    pub fn write_ram(&mut self, addr: u16, val: u8) { self.mapper.write_ram(addr, val) }
}
