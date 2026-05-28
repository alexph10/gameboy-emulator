//! DIV / TIMA / TMA / TAC. See `docs/timers.md`.

use crate::interrupts::{IntFlags, Interrupts};

#[derive(Debug, Default)]
pub struct Timer {
    /// Internal 16-bit counter; DIV is its upper byte.
    pub counter: u16,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
}

impl Timer {
    pub fn new() -> Self { Self::default() }

    pub fn tick(&mut self, _t_cycles: u32, _ints: &mut Interrupts) {
        // TODO: increment internal counter, detect falling edge of
        // (counter_bit AND tac_enable), handle TIMA overflow + reload window.
        let _ = IntFlags::TIMER;
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.counter >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => 0xF8 | self.tac,
            _ => 0xFF,
        }
    }
    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF04 => self.counter = 0,
            0xFF05 => self.tima = val,
            0xFF06 => self.tma = val,
            0xFF07 => self.tac = val & 0x07,
            _ => {}
        }
    }
}
