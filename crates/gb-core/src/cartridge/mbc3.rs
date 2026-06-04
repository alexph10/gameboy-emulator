//! MBC3 — up to 2 MiB ROM + 32 KiB RAM, optional RTC. See Pan Docs §MBC3.
//!
//! Address map:
//! * `0x0000–0x3FFF` — fixed ROM bank 0.
//! * `0x4000–0x7FFF` — switchable ROM bank 1–127 (writing 0 → 1).
//! * `0xA000–0xBFFF` — either an 8 KiB RAM bank (0x00–0x03) or one of the
//!   latched RTC registers (0x08–0x0C), depending on the value last written
//!   to `0x4000–0x5FFF`.
//!
//! Register writes (to the ROM region):
//! * `0x0000–0x1FFF` — RAM + RTC enable: low nibble == 0xA enables, else disables.
//! * `0x2000–0x3FFF` — ROM bank (7 bits); 0 → 1.
//! * `0x4000–0x5FFF` — RAM bank (0x00–0x03) **or** RTC register select (0x08–0x0C).
//! * `0x6000–0x7FFF` — Latch clock data: writing 0x00 then 0x01 latches the
//!   live RTC counter into the user-visible registers.
//!
//! The RTC counters advance with wall-clock time, sourced through an injected
//! `Fn() -> Duration` closure so tests can drive it deterministically.

use super::Mapper;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RTC_S: u8 = 0x08;
const RTC_M: u8 = 0x09;
const RTC_H: u8 = 0x0A;
const RTC_DL: u8 = 0x0B;
const RTC_DH: u8 = 0x0C;

const DH_DAY_HIGH: u8 = 0b0000_0001;
const DH_HALT: u8 = 0b0100_0000;
const DH_OVERFLOW: u8 = 0b1000_0000;

const SECS_PER_MIN: i64 = 60;
const SECS_PER_HOUR: i64 = 60 * 60;
const SECS_PER_DAY: i64 = 24 * 60 * 60;
const DAY_WRAP: i64 = 512;

pub struct Mbc3 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    rom_bank_count: usize,
    ram_bank_count: usize,

    ram_rtc_enable: bool,
    /// ROM bank for the 0x4000–0x7FFF window (7 bits; 0 stored as 1).
    rom_bank: u8,
    /// Last value written to 0x4000–0x5FFF.
    /// 0x00–0x03 selects a RAM bank; 0x08–0x0C selects an RTC register.
    ram_bank_or_rtc: u8,

    /// Wall-clock at which `base_counter_seconds` was sampled.
    base_time: Duration,
    /// RTC counter (in seconds since "day 0, 00:00:00") at `base_time`.
    base_counter_seconds: i64,
    halted: bool,
    /// Frozen counter value while halted.
    halt_seconds: i64,
    /// Sticky day-overflow latch (bit 7 of DH).
    day_overflow: bool,

    // User-visible latched RTC registers.
    latched_s: u8,
    latched_m: u8,
    latched_h: u8,
    latched_dl: u8,
    latched_dh: u8,
    /// Previous value written to 0x6000–0x7FFF — used to detect a 0→1 edge.
    latch_prev: u8,

    clock: Box<dyn Fn() -> Duration + Send>,
}

impl std::fmt::Debug for Mbc3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mbc3")
            .field("rom_bank_count", &self.rom_bank_count)
            .field("ram_bank_count", &self.ram_bank_count)
            .field("ram_rtc_enable", &self.ram_rtc_enable)
            .field("rom_bank", &self.rom_bank)
            .field("ram_bank_or_rtc", &self.ram_bank_or_rtc)
            .field("halted", &self.halted)
            .field("day_overflow", &self.day_overflow)
            .finish()
    }
}

impl Mbc3 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        Self::new_with_clock(rom, ram_size, || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
        })
    }

    pub fn new_with_clock<F>(rom: Vec<u8>, ram_size: usize, clock: F) -> Self
    where
        F: Fn() -> Duration + Send + 'static,
    {
        let rom_bank_count = (rom.len() / 0x4000).max(1);
        let ram_bank_count = ram_size / 0x2000;
        let now = clock();
        Self {
            rom,
            ram: vec![0; ram_size],
            rom_bank_count,
            ram_bank_count,
            ram_rtc_enable: false,
            rom_bank: 1,
            ram_bank_or_rtc: 0,
            base_time: now,
            base_counter_seconds: 0,
            halted: false,
            halt_seconds: 0,
            day_overflow: false,
            latched_s: 0,
            latched_m: 0,
            latched_h: 0,
            latched_dl: 0,
            latched_dh: 0,
            latch_prev: 0xFF,
            clock: Box::new(clock),
        }
    }

    fn now(&self) -> Duration {
        (self.clock)()
    }

    /// Live total RTC seconds (since "day 0").
    fn current_total_seconds(&self) -> i64 {
        if self.halted {
            self.halt_seconds
        } else {
            let now = self.now();
            let elapsed = now.saturating_sub(self.base_time).as_secs() as i64;
            self.base_counter_seconds.saturating_add(elapsed)
        }
    }

    /// Re-base the live counter to `total`, anchored at the current wall-clock.
    fn rebase_counter(&mut self, total: i64) {
        if self.halted {
            self.halt_seconds = total;
        } else {
            self.base_time = self.now();
            self.base_counter_seconds = total;
        }
    }

    /// Copy the current RTC counter into the user-visible latched registers.
    fn latch_now(&mut self) {
        let mut total = self.current_total_seconds();
        let mut days = total.div_euclid(SECS_PER_DAY);

        // Sticky day overflow: once we exceed the 9-bit day range, set the
        // flag and fold the days down into [0, 512) so we don't keep growing.
        if days >= DAY_WRAP {
            self.day_overflow = true;
            let wraps = days.div_euclid(DAY_WRAP);
            let adj_secs = wraps * DAY_WRAP * SECS_PER_DAY;
            days -= wraps * DAY_WRAP;
            total -= adj_secs;
            if self.halted {
                self.halt_seconds -= adj_secs;
            } else {
                self.base_counter_seconds -= adj_secs;
            }
        }

        let secs = total.rem_euclid(SECS_PER_MIN) as u8;
        let mins = total.div_euclid(SECS_PER_MIN).rem_euclid(60) as u8;
        let hours = total.div_euclid(SECS_PER_HOUR).rem_euclid(24) as u8;

        self.latched_s = secs;
        self.latched_m = mins;
        self.latched_h = hours;
        self.latched_dl = (days & 0xFF) as u8;
        let day_high = ((days >> 8) & 1) as u8;
        self.latched_dh = (day_high & DH_DAY_HIGH)
            | if self.halted { DH_HALT } else { 0 }
            | if self.day_overflow { DH_OVERFLOW } else { 0 };
    }

    /// CPU wrote `val` to the RTC register currently selected via 0x4000–0x5FFF.
    /// Updates the *live* counter (not the latched copy).
    fn write_rtc_register(&mut self, reg: u8, val: u8) {
        let total = self.current_total_seconds();
        let mut secs = total.rem_euclid(SECS_PER_MIN);
        let mut mins = total.div_euclid(SECS_PER_MIN).rem_euclid(60);
        let mut hours = total.div_euclid(SECS_PER_HOUR).rem_euclid(24);
        let mut days = total.div_euclid(SECS_PER_DAY);
        // Normalize days into the 9-bit window before edits.
        let day_high = (days >> 8) & 1;
        let day_low = days & 0xFF;
        days = (day_high << 8) | day_low;

        match reg {
            RTC_S => secs = (val & 0x3F) as i64,
            RTC_M => mins = (val & 0x3F) as i64,
            RTC_H => hours = (val & 0x1F) as i64,
            RTC_DL => days = (days & 0x100) | (val as i64),
            RTC_DH => {
                let new_day_high = (val & DH_DAY_HIGH) as i64;
                let new_halt = (val & DH_HALT) != 0;
                let new_overflow = (val & DH_OVERFLOW) != 0;
                days = (new_day_high << 8) | (days & 0xFF);
                // Halt transition: capture/release the counter snapshot.
                if new_halt && !self.halted {
                    self.halt_seconds = total;
                } else if !new_halt && self.halted {
                    // Resume: re-anchor at "now" so resumed seconds count
                    // from the captured value forward.
                    self.base_time = self.now();
                    self.base_counter_seconds = self.halt_seconds;
                }
                self.halted = new_halt;
                self.day_overflow = new_overflow;
            }
            _ => return,
        }

        let new_total = days * SECS_PER_DAY + hours * SECS_PER_HOUR + mins * SECS_PER_MIN + secs;
        self.rebase_counter(new_total);
    }
}

impl Mapper for Mbc3 {
    fn read_rom(&self, addr: u16) -> u8 {
        let bank = if addr < 0x4000 {
            0
        } else {
            (self.rom_bank as usize) & (self.rom_bank_count - 1)
        };
        let off = bank * 0x4000 + (addr as usize & 0x3FFF);
        *self.rom.get(off).unwrap_or(&0xFF)
    }

    fn write_rom(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_rtc_enable = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                let v = val & 0x7F;
                self.rom_bank = if v == 0 { 1 } else { v };
            }
            0x4000..=0x5FFF => {
                self.ram_bank_or_rtc = val;
            }
            0x6000..=0x7FFF => {
                if self.latch_prev == 0x00 && val == 0x01 {
                    self.latch_now();
                }
                self.latch_prev = val;
            }
            _ => {}
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_rtc_enable {
            return 0xFF;
        }
        match self.ram_bank_or_rtc {
            0x00..=0x03 => {
                if self.ram.is_empty() || self.ram_bank_count == 0 {
                    return 0xFF;
                }
                let bank = (self.ram_bank_or_rtc as usize) & (self.ram_bank_count - 1).max(0);
                let off = bank * 0x2000 + (addr as usize - 0xA000);
                *self.ram.get(off).unwrap_or(&0xFF)
            }
            RTC_S => self.latched_s,
            RTC_M => self.latched_m,
            RTC_H => self.latched_h,
            RTC_DL => self.latched_dl,
            RTC_DH => self.latched_dh,
            _ => 0xFF,
        }
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_rtc_enable {
            return;
        }
        match self.ram_bank_or_rtc {
            0x00..=0x03 => {
                if self.ram.is_empty() || self.ram_bank_count == 0 {
                    return;
                }
                let bank = (self.ram_bank_or_rtc as usize) & (self.ram_bank_count - 1).max(0);
                let off = bank * 0x2000 + (addr as usize - 0xA000);
                if let Some(slot) = self.ram.get_mut(off) {
                    *slot = val;
                }
            }
            reg @ (RTC_S | RTC_M | RTC_H | RTC_DL | RTC_DH) => {
                self.write_rtc_register(reg, val);
            }
            _ => {}
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
    use std::sync::{Arc, Mutex};

    fn rom_with_n_banks(n: usize) -> Vec<u8> {
        let mut rom = vec![0u8; n * 0x4000];
        for bank in 0..n {
            for i in 0..0x4000 {
                rom[bank * 0x4000 + i] = bank as u8;
            }
        }
        rom
    }

    /// A test-only clock backed by a shared `Duration` cell.
    #[derive(Clone)]
    struct TestClock(Arc<Mutex<Duration>>);
    impl TestClock {
        fn new(start: Duration) -> Self {
            Self(Arc::new(Mutex::new(start)))
        }
        fn advance(&self, by: Duration) {
            let mut g = self.0.lock().unwrap();
            *g += by;
        }
        fn fn_clone(&self) -> impl Fn() -> Duration + Send + 'static {
            let inner = self.0.clone();
            move || *inner.lock().unwrap()
        }
    }

    fn make(rom_banks: usize, ram_size: usize) -> (Mbc3, TestClock) {
        let clk = TestClock::new(Duration::from_secs(1_000_000));
        let m = Mbc3::new_with_clock(rom_with_n_banks(rom_banks), ram_size, clk.fn_clone());
        (m, clk)
    }

    #[test]
    fn defaults_select_bank_0_and_bank_1() {
        let (m, _) = make(4, 0);
        assert_eq!(m.read_rom(0x0000), 0);
        assert_eq!(m.read_rom(0x4000), 1);
    }

    #[test]
    fn bank_number_zero_maps_to_one() {
        let (mut m, _) = make(4, 0);
        m.write_rom(0x2000, 0x00);
        assert_eq!(m.read_rom(0x4000), 1);
    }

    #[test]
    fn switching_rom_banks_up_to_127() {
        let (mut m, _) = make(128, 0);
        for bank in [1u8, 2, 5, 33, 64, 127] {
            m.write_rom(0x2000, bank);
            assert_eq!(m.read_rom(0x4000), bank, "bank {bank}");
            assert_eq!(m.read_rom(0x7FFF), bank, "bank {bank} (top)");
        }
        // Top bit of a write to 0x2000 is masked (MBC3 = 7 bits).
        m.write_rom(0x2000, 0x80);
        assert_eq!(m.read_rom(0x4000), 1, "0x80 → low 7 bits = 0 → remap to 1");
    }

    #[test]
    fn ram_writes_only_when_enabled() {
        let (mut m, _) = make(2, 0x2000);
        m.write_ram(0xA000, 0x42);
        assert_eq!(m.read_ram(0xA000), 0xFF);
        m.write_rom(0x0000, 0x0A); // enable
        m.write_ram(0xA000, 0x42);
        assert_eq!(m.read_ram(0xA000), 0x42);
        m.write_rom(0x0000, 0x00); // disable
        assert_eq!(m.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn ram_banking_4_banks() {
        let (mut m, _) = make(2, 0x8000); // 32 KiB = 4 banks
        m.write_rom(0x0000, 0x0A);
        for b in 0u8..4 {
            m.write_rom(0x4000, b);
            m.write_ram(0xA000, 0x10 + b);
        }
        for b in 0u8..4 {
            m.write_rom(0x4000, b);
            assert_eq!(m.read_ram(0xA000), 0x10 + b, "bank {b}");
        }
    }

    #[test]
    fn rtc_register_select_overlays_ram_region() {
        let (mut m, _) = make(2, 0x2000);
        m.write_rom(0x0000, 0x0A); // enable
        // Write a known M value into the live counter, then latch.
        m.write_rom(0x4000, RTC_M);
        m.write_ram(0xA000, 42);
        // Latch the live counter into the readable registers.
        m.write_rom(0x6000, 0x00);
        m.write_rom(0x6000, 0x01);
        // Reading 0xA000 with RTC_M selected returns the latched M.
        assert_eq!(m.read_ram(0xA000), 42);
        // Selecting S returns S, not M.
        m.write_rom(0x4000, RTC_S);
        assert_ne!(m.read_ram(0xA000), 42);
    }

    #[test]
    fn rtc_latches_on_0_then_1_write_to_6000() {
        let (mut m, clk) = make(2, 0x2000);
        m.write_rom(0x0000, 0x0A);
        // Latch initial value (S = 0).
        m.write_rom(0x6000, 0x00);
        m.write_rom(0x6000, 0x01);
        m.write_rom(0x4000, RTC_S);
        let s0 = m.read_ram(0xA000);
        assert_eq!(s0, 0);
        // Advance wall clock; latched value should NOT change.
        clk.advance(Duration::from_secs(5));
        assert_eq!(m.read_ram(0xA000), 0, "no re-latch yet");
        // 0 → 1 edge re-latches.
        m.write_rom(0x6000, 0x00);
        m.write_rom(0x6000, 0x01);
        assert_eq!(m.read_ram(0xA000), 5);
        // A write of 1 without an intervening 0 does NOT re-latch.
        clk.advance(Duration::from_secs(7));
        m.write_rom(0x6000, 0x01);
        assert_eq!(m.read_ram(0xA000), 5);
    }

    #[test]
    fn rtc_halt_stops_counter() {
        let (mut m, clk) = make(2, 0x2000);
        m.write_rom(0x0000, 0x0A);
        // Run for 70 seconds; latch.
        clk.advance(Duration::from_secs(70));
        m.write_rom(0x6000, 0x00);
        m.write_rom(0x6000, 0x01);
        m.write_rom(0x4000, RTC_S);
        assert_eq!(m.read_ram(0xA000), 10);
        m.write_rom(0x4000, RTC_M);
        assert_eq!(m.read_ram(0xA000), 1);

        // Halt the RTC.
        m.write_rom(0x4000, RTC_DH);
        m.write_ram(0xA000, DH_HALT);
        // Re-latch immediately so the user-visible registers see "halt instant".
        m.write_rom(0x6000, 0x00);
        m.write_rom(0x6000, 0x01);
        m.write_rom(0x4000, RTC_S);
        let s_at_halt = m.read_ram(0xA000);

        // Advance a lot of wall time.
        clk.advance(Duration::from_secs(10_000));
        m.write_rom(0x6000, 0x00);
        m.write_rom(0x6000, 0x01);

        m.write_rom(0x4000, RTC_S);
        assert_eq!(m.read_ram(0xA000), s_at_halt, "S frozen while halted");
        m.write_rom(0x4000, RTC_M);
        assert_eq!(m.read_ram(0xA000), 1, "M frozen while halted");
        m.write_rom(0x4000, RTC_H);
        assert_eq!(m.read_ram(0xA000), 0, "H frozen while halted");
        m.write_rom(0x4000, RTC_DL);
        assert_eq!(m.read_ram(0xA000), 0, "DL frozen while halted");
        m.write_rom(0x4000, RTC_DH);
        assert_eq!(m.read_ram(0xA000) & DH_HALT, DH_HALT, "halt bit visible");
    }

    // Bonus: ensure the production (SystemTime-backed) constructor builds.
    #[test]
    fn production_constructor_compiles() {
        let _ = Mbc3::new(rom_with_n_banks(1), 0);
    }
}
