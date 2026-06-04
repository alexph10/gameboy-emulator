//! DIV / TIMA / TMA / TAC. See `docs/timers.md`.
//!
//! Implementation follows the classic "internal 16-bit counter with edge
//! detection" model documented in TCAGBD §2.1 and the Pan Docs Timer section.
//!
//! * `DIV` (`FF04`) is the upper 8 bits of `counter`. Writing any value to
//!   `DIV` resets the *entire* counter to zero (this can also cause spurious
//!   TIMA ticks via the falling-edge detector — handled below).
//! * `TIMA` (`FF05`) ticks on the **falling edge** of `(counter_bit AND tac_enable)`
//!   where `counter_bit` is selected by `TAC[1:0]`:
//!   00 → bit 9  (4096 Hz),
//!   01 → bit 3  (262144 Hz),
//!   10 → bit 5  (65536 Hz),
//!   11 → bit 7  (16384 Hz).
//! * On `TIMA` overflow there is a 4-T-cycle "reload window" where `TIMA`
//!   reads as 0; after that it's reloaded from `TMA` and the timer interrupt
//!   is requested.

use crate::interrupts::{IntFlags, Interrupts};

#[derive(Debug, Default)]
pub struct Timer {
    /// Internal 16-bit counter; DIV is its upper byte.
    pub counter: u16,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
    /// True for the 4 T-cycle window after a TIMA overflow, before TMA reload.
    overflow_pending: u8,
    /// Cached "edge-detector" bit value from last tick — used to detect
    /// falling edges across tick boundaries.
    last_and_result: bool,
}

impl Timer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self, t_cycles: u32, ints: &mut Interrupts) {
        for _ in 0..t_cycles {
            self.tick_one(ints);
        }
    }

    fn tick_one(&mut self, ints: &mut Interrupts) {
        // 1. Resolve pending overflow exactly 4 T-cycles after it happened.
        if self.overflow_pending > 0 {
            self.overflow_pending -= 1;
            if self.overflow_pending == 0 {
                self.tima = self.tma;
                ints.request(IntFlags::TIMER);
            }
        }

        // 2. Advance the internal counter (DIV is its upper byte).
        self.counter = self.counter.wrapping_add(1);

        // 3. Falling-edge detector: if (selected_bit & enable) goes 1→0, TIMA ticks.
        let new_and = self.and_result();
        if self.last_and_result && !new_and {
            self.increment_tima();
        }
        self.last_and_result = new_and;
    }

    fn increment_tima(&mut self) {
        let (next, overflow) = self.tima.overflowing_add(1);
        self.tima = next;
        if overflow {
            // TIMA reads as 0 for 4 T-cycles, then reloads from TMA + sets IRQ.
            self.tima = 0;
            self.overflow_pending = 4;
        }
    }

    fn and_result(&self) -> bool {
        let bit = match self.tac & 0b11 {
            0b00 => 9,
            0b01 => 3,
            0b10 => 5,
            _ => 7,
        };
        let enabled = (self.tac & 0b100) != 0;
        enabled && (self.counter & (1 << bit)) != 0
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
            0xFF04 => {
                // Writing DIV resets the whole internal counter. The reset
                // can itself produce a falling edge that ticks TIMA.
                self.counter = 0;
                let new_and = self.and_result();
                if self.last_and_result && !new_and {
                    self.increment_tima();
                }
                self.last_and_result = new_and;
            }
            0xFF05 => self.tima = val,
            0xFF06 => self.tma = val,
            0xFF07 => {
                self.tac = val & 0x07;
                // Changing TAC can also cause a glitch falling edge.
                let new_and = self.and_result();
                if self.last_and_result && !new_and {
                    self.increment_tima();
                }
                self.last_and_result = new_and;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (Timer, Interrupts) {
        (Timer::new(), Interrupts::new())
    }

    #[test]
    fn div_increments_every_256_t_cycles() {
        let (mut t, mut i) = fresh();
        t.tick(256, &mut i);
        assert_eq!(t.read(0xFF04), 1);
        t.tick(256 * 254, &mut i);
        assert_eq!(t.read(0xFF04), 255);
        t.tick(256, &mut i);
        assert_eq!(t.read(0xFF04), 0); // wraps
    }

    #[test]
    fn writing_div_zeros_counter() {
        let (mut t, mut i) = fresh();
        t.tick(10_000, &mut i);
        assert_ne!(t.read(0xFF04), 0);
        t.write(0xFF04, 0xAB);
        assert_eq!(t.read(0xFF04), 0);
    }

    #[test]
    fn tima_clock_select_00_is_4096_hz() {
        // TAC=0b100 → enable, bit 9, 1024 T-cycles per increment.
        let (mut t, mut i) = fresh();
        t.write(0xFF07, 0b100);
        t.tick(1024, &mut i);
        assert_eq!(t.tima, 1);
        t.tick(1024 * 3, &mut i);
        assert_eq!(t.tima, 4);
    }

    #[test]
    fn tima_clock_select_01_is_262144_hz() {
        // TAC=0b101 → enable, bit 3, 16 T-cycles per increment.
        let (mut t, mut i) = fresh();
        t.write(0xFF07, 0b101);
        t.tick(16, &mut i);
        assert_eq!(t.tima, 1);
        t.tick(16 * 9, &mut i);
        assert_eq!(t.tima, 10);
    }

    #[test]
    fn tima_overflow_reloads_tma_and_fires_irq_after_delay() {
        let (mut t, mut i) = fresh();
        t.write(0xFF06, 0x42); // TMA
        t.write(0xFF05, 0xFF); // TIMA on the brink
        t.write(0xFF07, 0b101); // enable, bit 3
        t.tick(16, &mut i); // one TIMA increment → overflow
        assert_eq!(t.tima, 0, "reads as 0 during reload window");
        assert!(!i.pending().contains(IntFlags::TIMER));
        t.tick(4, &mut i); // close the reload window
        assert_eq!(t.tima, 0x42);
        assert!(i.iflag.contains(IntFlags::TIMER));
    }

    #[test]
    fn timer_disabled_does_not_tick_tima() {
        let (mut t, mut i) = fresh();
        t.write(0xFF07, 0b000); // disabled
        t.tick(100_000, &mut i);
        assert_eq!(t.tima, 0);
    }
}
