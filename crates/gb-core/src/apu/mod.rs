//! Audio Processing Unit — DMG 4-channel sound chip.
//!
//! Implements the four channels (square+sweep, square, wave, noise), the
//! 512 Hz frame sequencer, the master mixer (NR50/NR51/NR52), and a fractional
//! down-sampler that emits stereo `i16` samples at a host-friendly rate.
//!
//! **Correctness target.** The implementation aims to pass Blargg
//! `dmg_sound/01-registers` — which exercises register read-back masks, the
//! initial post-boot register values, and DMG power-off semantics — while
//! producing clean audible output through the SDL frontend. It is not
//! cycle-perfect; many of the more subtle dmg_sound sub-tests (sweep details,
//! trigger overflow, wave-RAM access while running, etc.) are not targeted.
//!
//! References: Pan Docs §Audio, Gekkio gb-ctr §APU, Blargg dmg_sound source.

use core::fmt;

/// Host output sample rate. 48 kHz is the universal SDL choice on Windows.
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// DMG master clock (T-cycles per second) — duplicated from `crate::CLOCK_HZ`
/// so this module stays standalone-testable.
const CPU_HZ: u32 = 4_194_304;

/// Frame-sequencer period in T-cycles (8192 → 512 Hz).
const FS_PERIOD_T: u32 = 8_192;

/// Per-register read-back OR masks — bits that always read as 1.
/// Indexed by `addr - 0xFF10`. Wave RAM (`0xFF30..=0xFF3F`) is handled
/// separately (fully readable).
#[rustfmt::skip]
const READ_MASKS: [u8; 0x20] = [
    /* FF10 NR10 */ 0x80,
    /* FF11 NR11 */ 0x3F,
    /* FF12 NR12 */ 0x00,
    /* FF13 NR13 */ 0xFF,
    /* FF14 NR14 */ 0xBF,
    /* FF15      */ 0xFF, // unused
    /* FF16 NR21 */ 0x3F,
    /* FF17 NR22 */ 0x00,
    /* FF18 NR23 */ 0xFF,
    /* FF19 NR24 */ 0xBF,
    /* FF1A NR30 */ 0x7F,
    /* FF1B NR31 */ 0xFF,
    /* FF1C NR32 */ 0x9F,
    /* FF1D NR33 */ 0xFF,
    /* FF1E NR34 */ 0xBF,
    /* FF1F      */ 0xFF, // unused
    /* FF20 NR41 */ 0xFF,
    /* FF21 NR42 */ 0x00,
    /* FF22 NR43 */ 0x00,
    /* FF23 NR44 */ 0xBF,
    /* FF24 NR50 */ 0x00,
    /* FF25 NR51 */ 0x00,
    /* FF26 NR52 */ 0x70,
    /* FF27..FF2F */
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

/// Post-boot register values (what the DMG boot ROM leaves behind). Blargg
/// tests assume these when no boot ROM is in use.
const POST_BOOT_NR10: u8 = 0x80;
const POST_BOOT_NR11: u8 = 0xBF;
const POST_BOOT_NR12: u8 = 0xF3;
const POST_BOOT_NR14: u8 = 0xBF;
const POST_BOOT_NR21: u8 = 0x3F;
const POST_BOOT_NR24: u8 = 0xBF;
const POST_BOOT_NR30: u8 = 0x7F;
const POST_BOOT_NR32: u8 = 0x9F;
const POST_BOOT_NR34: u8 = 0xBF;
const POST_BOOT_NR44: u8 = 0xBF;
const POST_BOOT_NR50: u8 = 0x77;
const POST_BOOT_NR51: u8 = 0xF3;
const POST_BOOT_NR52: u8 = 0xF1;

/// One pulse waveform per duty selector (12.5 %, 25 %, 50 %, 75 %).
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 0, 1, 1, 1], // 50%
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%
];

// ──────────────────────────────────────────────────────────────────────────
// Square channel (used for CH1 with sweep, CH2 without)
// ──────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy)]
struct Envelope {
    initial: u8,    // 0..=15
    increase: bool, // direction
    period: u8,     // 0..=7 (0 disables)
    volume: u8,
    timer: u8,
    enabled: bool,
}

impl Envelope {
    fn write_nrx2(&mut self, val: u8) {
        self.initial = val >> 4;
        self.increase = val & 0x08 != 0;
        self.period = val & 0x07;
    }
    fn nrx2(&self) -> u8 {
        (self.initial << 4) | (u8::from(self.increase) << 3) | self.period
    }
    fn trigger(&mut self) {
        self.timer = if self.period == 0 { 8 } else { self.period };
        self.volume = self.initial;
        self.enabled = true;
    }
    fn tick(&mut self) {
        if !self.enabled || self.period == 0 {
            return;
        }
        self.timer = self.timer.saturating_sub(1);
        if self.timer == 0 {
            self.timer = self.period;
            if self.increase && self.volume < 15 {
                self.volume += 1;
            } else if !self.increase && self.volume > 0 {
                self.volume -= 1;
            } else {
                self.enabled = false;
            }
        }
    }
    /// DAC is off when bits 7..=3 of NRx2 are all zero (env initial = 0 and
    /// direction = decrease). Triggering with DAC off immediately disables
    /// the channel.
    fn dac_enabled(&self) -> bool {
        self.initial != 0 || self.increase
    }
}

#[derive(Debug, Default)]
struct SquareChannel {
    /// Channel-1 has a sweep unit; channel-2 leaves it inert.
    has_sweep: bool,

    enabled: bool, // NR52 status bit
    length: u16,   // 0..=64
    length_enable: bool,

    duty: u8,        // 0..=3
    duty_index: u8,  // 0..=7
    freq: u16,       // 11-bit period selector
    freq_timer: u32, // counts down in T-cycles

    env: Envelope,

    // Sweep state (channel 1 only).
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_timer: u8,
    sweep_enabled: bool,
    sweep_shadow_freq: u16,
    sweep_negate_calculated: bool, // for the "negate-clear" quirk

    nrx0_raw: u8, // raw NR10 byte for readback (also encodes sweep)
    nrx1_raw: u8, // raw NRx1 byte (duty in top 2 bits)
}

impl SquareChannel {
    fn new(has_sweep: bool) -> Self {
        Self { has_sweep, ..Self::default() }
    }

    fn period_t_cycles(&self) -> u32 {
        // (2048 - freq) * 4 T-cycles per duty step.
        (2048u32 - self.freq as u32) * 4
    }

    fn write_nrx0(&mut self, val: u8) {
        // Only meaningful for channel 1. NR10 layout: -PPP NSSS
        self.nrx0_raw = val;
        if self.has_sweep {
            self.sweep_period = (val >> 4) & 0x07;
            let new_negate = val & 0x08 != 0;
            // "Clearing the sweep negate bit after calculating in negate mode
            // disables the channel." (Blargg sweep_details.)
            if self.sweep_negate && !new_negate && self.sweep_negate_calculated {
                self.enabled = false;
            }
            self.sweep_negate = new_negate;
            self.sweep_shift = val & 0x07;
        }
    }
    fn read_nrx0(&self) -> u8 {
        self.nrx0_raw
    }

    fn write_nrx1(&mut self, val: u8, power_on: bool) {
        // Length counter low-6 bits are writable even when APU is off (DMG).
        self.length = 64 - (val & 0x3F) as u16;
        if power_on {
            self.duty = (val >> 6) & 0x03;
            self.nrx1_raw = val;
        }
    }
    fn read_nrx1(&self) -> u8 {
        self.nrx1_raw & 0xC0
    }

    fn write_nrx2(&mut self, val: u8) {
        self.env.write_nrx2(val);
        if !self.env.dac_enabled() {
            self.enabled = false;
        }
    }
    fn read_nrx2(&self) -> u8 {
        self.env.nrx2()
    }

    fn write_nrx3(&mut self, val: u8) {
        self.freq = (self.freq & 0x0700) | val as u16;
    }

    fn write_nrx4(&mut self, val: u8, fs_step: u8) {
        self.freq = (self.freq & 0x00FF) | (((val & 0x07) as u16) << 8);
        let prev_le = self.length_enable;
        self.length_enable = val & 0x40 != 0;

        // Extra-length-clock quirk: enabling length in the half of the frame
        // sequence where the *next* step does NOT clock length causes an
        // immediate extra length clock.
        let next_step_clocks_length = matches!((fs_step + 1) & 7, 0 | 2 | 4 | 6);
        if !prev_le && self.length_enable && !next_step_clocks_length && self.length > 0 {
            self.length -= 1;
            if self.length == 0 && val & 0x80 == 0 {
                self.enabled = false;
            }
        }

        if val & 0x80 != 0 {
            self.trigger(fs_step);
        }
    }

    fn trigger(&mut self, fs_step: u8) {
        self.enabled = true;
        if self.length == 0 {
            self.length = 64;
            // If length is being enabled in the "non-clock" half, this fresh
            // 64 immediately ticks down to 63.
            let next_step_clocks_length = matches!((fs_step + 1) & 7, 0 | 2 | 4 | 6);
            if self.length_enable && !next_step_clocks_length {
                self.length -= 1;
            }
        }
        self.freq_timer = self.period_t_cycles();
        self.env.trigger();

        if self.has_sweep {
            self.sweep_shadow_freq = self.freq;
            self.sweep_timer = if self.sweep_period == 0 { 8 } else { self.sweep_period };
            self.sweep_enabled = self.sweep_period != 0 || self.sweep_shift != 0;
            self.sweep_negate_calculated = false;
            if self.sweep_shift != 0 {
                let _ = self.sweep_calculate();
            }
        }
        if !self.env.dac_enabled() {
            self.enabled = false;
        }
    }

    /// Calculate the next sweep frequency and apply the overflow check.
    /// Returns the calculated frequency. Sets `enabled = false` on overflow.
    fn sweep_calculate(&mut self) -> u16 {
        let delta = self.sweep_shadow_freq >> self.sweep_shift;
        let new_freq = if self.sweep_negate {
            self.sweep_negate_calculated = true;
            self.sweep_shadow_freq.wrapping_sub(delta)
        } else {
            self.sweep_shadow_freq.wrapping_add(delta)
        };
        if new_freq > 2047 {
            self.enabled = false;
        }
        new_freq
    }

    fn tick_t(&mut self, t: u32) {
        if !self.enabled {
            return;
        }
        let mut remaining = t;
        while remaining > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = self.period_t_cycles();
            }
            let step = remaining.min(self.freq_timer);
            self.freq_timer -= step;
            remaining -= step;
            if self.freq_timer == 0 {
                self.duty_index = (self.duty_index + 1) & 7;
            }
        }
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn clock_sweep(&mut self) {
        if !self.has_sweep {
            return;
        }
        self.sweep_timer = self.sweep_timer.saturating_sub(1);
        if self.sweep_timer == 0 {
            self.sweep_timer = if self.sweep_period == 0 { 8 } else { self.sweep_period };
            if self.sweep_enabled && self.sweep_period != 0 {
                let new_freq = self.sweep_calculate();
                if new_freq <= 2047 && self.sweep_shift != 0 {
                    self.freq = new_freq;
                    self.sweep_shadow_freq = new_freq;
                    let _ = self.sweep_calculate(); // second overflow check
                }
            }
        }
    }

    fn sample(&self) -> u8 {
        if !self.enabled || !self.env.dac_enabled() {
            return 0;
        }
        let bit = DUTY_TABLE[self.duty as usize][self.duty_index as usize];
        if bit == 1 {
            self.env.volume
        } else {
            0
        }
    }

    fn power_off_reset(&mut self) {
        // Preserve length counter on DMG.
        let length = self.length;
        let length_enable = self.length_enable;
        *self = Self::new(self.has_sweep);
        self.length = length;
        let _ = length_enable; // length-enable IS cleared on power-off
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Wave channel (CH3)
// ──────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct WaveChannel {
    enabled: bool, // NR52 status bit
    dac_enabled: bool,
    length: u16, // 0..=256
    length_enable: bool,
    volume_code: u8, // 0..=3
    freq: u16,
    freq_timer: u32,
    sample_index: u8, // 0..=31
    ram: [u8; 16],
    nr31_raw: u8,
}

impl WaveChannel {
    fn period_t_cycles(&self) -> u32 {
        (2048u32 - self.freq as u32) * 2
    }

    fn write_nr30(&mut self, val: u8) {
        self.dac_enabled = val & 0x80 != 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }
    fn read_nr30(&self) -> u8 {
        u8::from(self.dac_enabled) << 7
    }

    fn write_nr31(&mut self, val: u8) {
        self.length = 256 - val as u16;
        self.nr31_raw = val;
    }

    fn write_nr32(&mut self, val: u8) {
        self.volume_code = (val >> 5) & 0x03;
    }
    fn read_nr32(&self) -> u8 {
        self.volume_code << 5
    }

    fn write_nr33(&mut self, val: u8) {
        self.freq = (self.freq & 0x0700) | val as u16;
    }

    fn write_nr34(&mut self, val: u8, fs_step: u8) {
        self.freq = (self.freq & 0x00FF) | (((val & 0x07) as u16) << 8);
        let prev_le = self.length_enable;
        self.length_enable = val & 0x40 != 0;

        let next_step_clocks_length = matches!((fs_step + 1) & 7, 0 | 2 | 4 | 6);
        if !prev_le && self.length_enable && !next_step_clocks_length && self.length > 0 {
            self.length -= 1;
            if self.length == 0 && val & 0x80 == 0 {
                self.enabled = false;
            }
        }

        if val & 0x80 != 0 {
            self.trigger(fs_step);
        }
    }

    fn trigger(&mut self, fs_step: u8) {
        self.enabled = true;
        if self.length == 0 {
            self.length = 256;
            let next_step_clocks_length = matches!((fs_step + 1) & 7, 0 | 2 | 4 | 6);
            if self.length_enable && !next_step_clocks_length {
                self.length -= 1;
            }
        }
        self.freq_timer = self.period_t_cycles();
        self.sample_index = 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }

    fn tick_t(&mut self, t: u32) {
        if !self.enabled {
            return;
        }
        let mut remaining = t;
        while remaining > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = self.period_t_cycles();
            }
            let step = remaining.min(self.freq_timer);
            self.freq_timer -= step;
            remaining -= step;
            if self.freq_timer == 0 {
                self.sample_index = (self.sample_index + 1) & 31;
            }
        }
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn sample(&self) -> u8 {
        if !self.enabled || !self.dac_enabled {
            return 0;
        }
        let byte = self.ram[(self.sample_index >> 1) as usize];
        let nibble = if self.sample_index & 1 == 0 { byte >> 4 } else { byte & 0x0F };
        match self.volume_code {
            0 => 0,
            1 => nibble,
            2 => nibble >> 1,
            _ => nibble >> 2,
        }
    }

    fn power_off_reset(&mut self) {
        // Wave RAM is preserved; length counter preserved on DMG.
        let length = self.length;
        let ram = self.ram;
        *self = Self::default();
        self.length = length;
        self.ram = ram;
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Noise channel (CH4)
// ──────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct NoiseChannel {
    enabled: bool,
    length: u16, // 0..=64
    length_enable: bool,
    env: Envelope,

    clock_shift: u8,
    width_mode_7bit: bool,
    divisor_code: u8,

    timer: u32,
    lfsr: u16,

    nr43_raw: u8,
}

impl NoiseChannel {
    fn period_t_cycles(&self) -> u32 {
        let divisor: u32 = match self.divisor_code {
            0 => 8,
            n => (n as u32) * 16,
        };
        divisor << self.clock_shift
    }

    fn write_nr41(&mut self, val: u8) {
        self.length = 64 - (val & 0x3F) as u16;
    }

    fn write_nr42(&mut self, val: u8) {
        self.env.write_nrx2(val);
        if !self.env.dac_enabled() {
            self.enabled = false;
        }
    }
    fn read_nr42(&self) -> u8 {
        self.env.nrx2()
    }

    fn write_nr43(&mut self, val: u8) {
        self.nr43_raw = val;
        self.clock_shift = val >> 4;
        self.width_mode_7bit = val & 0x08 != 0;
        self.divisor_code = val & 0x07;
    }
    fn read_nr43(&self) -> u8 {
        self.nr43_raw
    }

    fn write_nr44(&mut self, val: u8, fs_step: u8) {
        let prev_le = self.length_enable;
        self.length_enable = val & 0x40 != 0;

        let next_step_clocks_length = matches!((fs_step + 1) & 7, 0 | 2 | 4 | 6);
        if !prev_le && self.length_enable && !next_step_clocks_length && self.length > 0 {
            self.length -= 1;
            if self.length == 0 && val & 0x80 == 0 {
                self.enabled = false;
            }
        }

        if val & 0x80 != 0 {
            self.trigger(fs_step);
        }
    }

    fn trigger(&mut self, fs_step: u8) {
        self.enabled = true;
        if self.length == 0 {
            self.length = 64;
            let next_step_clocks_length = matches!((fs_step + 1) & 7, 0 | 2 | 4 | 6);
            if self.length_enable && !next_step_clocks_length {
                self.length -= 1;
            }
        }
        self.timer = self.period_t_cycles().max(1);
        self.lfsr = 0x7FFF;
        self.env.trigger();
        if !self.env.dac_enabled() {
            self.enabled = false;
        }
    }

    fn tick_t(&mut self, t: u32) {
        if !self.enabled {
            return;
        }
        let mut remaining = t;
        while remaining > 0 {
            if self.timer == 0 {
                self.timer = self.period_t_cycles().max(1);
            }
            let step = remaining.min(self.timer);
            self.timer -= step;
            remaining -= step;
            if self.timer == 0 {
                let bit = (self.lfsr & 1) ^ ((self.lfsr >> 1) & 1);
                self.lfsr = (self.lfsr >> 1) | (bit << 14);
                if self.width_mode_7bit {
                    self.lfsr = (self.lfsr & !(1 << 6)) | (bit << 6);
                }
            }
        }
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn sample(&self) -> u8 {
        if !self.enabled || !self.env.dac_enabled() {
            return 0;
        }
        if self.lfsr & 1 == 0 {
            self.env.volume
        } else {
            0
        }
    }

    fn power_off_reset(&mut self) {
        let length = self.length;
        *self = Self::default();
        self.length = length;
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Top-level APU
// ──────────────────────────────────────────────────────────────────────────

pub struct Apu {
    powered_on: bool,
    nr50: u8,
    nr51: u8,

    ch1: SquareChannel,
    ch2: SquareChannel,
    ch3: WaveChannel,
    ch4: NoiseChannel,

    fs_counter: u32, // T-cycles until next 512 Hz tick
    fs_step: u8,     // 0..=7

    // Down-sampler.
    sample_accum: u32, // fixed-point: increments by `SAMPLE_RATE_HZ` per T-cycle
    samples: Vec<(i16, i16)>,
}

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Apu")
            .field("powered_on", &self.powered_on)
            .field("nr50", &format_args!("{:#04X}", self.nr50))
            .field("nr51", &format_args!("{:#04X}", self.nr51))
            .field("fs_step", &self.fs_step)
            .field("pending_samples", &self.samples.len())
            .finish()
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub fn new() -> Self {
        let mut apu = Self {
            powered_on: true,
            nr50: POST_BOOT_NR50,
            nr51: POST_BOOT_NR51,
            ch1: SquareChannel::new(true),
            ch2: SquareChannel::new(false),
            ch3: WaveChannel::default(),
            ch4: NoiseChannel::default(),
            fs_counter: FS_PERIOD_T,
            fs_step: 0,
            sample_accum: 0,
            samples: Vec::with_capacity(2048),
        };
        apu.init_post_boot();
        apu
    }

    /// Recreate the register state the DMG boot ROM leaves behind.
    fn init_post_boot(&mut self) {
        self.ch1.write_nrx0(POST_BOOT_NR10);
        self.ch1.write_nrx1(POST_BOOT_NR11, true);
        self.ch1.write_nrx2(POST_BOOT_NR12);
        self.ch1.length_enable = POST_BOOT_NR14 & 0x40 != 0;
        self.ch2.write_nrx1(POST_BOOT_NR21, true);
        self.ch2.length_enable = POST_BOOT_NR24 & 0x40 != 0;
        self.ch3.write_nr30(POST_BOOT_NR30);
        self.ch3.write_nr32(POST_BOOT_NR32);
        self.ch3.length_enable = POST_BOOT_NR34 & 0x40 != 0;
        self.ch4.length_enable = POST_BOOT_NR44 & 0x40 != 0;
        // NR52 power bit already set; channel-status bits are derived live.
        let _ = POST_BOOT_NR52;
    }

    pub fn tick(&mut self, t_cycles: u32) {
        if self.powered_on {
            // Run the frame sequencer.
            let mut remaining = t_cycles;
            while remaining > 0 {
                let step = remaining.min(self.fs_counter);
                self.ch1.tick_t(step);
                self.ch2.tick_t(step);
                self.ch3.tick_t(step);
                self.ch4.tick_t(step);
                self.fs_counter -= step;
                remaining -= step;
                if self.fs_counter == 0 {
                    self.fs_counter = FS_PERIOD_T;
                    self.fs_tick();
                }
            }
        }

        // Sample generation continues even with the APU off (silence). We use
        // an integer fractional counter: each T-cycle adds SAMPLE_RATE_HZ to
        // an accumulator; when it crosses CPU_HZ we emit one sample.
        for _ in 0..t_cycles {
            self.sample_accum += SAMPLE_RATE_HZ;
            if self.sample_accum >= CPU_HZ {
                self.sample_accum -= CPU_HZ;
                self.emit_sample();
            }
        }
    }

    fn fs_tick(&mut self) {
        match self.fs_step {
            0 | 4 => self.clock_length(),
            2 | 6 => {
                self.clock_length();
                self.ch1.clock_sweep();
            }
            7 => {
                self.ch1.env.tick();
                self.ch2.env.tick();
                self.ch4.env.tick();
            }
            _ => {}
        }
        self.fs_step = (self.fs_step + 1) & 7;
    }

    fn clock_length(&mut self) {
        self.ch1.clock_length();
        self.ch2.clock_length();
        self.ch3.clock_length();
        self.ch4.clock_length();
    }

    fn emit_sample(&mut self) {
        // Pull 4-bit DAC inputs from each channel (0..=15).
        let s1 = self.ch1.sample();
        let s2 = self.ch2.sample();
        let s3 = self.ch3.sample();
        let s4 = self.ch4.sample();

        // DAC: 0..=15 → -1.0..=+1.0. Channels whose DAC is off emit 0.
        let d1 = if self.ch1.env.dac_enabled() { dac(s1) } else { 0.0 };
        let d2 = if self.ch2.env.dac_enabled() { dac(s2) } else { 0.0 };
        let d3 = if self.ch3.dac_enabled { dac(s3) } else { 0.0 };
        let d4 = if self.ch4.env.dac_enabled() { dac(s4) } else { 0.0 };

        // Mix using the NR51 panning matrix.
        let n51 = self.nr51;
        let mut left = 0.0;
        let mut right = 0.0;
        if n51 & 0x10 != 0 {
            left += d1;
        }
        if n51 & 0x20 != 0 {
            left += d2;
        }
        if n51 & 0x40 != 0 {
            left += d3;
        }
        if n51 & 0x80 != 0 {
            left += d4;
        }
        if n51 & 0x01 != 0 {
            right += d1;
        }
        if n51 & 0x02 != 0 {
            right += d2;
        }
        if n51 & 0x04 != 0 {
            right += d3;
        }
        if n51 & 0x08 != 0 {
            right += d4;
        }

        // Master volume (NR50): bits 0-2 right level, bits 4-6 left level.
        let left_vol = ((self.nr50 >> 4) & 0x07) as f32 / 7.0;
        let right_vol = (self.nr50 & 0x07) as f32 / 7.0;
        left *= left_vol;
        right *= right_vol;

        // 4 channels summed, range roughly -4..=+4. Scale to leave headroom.
        const HEADROOM: f32 = 0.20; // gives ~i16::MAX/5 per channel at full env
        let li = (left * HEADROOM * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32);
        let ri = (right * HEADROOM * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32);
        self.samples.push((li as i16, ri as i16));
    }

    /// Drain (and return) the samples produced since the last call. Frontends
    /// call this once per host frame and forward the result to the audio sink.
    pub fn drain_samples(&mut self) -> Vec<(i16, i16)> {
        std::mem::take(&mut self.samples)
    }

    /// CPU read of an APU register or wave-RAM byte.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF10..=0xFF2F => {
                let i = (addr - 0xFF10) as usize;
                let raw = match addr {
                    0xFF10 => self.ch1.read_nrx0(),
                    0xFF11 => self.ch1.read_nrx1(),
                    0xFF12 => self.ch1.read_nrx2(),
                    0xFF13 => 0x00,
                    0xFF14 => u8::from(self.ch1.length_enable) << 6,
                    0xFF16 => self.ch2.read_nrx1(),
                    0xFF17 => self.ch2.read_nrx2(),
                    0xFF18 => 0x00,
                    0xFF19 => u8::from(self.ch2.length_enable) << 6,
                    0xFF1A => self.ch3.read_nr30(),
                    0xFF1B => 0x00,
                    0xFF1C => self.ch3.read_nr32(),
                    0xFF1D => 0x00,
                    0xFF1E => u8::from(self.ch3.length_enable) << 6,
                    0xFF20 => 0x00,
                    0xFF21 => self.ch4.read_nr42(),
                    0xFF22 => self.ch4.read_nr43(),
                    0xFF23 => u8::from(self.ch4.length_enable) << 6,
                    0xFF24 => self.nr50,
                    0xFF25 => self.nr51,
                    0xFF26 => self.read_nr52(),
                    _ => 0x00,
                };
                raw | READ_MASKS[i]
            }
            0xFF30..=0xFF3F => self.ch3.ram[(addr - 0xFF30) as usize],
            _ => 0xFF,
        }
    }

    /// CPU write to an APU register or wave-RAM byte.
    pub fn write(&mut self, addr: u16, val: u8) {
        // Wave RAM is always accessible.
        if (0xFF30..=0xFF3F).contains(&addr) {
            self.ch3.ram[(addr - 0xFF30) as usize] = val;
            return;
        }
        // NR52 power control is always accessible.
        if addr == 0xFF26 {
            self.write_nr52(val);
            return;
        }
        // While powered off, most writes are ignored. On DMG the length-load
        // low 6 bits of NR11/NR21/NR31/NR41 remain writable.
        if !self.powered_on {
            match addr {
                0xFF11 => self.ch1.write_nrx1(val, false),
                0xFF16 => self.ch2.write_nrx1(val, false),
                0xFF1B => self.ch3.write_nr31(val),
                0xFF20 => self.ch4.write_nr41(val),
                _ => {}
            }
            return;
        }
        let fs = self.fs_step;
        match addr {
            0xFF10 => self.ch1.write_nrx0(val),
            0xFF11 => self.ch1.write_nrx1(val, true),
            0xFF12 => self.ch1.write_nrx2(val),
            0xFF13 => self.ch1.write_nrx3(val),
            0xFF14 => self.ch1.write_nrx4(val, fs),
            0xFF16 => self.ch2.write_nrx1(val, true),
            0xFF17 => self.ch2.write_nrx2(val),
            0xFF18 => self.ch2.write_nrx3(val),
            0xFF19 => self.ch2.write_nrx4(val, fs),
            0xFF1A => self.ch3.write_nr30(val),
            0xFF1B => self.ch3.write_nr31(val),
            0xFF1C => self.ch3.write_nr32(val),
            0xFF1D => self.ch3.write_nr33(val),
            0xFF1E => self.ch3.write_nr34(val, fs),
            0xFF20 => self.ch4.write_nr41(val),
            0xFF21 => self.ch4.write_nr42(val),
            0xFF22 => self.ch4.write_nr43(val),
            0xFF23 => self.ch4.write_nr44(val, fs),
            0xFF24 => self.nr50 = val,
            0xFF25 => self.nr51 = val,
            _ => {} // FF15, FF1F, FF27-FF2F are unmapped / read-only
        }
    }

    fn read_nr52(&self) -> u8 {
        let mut v = if self.powered_on { 0x80 } else { 0x00 };
        if self.ch1.enabled {
            v |= 0x01;
        }
        if self.ch2.enabled {
            v |= 0x02;
        }
        if self.ch3.enabled {
            v |= 0x04;
        }
        if self.ch4.enabled {
            v |= 0x08;
        }
        v
    }

    fn write_nr52(&mut self, val: u8) {
        let want_on = val & 0x80 != 0;
        if !want_on && self.powered_on {
            // Power off: clear all channel registers (except length counters,
            // which DMG preserves) and zero NR50/NR51.
            for addr in 0xFF10..=0xFF25 {
                // Bypass the powered-off filter via direct field writes.
                self.power_off_write(addr);
            }
            self.nr50 = 0;
            self.nr51 = 0;
            self.ch1.power_off_reset();
            self.ch2.power_off_reset();
            self.ch3.power_off_reset();
            self.ch4.power_off_reset();
            self.powered_on = false;
        } else if want_on && !self.powered_on {
            self.powered_on = true;
            self.fs_step = 0;
            self.fs_counter = FS_PERIOD_T;
            self.ch1.duty_index = 0;
            self.ch2.duty_index = 0;
            self.ch3.sample_index = 0;
        }
    }

    /// Used only inside `write_nr52(0)` to make sure every register is
    /// observably zeroed before the channel `power_off_reset` call wipes
    /// internal state. Currently a no-op placeholder since `power_off_reset`
    /// handles it — kept for symmetry / future expansion.
    fn power_off_write(&mut self, _addr: u16) {}
}

#[inline]
fn dac(sample_4bit: u8) -> f32 {
    // 0 → +1.0, 15 → -1.0. (The DMG DAC is inverting, but for audio purposes
    // either polarity sounds the same; we map 0..=15 → -1..=+1 for clarity.)
    (sample_4bit as f32) / 7.5 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr52_initial_value_after_boot() {
        let apu = Apu::new();
        // Bits 4-6 always read 1 (mask 0x70). Power bit 1. CH1 starts off
        // (length enable but not triggered) so the channel-status bits are 0.
        // Initial NR52 read: 0x80 (power) | 0x70 (mask) = 0xF0.
        assert_eq!(apu.read(0xFF26), 0xF0);
    }

    #[test]
    fn power_off_clears_registers() {
        let mut apu = Apu::new();
        // Powering off clears NR10..NR51.
        apu.write(0xFF26, 0x00);
        for addr in 0xFF10..=0xFF25 {
            let expected = READ_MASKS[(addr - 0xFF10) as usize];
            assert_eq!(apu.read(addr), expected, "addr {addr:#06X}");
        }
        // Wave RAM should NOT be cleared.
        apu.write(0xFF26, 0x80);
        apu.write(0xFF30, 0xAB);
        apu.write(0xFF26, 0x00);
        assert_eq!(apu.read(0xFF30), 0xAB);
    }

    #[test]
    fn writes_ignored_while_powered_off_except_length_and_wave() {
        let mut apu = Apu::new();
        apu.write(0xFF26, 0x00); // power off
        apu.write(0xFF12, 0xF0); // NR12 — should be ignored
        assert_eq!(apu.read(0xFF12), 0x00);
        // NR11 length write should land (DMG).
        apu.write(0xFF11, 0x10);
        // Length isn't directly observable on readback (duty bits only), but
        // we can check internal state.
        assert_eq!(apu.ch1.length, 64 - 0x10);
        // Wave RAM remains writable.
        apu.write(0xFF35, 0xCD);
        assert_eq!(apu.read(0xFF35), 0xCD);
    }

    #[test]
    fn nr52_only_writes_bit7() {
        let mut apu = Apu::new();
        apu.write(0xFF26, 0x7F); // all low bits, no power bit
        assert_eq!(apu.read(0xFF26) & 0x80, 0);
        apu.write(0xFF26, 0x80);
        assert_eq!(apu.read(0xFF26) & 0x80, 0x80);
    }

    #[test]
    fn samples_accumulate_at_target_rate() {
        let mut apu = Apu::new();
        apu.tick(CPU_HZ); // exactly one second of audio
        let samples = apu.drain_samples();
        // Allow ±1 sample for rounding.
        assert!(
            (samples.len() as i64 - SAMPLE_RATE_HZ as i64).abs() <= 1,
            "got {} samples, expected ~{}",
            samples.len(),
            SAMPLE_RATE_HZ
        );
    }
}
