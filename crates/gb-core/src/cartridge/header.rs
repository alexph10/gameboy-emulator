//! Parses the 0x0100..0x0150 cartridge header.

use crate::Error;

#[derive(Debug, Clone)]
pub struct Header {
    pub title: String,
    pub cart_type: u8,
    pub rom_size_bytes: usize,
    pub ram_size_bytes: usize,
    pub cgb_flag: u8,
    pub sgb_flag: u8,
    pub header_checksum_ok: bool,
}

impl Header {
    pub fn parse(rom: &[u8]) -> Result<Self, Error> {
        if rom.len() < 0x0150 {
            return Err(Error::InvalidRom("ROM too small for header"));
        }
        let title_bytes = &rom[0x0134..0x0144];
        let title = title_bytes
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();

        let cart_type = rom[0x0147];
        let rom_size_bytes = (32 * 1024usize) << rom[0x0148] as usize;
        let ram_size_bytes = match rom[0x0149] {
            0x00 => 0,
            0x01 => 2 * 1024,    // 2 KiB (unused on official carts but valid)
            0x02 => 8 * 1024,
            0x03 => 32 * 1024,
            0x04 => 128 * 1024,
            0x05 => 64 * 1024,
            _    => 0,
        };

        // Header checksum: x = 0; for i in 0x134..=0x14C: x = x - rom[i] - 1
        let checksum = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        let header_checksum_ok = checksum == rom[0x014D];

        Ok(Self {
            title,
            cart_type,
            rom_size_bytes,
            ram_size_bytes,
            cgb_flag: rom[0x0143],
            sgb_flag: rom[0x0146],
            header_checksum_ok,
        })
    }

    /// True if the cartridge header's CGB flag (`0x0143`) requests CGB hardware
    /// features. `0x80` = "CGB enhanced, DMG compatible", `0xC0` = "CGB only".
    /// All other values mean DMG-only.
    pub fn is_cgb(&self) -> bool {
        matches!(self.cgb_flag, 0x80 | 0xC0)
    }

    /// True for cart types that include battery-backed external RAM (or RTC),
    /// per the cartridge-type table in Pan Docs §The Cartridge Header.
    pub fn has_battery(&self) -> bool {
        matches!(
            self.cart_type,
            0x03 // MBC1+RAM+BATTERY
            | 0x06 // MBC2+BATTERY
            | 0x09 // ROM+RAM+BATTERY
            | 0x0D // MMM01+RAM+BATTERY
            | 0x0F // MBC3+TIMER+BATTERY
            | 0x10 // MBC3+TIMER+RAM+BATTERY
            | 0x13 // MBC3+RAM+BATTERY
            | 0x1B // MBC5+RAM+BATTERY
            | 0x1E // MBC5+RUMBLE+RAM+BATTERY
            | 0x22 // MBC7+SENSOR+RUMBLE+RAM+BATTERY
            | 0xFF // HuC1+RAM+BATTERY
        )
    }
}
