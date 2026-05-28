//! Top-level [`Gameboy`] struct that owns and ticks all subsystems.

use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::cpu::Cpu;
use crate::ppu::FrameBuffer;
use crate::{Error, T_CYCLES_PER_FRAME};

/// Construction-time options for the emulator.
#[derive(Debug, Default, Clone)]
pub struct GameboyOptions {
    /// Optional DMG boot ROM (256 bytes). When `None`, post-bootrom CPU state
    /// is initialized directly.
    pub boot_rom: Option<Vec<u8>>,
}

/// Owns the entire emulated machine.
#[derive(Debug)]
pub struct Gameboy {
    cpu: Cpu,
    bus: Bus,
}

impl Gameboy {
    /// Construct a new emulator from a cartridge ROM and options.
    pub fn new(rom: Vec<u8>, opts: GameboyOptions) -> Result<Self, Error> {
        let cart = Cartridge::from_rom(rom)?;
        let bus = Bus::new(cart, opts.boot_rom);
        let cpu = Cpu::post_boot();
        Ok(Self { cpu, bus })
    }

    /// Run until the PPU has produced one full frame.
    /// Returns the number of T-cycles actually advanced.
    pub fn run_frame(&mut self) -> u32 {
        let mut elapsed = 0u32;
        while elapsed < T_CYCLES_PER_FRAME {
            elapsed += self.step();
        }
        elapsed
    }

    /// Advance the machine by one CPU instruction. Returns elapsed T-cycles.
    pub fn step(&mut self) -> u32 {
        let m_cycles = self.cpu.step(&mut self.bus);
        let t_cycles = m_cycles * 4;
        self.bus.tick(t_cycles);
        t_cycles
    }

    /// The most recently completed frame (160×144).
    pub fn frame_buffer(&self) -> &FrameBuffer {
        self.bus.ppu().frame_buffer()
    }
}
