//! Audio Processing Unit (4 channels + frame sequencer).
//! See `docs/apu.md`.

#[derive(Debug, Default)]
pub struct Apu {
    // TODO: channel1 (square+sweep), channel2 (square),
    // channel3 (wave), channel4 (noise), frame sequencer, mixer.
}

impl Apu {
    pub fn new() -> Self { Self::default() }

    pub fn tick(&mut self, _t_cycles: u32) {
        // TODO: advance frame sequencer and per-channel timers.
    }

    pub fn read(&self, _addr: u16) -> u8 { 0xFF }
    pub fn write(&mut self, _addr: u16, _val: u8) {}
}
