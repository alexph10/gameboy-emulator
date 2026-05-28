//! Interrupt controller — IF (`FF0F`) and IE (`FFFF`). See `docs/interrupts.md`.

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct IntFlags: u8 {
        const VBLANK = 1 << 0;
        const STAT   = 1 << 1;
        const TIMER  = 1 << 2;
        const SERIAL = 1 << 3;
        const JOYPAD = 1 << 4;
    }
}

impl IntFlags {
    pub const fn vector(self) -> u16 {
        // Lowest-bit-set wins.
        match self.bits().trailing_zeros() {
            0 => 0x0040, // VBlank
            1 => 0x0048, // STAT
            2 => 0x0050, // Timer
            3 => 0x0058, // Serial
            4 => 0x0060, // Joypad
            _ => 0x0000,
        }
    }
}

#[derive(Debug, Default)]
pub struct Interrupts {
    pub ie: IntFlags,
    pub iflag: IntFlags,
}

impl Interrupts {
    pub fn new() -> Self { Self::default() }

    pub fn request(&mut self, src: IntFlags) {
        self.iflag |= src;
    }

    pub fn pending(&self) -> IntFlags {
        self.ie & self.iflag
    }

    pub fn read_if(&self) -> u8 { 0xE0 | self.iflag.bits() }
    pub fn write_if(&mut self, v: u8) { self.iflag = IntFlags::from_bits_truncate(v); }
    pub fn read_ie(&self) -> u8 { self.ie.bits() }
    pub fn write_ie(&mut self, v: u8) { self.ie = IntFlags::from_bits_truncate(v); }
}
