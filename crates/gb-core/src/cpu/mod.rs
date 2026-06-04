//! Sharp LR35902 / SM83 CPU core.
//!
//! See `docs/cpu.md`.

pub mod alu;
mod exec;
pub mod registers;

use crate::bus::Bus;
use crate::interrupts::IntFlags;
use registers::Registers;

#[derive(Debug)]
pub struct Cpu {
    pub regs: Registers,
    /// Interrupt Master Enable.
    pub ime: bool,
    /// Set by `EI`; IME becomes true *after* the next instruction.
    pub ime_pending: bool,
    /// `HALT` state — CPU stalls until an interrupt is pending.
    pub halted: bool,
    /// HALT bug: when HALT is executed with IME=0 and IF&IE != 0, the byte
    /// after HALT is read twice (PC doesn't advance for the next opcode fetch).
    pub halt_bug: bool,
}

impl Cpu {
    /// CPU state after the DMG boot ROM has finished (skips boot).
    pub fn post_boot() -> Self {
        Self {
            regs: Registers::post_boot(),
            ime: false,
            ime_pending: false,
            halted: false,
            halt_bug: false,
        }
    }

    /// Execute the next instruction (or service an interrupt). Returns
    /// elapsed **M-cycles**.
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        // Resolve any pending EI from the *previous* instruction by snapshotting
        // it now; if the upcoming instruction is EI, it will re-set the flag.
        let activate_ime = self.ime_pending;

        // Interrupt servicing — runs before instruction fetch.
        if self.ime && !bus.pending_interrupts().is_empty() {
            self.halted = false;
            return self.service_interrupt(bus);
        }

        // HALT wakeup: any pending interrupt (regardless of IME) breaks HALT.
        if self.halted {
            if !bus.pending_interrupts().is_empty() {
                self.halted = false;
                // Fall through and execute the next instruction.
            } else {
                // Sleep one M-cycle.
                return 1;
            }
        }

        let cycles = exec::step(self, bus);

        if activate_ime {
            self.ime = true;
            self.ime_pending = false;
        }

        cycles
    }

    // ---- Memory access helpers (CPU side; bus tick is external) ----

    #[inline]
    pub(crate) fn fetch8(&mut self, bus: &mut Bus) -> u8 {
        let v = bus.read(self.regs.pc);
        if self.halt_bug {
            // Halt bug: do not advance PC for this one opcode fetch.
            self.halt_bug = false;
        } else {
            self.regs.pc = self.regs.pc.wrapping_add(1);
        }
        v
    }

    #[inline]
    pub(crate) fn fetch16(&mut self, bus: &mut Bus) -> u16 {
        let lo = self.fetch8(bus) as u16;
        let hi = self.fetch8(bus) as u16;
        (hi << 8) | lo
    }

    #[inline]
    pub(crate) fn push16(&mut self, bus: &mut Bus, v: u16) {
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        bus.write(self.regs.sp, (v >> 8) as u8);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        bus.write(self.regs.sp, v as u8);
    }

    #[inline]
    pub(crate) fn pop16(&mut self, bus: &mut Bus) -> u16 {
        let lo = bus.read(self.regs.sp) as u16;
        self.regs.sp = self.regs.sp.wrapping_add(1);
        let hi = bus.read(self.regs.sp) as u16;
        self.regs.sp = self.regs.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    fn service_interrupt(&mut self, bus: &mut Bus) -> u32 {
        let pending = bus.pending_interrupts();
        // Lowest-bit-set wins.
        let bit = pending.bits().trailing_zeros();
        let flag = IntFlags::from_bits_truncate(1u8 << bit);
        let vector = flag.vector();

        self.ime = false;
        bus.clear_if(flag);
        self.push16(bus, self.regs.pc);
        self.regs.pc = vector;
        5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;
    use registers::Flags;

    fn make_bus(rom_at_100: &[u8]) -> Bus {
        // Build a minimal 32 KiB ROM with header bytes patched at 0x0150.
        let mut rom = vec![0u8; 0x8000];
        // Nintendo logo at 0x0104..0x0134 zeroed is fine for Header::parse if we
        // bypass header validation — but Cartridge::from_rom calls Header::parse.
        // We instead skip the cart and place a custom Bus, but Bus needs a
        // Cartridge. Use the simplest valid MBC0 header.
        rom[0x0147] = 0x00; // ROM ONLY
        // Header checksum (0x014D) — Cartridge::from_rom doesn't validate, so
        // any value is OK as long as parsing doesn't error.
        for (i, b) in rom_at_100.iter().enumerate() {
            rom[0x0100 + i] = *b;
        }
        let cart = Cartridge::from_rom(rom).expect("rom");
        Bus::new(cart, None)
    }

    #[test]
    fn ld_a_imm_and_add() {
        // 0x3E 0x05  LD A, 0x05
        // 0xC6 0x03  ADD A, 0x03
        // 0x76       HALT
        let mut bus = make_bus(&[0x3E, 0x05, 0xC6, 0x03, 0x76]);
        let mut cpu = Cpu::post_boot();
        cpu.regs.pc = 0x0100;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.a, 0x05);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.a, 0x08);
    }

    #[test]
    fn jp_nn() {
        // 0xC3 0x34 0x12  JP 0x1234
        let mut bus = make_bus(&[0xC3, 0x34, 0x12]);
        let mut cpu = Cpu::post_boot();
        cpu.regs.pc = 0x0100;
        let c = cpu.step(&mut bus);
        assert_eq!(c, 4);
        assert_eq!(cpu.regs.pc, 0x1234);
    }

    #[test]
    fn call_then_ret() {
        // 0xCD 0x05 0x01   CALL 0x0105    (at 0x0100)
        // 0x00 0x00         filler
        // 0xC9              RET            (at 0x0105)
        let mut bus = make_bus(&[0xCD, 0x05, 0x01, 0x00, 0x00, 0xC9]);
        let mut cpu = Cpu::post_boot();
        cpu.regs.pc = 0x0100;
        cpu.regs.sp = 0xFFFE;
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.pc, 0x0105);
        assert_eq!(cpu.regs.sp, 0xFFFC);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.pc, 0x0103);
        assert_eq!(cpu.regs.sp, 0xFFFE);
    }

    #[test]
    fn push_pop_af_low_nibble_zero() {
        // 0x01 0xCD 0xAB   LD BC, 0xABCD
        // 0xC5             PUSH BC
        // 0xF1             POP AF
        let mut bus = make_bus(&[0x01, 0xCD, 0xAB, 0xC5, 0xF1]);
        let mut cpu = Cpu::post_boot();
        cpu.regs.pc = 0x0100;
        cpu.regs.sp = 0xFFFE;
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.a, 0xAB);
        assert_eq!(cpu.regs.f.bits(), 0xCD & 0xF0);
    }

    #[test]
    fn ld_r_r_basic() {
        // 0x06 0x42  LD B, 0x42
        // 0x78       LD A, B
        let mut bus = make_bus(&[0x06, 0x42, 0x78]);
        let mut cpu = Cpu::post_boot();
        cpu.regs.pc = 0x0100;
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.a, 0x42);
    }

    #[test]
    fn daa_smoke() {
        // 0x3E 0x45  LD A, 0x45
        // 0xC6 0x38  ADD A, 0x38
        // 0x27       DAA
        let mut bus = make_bus(&[0x3E, 0x45, 0xC6, 0x38, 0x27]);
        let mut cpu = Cpu::post_boot();
        cpu.regs.pc = 0x0100;
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.a, 0x83);
        assert!(!cpu.regs.f.contains(Flags::C));
    }
}
