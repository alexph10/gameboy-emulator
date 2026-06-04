//! CPU register file.

use bitflags::bitflags;

bitflags! {
    /// Flags packed into the low nibble of register F.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Flags: u8 {
        const Z = 0b1000_0000; // Zero
        const N = 0b0100_0000; // Subtract (BCD)
        const H = 0b0010_0000; // Half-carry (BCD)
        const C = 0b0001_0000; // Carry
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Registers {
    pub a: u8,
    pub f: Flags,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl Registers {
    /// Values left in the registers when the DMG bootrom hands off control.
    /// Source: Pan Docs §Power Up Sequence.
    pub fn post_boot() -> Self {
        Self {
            a: 0x01,
            f: Flags::Z | Flags::H | Flags::C,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            sp: 0xFFFE,
            pc: 0x0100,
        }
    }

    #[inline] pub fn af(&self) -> u16 { ((self.a as u16) << 8) | (self.f.bits() as u16) }
    #[inline] pub fn bc(&self) -> u16 { ((self.b as u16) << 8) | self.c as u16 }
    #[inline] pub fn de(&self) -> u16 { ((self.d as u16) << 8) | self.e as u16 }
    #[inline] pub fn hl(&self) -> u16 { ((self.h as u16) << 8) | self.l as u16 }

    #[inline]
    pub fn set_af(&mut self, v: u16) {
        self.a = (v >> 8) as u8;
        // Low nibble of F is always zero on real hardware.
        self.f = Flags::from_bits_truncate((v as u8) & 0xF0);
    }
    #[inline] pub fn set_bc(&mut self, v: u16) { self.b = (v >> 8) as u8; self.c = v as u8; }
    #[inline] pub fn set_de(&mut self, v: u16) { self.d = (v >> 8) as u8; self.e = v as u8; }
    #[inline] pub fn set_hl(&mut self, v: u16) { self.h = (v >> 8) as u8; self.l = v as u8; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn af_low_nibble_is_zero() {
        let mut r = Registers::default();
        r.set_af(0xABCD);
        assert_eq!(r.af() & 0x000F, 0, "F low nibble must be zero");
    }
}
