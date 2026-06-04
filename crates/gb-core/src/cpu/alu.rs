//! 8-bit ALU primitives.
//!
//! All helpers mutate the [`Flags`] reference per Pan Docs / GBCPUman.

use super::registers::Flags;

#[inline]
pub fn add(a: u8, b: u8, f: &mut Flags) -> u8 {
    let r = a.wrapping_add(b);
    f.set(Flags::Z, r == 0);
    f.remove(Flags::N);
    f.set(Flags::H, ((a & 0x0F) + (b & 0x0F)) > 0x0F);
    f.set(Flags::C, (a as u16 + b as u16) > 0xFF);
    r
}

#[inline]
pub fn adc(a: u8, b: u8, f: &mut Flags) -> u8 {
    let c = if f.contains(Flags::C) { 1 } else { 0 };
    let r = a.wrapping_add(b).wrapping_add(c);
    f.set(Flags::Z, r == 0);
    f.remove(Flags::N);
    f.set(Flags::H, ((a & 0x0F) + (b & 0x0F) + c) > 0x0F);
    f.set(Flags::C, (a as u16 + b as u16 + c as u16) > 0xFF);
    r
}

#[inline]
pub fn sub(a: u8, b: u8, f: &mut Flags) -> u8 {
    let r = a.wrapping_sub(b);
    f.set(Flags::Z, r == 0);
    f.insert(Flags::N);
    f.set(Flags::H, (a & 0x0F) < (b & 0x0F));
    f.set(Flags::C, a < b);
    r
}

#[inline]
pub fn sbc(a: u8, b: u8, f: &mut Flags) -> u8 {
    let c = if f.contains(Flags::C) { 1u8 } else { 0 };
    let r = a.wrapping_sub(b).wrapping_sub(c);
    f.set(Flags::Z, r == 0);
    f.insert(Flags::N);
    f.set(Flags::H, (a & 0x0F) < ((b & 0x0F) + c));
    f.set(Flags::C, (a as u16) < (b as u16 + c as u16));
    r
}

#[inline]
pub fn and(a: u8, b: u8, f: &mut Flags) -> u8 {
    let r = a & b;
    *f = Flags::H;
    if r == 0 {
        f.insert(Flags::Z);
    }
    r
}

#[inline]
pub fn or(a: u8, b: u8, f: &mut Flags) -> u8 {
    let r = a | b;
    *f = Flags::empty();
    if r == 0 {
        f.insert(Flags::Z);
    }
    r
}

#[inline]
pub fn xor(a: u8, b: u8, f: &mut Flags) -> u8 {
    let r = a ^ b;
    *f = Flags::empty();
    if r == 0 {
        f.insert(Flags::Z);
    }
    r
}

#[inline]
pub fn cp(a: u8, b: u8, f: &mut Flags) {
    let _ = sub(a, b, f);
}

#[inline]
pub fn inc(a: u8, f: &mut Flags) -> u8 {
    let r = a.wrapping_add(1);
    f.set(Flags::Z, r == 0);
    f.remove(Flags::N);
    f.set(Flags::H, (a & 0x0F) + 1 > 0x0F);
    // C preserved
    r
}

#[inline]
pub fn dec(a: u8, f: &mut Flags) -> u8 {
    let r = a.wrapping_sub(1);
    f.set(Flags::Z, r == 0);
    f.insert(Flags::N);
    f.set(Flags::H, (a & 0x0F) == 0);
    // C preserved
    r
}

/// `DAA` — adjust A after BCD add/sub. Modifies A and flags Z, H, C; N preserved.
#[inline]
pub fn daa(a: u8, f: &mut Flags) -> u8 {
    let mut a = a;
    let mut carry = f.contains(Flags::C);
    if !f.contains(Flags::N) {
        if f.contains(Flags::C) || a > 0x99 {
            a = a.wrapping_add(0x60);
            carry = true;
        }
        if f.contains(Flags::H) || (a & 0x0F) > 0x09 {
            a = a.wrapping_add(0x06);
        }
    } else {
        if f.contains(Flags::C) {
            a = a.wrapping_sub(0x60);
        }
        if f.contains(Flags::H) {
            a = a.wrapping_sub(0x06);
        }
    }
    f.set(Flags::Z, a == 0);
    f.remove(Flags::H);
    f.set(Flags::C, carry);
    a
}

// --- Rotates / shifts (CB-prefixed semantics; non-CB callers set Z=0) ---

#[inline]
pub fn rlc(v: u8, f: &mut Flags) -> u8 {
    let c = (v >> 7) & 1;
    let r = (v << 1) | c;
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    f.set(Flags::C, c != 0);
    r
}

#[inline]
pub fn rrc(v: u8, f: &mut Flags) -> u8 {
    let c = v & 1;
    let r = (v >> 1) | (c << 7);
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    f.set(Flags::C, c != 0);
    r
}

#[inline]
pub fn rl(v: u8, f: &mut Flags) -> u8 {
    let old_c = if f.contains(Flags::C) { 1 } else { 0 };
    let new_c = (v >> 7) & 1;
    let r = (v << 1) | old_c;
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    f.set(Flags::C, new_c != 0);
    r
}

#[inline]
pub fn rr(v: u8, f: &mut Flags) -> u8 {
    let old_c = if f.contains(Flags::C) { 1 } else { 0 };
    let new_c = v & 1;
    let r = (v >> 1) | (old_c << 7);
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    f.set(Flags::C, new_c != 0);
    r
}

#[inline]
pub fn sla(v: u8, f: &mut Flags) -> u8 {
    let c = (v >> 7) & 1;
    let r = v << 1;
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    f.set(Flags::C, c != 0);
    r
}

#[inline]
pub fn sra(v: u8, f: &mut Flags) -> u8 {
    let c = v & 1;
    let r = (v >> 1) | (v & 0x80);
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    f.set(Flags::C, c != 0);
    r
}

#[inline]
pub fn srl(v: u8, f: &mut Flags) -> u8 {
    let c = v & 1;
    let r = v >> 1;
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    f.set(Flags::C, c != 0);
    r
}

#[inline]
pub fn swap(v: u8, f: &mut Flags) -> u8 {
    let r = v.rotate_left(4);
    *f = Flags::empty();
    f.set(Flags::Z, r == 0);
    r
}

#[inline]
pub fn bit(n: u8, v: u8, f: &mut Flags) {
    let z = (v >> n) & 1 == 0;
    f.set(Flags::Z, z);
    f.remove(Flags::N);
    f.insert(Flags::H);
    // C preserved
}

#[inline]
pub fn res(n: u8, v: u8) -> u8 {
    v & !(1 << n)
}

#[inline]
pub fn set(n: u8, v: u8) -> u8 {
    v | (1 << n)
}

/// `ADD HL, rr` — N=0, H=carry from bit 11, C=carry from bit 15; Z preserved.
#[inline]
pub fn add16(hl: u16, rr: u16, f: &mut Flags) -> u16 {
    let r = hl.wrapping_add(rr);
    f.remove(Flags::N);
    f.set(Flags::H, ((hl & 0x0FFF) + (rr & 0x0FFF)) > 0x0FFF);
    f.set(Flags::C, (hl as u32 + rr as u32) > 0xFFFF);
    r
}

/// `ADD SP, r8` / `LD HL, SP+r8` — flags from **low byte** arithmetic.
/// Z=0, N=0, H = carry from bit 3, C = carry from bit 7 (8-bit add of SP_low + r8).
#[inline]
pub fn add_sp_i8(sp: u16, r8: i8, f: &mut Flags) -> u16 {
    let r8u = r8 as u8;
    let sp_lo = sp as u8;
    *f = Flags::empty();
    f.set(Flags::H, ((sp_lo & 0x0F) + (r8u & 0x0F)) > 0x0F);
    f.set(Flags::C, (sp_lo as u16 + r8u as u16) > 0xFF);
    sp.wrapping_add(r8 as i16 as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_basics() {
        let mut f = Flags::empty();
        assert_eq!(add(0x0F, 0x01, &mut f), 0x10);
        assert!(f.contains(Flags::H));
        assert!(!f.contains(Flags::C));
        assert_eq!(add(0xFF, 0x01, &mut f), 0x00);
        assert!(f.contains(Flags::Z) && f.contains(Flags::C) && f.contains(Flags::H));
    }

    #[test]
    fn daa_after_bcd_add() {
        // 0x45 + 0x38 = 0x7D → DAA = 0x83
        let mut f = Flags::empty();
        let s = add(0x45, 0x38, &mut f);
        let r = daa(s, &mut f);
        assert_eq!(r, 0x83);
        assert!(!f.contains(Flags::C));
    }

    #[test]
    fn daa_after_bcd_sub() {
        // 0x83 - 0x38 = 0x4B → DAA = 0x45
        let mut f = Flags::empty();
        let s = sub(0x83, 0x38, &mut f);
        let r = daa(s, &mut f);
        assert_eq!(r, 0x45);
    }

    #[test]
    fn add_sp_i8_flags() {
        let mut f = Flags::empty();
        let r = add_sp_i8(0x0008, 0x08, &mut f);
        assert_eq!(r, 0x0010);
        assert!(f.contains(Flags::H));
        assert!(!f.contains(Flags::C));
    }
}
