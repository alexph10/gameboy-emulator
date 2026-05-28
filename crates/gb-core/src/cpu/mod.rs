//! Sharp LR35902 / SM83 CPU core.
//!
//! See `docs/cpu.md`.

pub mod registers;

use crate::bus::Bus;
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
}

impl Cpu {
    /// CPU state after the DMG boot ROM has finished (skips boot).
    pub fn post_boot() -> Self {
        Self {
            regs: Registers::post_boot(),
            ime: false,
            ime_pending: false,
            halted: false,
        }
    }

    /// Execute the next instruction. Returns elapsed **M-cycles**.
    pub fn step(&mut self, _bus: &mut Bus) -> u32 {
        // TODO: fetch / decode / execute.
        // For now we advance one M-cycle so the rest of the system ticks.
        if self.ime_pending {
            self.ime = true;
            self.ime_pending = false;
        }
        1
    }
}
