//! # gb-core
//!
//! Pure-logic Nintendo Game Boy (DMG) emulator core. This crate performs **no
//! I/O**: it does not open windows, files, or audio devices, and it does not
//! spawn threads. Frontends are responsible for hosting the [`Gameboy`] and
//! pumping it with input/output.
//!
//! See `docs/architecture.md` in the repository root for the full design.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod hdma;
pub mod interrupts;
pub mod joypad;
pub mod ppu;
pub mod serial;
pub mod timer;

mod gameboy;

pub use gameboy::{Gameboy, GameboyOptions};
pub use interrupts::IntFlags;
pub use joypad::{Button, JoypadState};
pub use ppu::{FrameBuffer, SCREEN_HEIGHT, SCREEN_WIDTH};

/// Master clock frequency in Hz (T-cycles per second).
pub const CLOCK_HZ: u32 = 4_194_304;

/// T-cycles in a single PPU frame (154 scanlines × 456 cycles).
pub const T_CYCLES_PER_FRAME: u32 = 70_224;

/// Top-level error type surfaced to frontends.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid ROM: {0}")]
    InvalidRom(&'static str),
    #[error("unsupported cartridge type: 0x{0:02X}")]
    UnsupportedCartridge(u8),
}
