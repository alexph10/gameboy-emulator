//! Top-level [`Gameboy`] struct that owns and ticks all subsystems.

use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::cpu::Cpu;
use crate::joypad::JoypadState;
use crate::ppu::FrameBuffer;
use crate::Error;

/// Construction-time options for the emulator.
#[derive(Debug, Default, Clone)]
pub struct GameboyOptions {
    /// Optional DMG/CGB boot ROM (256 bytes for DMG, 2304 bytes for CGB).
    /// When `None`, post-bootrom CPU state is initialized directly.
    pub boot_rom: Option<Vec<u8>>,
    /// Force DMG behaviour even if the cartridge header advertises CGB
    /// compatibility (`0x80`). Has no effect for CGB-only carts (`0xC0`).
    pub force_dmg: bool,
}

/// Owns the entire emulated machine.
#[derive(Debug)]
pub struct Gameboy {
    cpu: Cpu,
    bus: Bus,
    rom: Vec<u8>,
    opts: GameboyOptions,
    cgb_mode: bool,
}

impl Gameboy {
    /// Construct a new emulator from a cartridge ROM and options.
    pub fn new(rom: Vec<u8>, opts: GameboyOptions) -> Result<Self, Error> {
        let cart = Cartridge::from_rom(rom.clone())?;
        let cgb_mode = cart.header.is_cgb() && !opts.force_dmg;
        let bus = Bus::new_with_mode(cart, opts.boot_rom.clone(), cgb_mode);
        let cpu = if cgb_mode { Cpu::post_boot_cgb() } else { Cpu::post_boot() };
        Ok(Self { cpu, bus, rom, opts, cgb_mode })
    }

    /// Re-create the machine from the same ROM and options (soft reset).
    pub fn reset(&mut self) {
        let cart = Cartridge::from_rom(self.rom.clone()).expect("ROM was valid at construction");
        self.cgb_mode = cart.header.is_cgb() && !self.opts.force_dmg;
        self.bus = Bus::new_with_mode(cart, self.opts.boot_rom.clone(), self.cgb_mode);
        self.cpu = if self.cgb_mode { Cpu::post_boot_cgb() } else { Cpu::post_boot() };
    }

    /// True if the emulator is currently running in CGB mode.
    pub fn cgb_mode(&self) -> bool {
        self.cgb_mode
    }

    /// Forward host input state to the joypad. Frontends should call this once
    /// per frame before [`Self::run_frame`].
    pub fn set_buttons(&mut self, state: JoypadState) {
        self.bus.set_joypad(state);
    }

    /// Run until the PPU finishes one full frame (rising edge of VBlank).
    /// Returns the number of T-cycles actually advanced. A hard upper bound
    /// prevents spinning forever if the PPU is disabled.
    pub fn run_frame(&mut self) -> u32 {
        let _ = self.bus.ppu_mut().take_frame_ready();
        let mut elapsed = 0u32;
        let cap = crate::T_CYCLES_PER_FRAME * 2;
        while elapsed < cap {
            elapsed += self.step();
            if self.bus.ppu_mut().take_frame_ready() {
                break;
            }
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

    /// The most recently completed frame (160×144 RGB555).
    pub fn frame_buffer(&self) -> &FrameBuffer {
        self.bus.ppu().frame_buffer()
    }

    /// Captured serial output. Test ROMs (e.g. Blargg) write their results to
    /// the serial port; integration tests scrape this buffer.
    pub fn serial_output(&self) -> &[u8] {
        self.bus.serial_output()
    }

    /// Drain (and return) the stereo audio samples the APU has produced since
    /// the last call. Frontends call this once per host frame and forward the
    /// result to the audio sink. Each tuple is `(left, right)` at
    /// [`crate::apu::SAMPLE_RATE_HZ`].
    pub fn take_audio_samples(&mut self) -> Vec<(i16, i16)> {
        self.bus.take_audio_samples()
    }

    /// Read a single byte from the CPU-visible address space. Intended for
    /// integration tests (e.g. scraping Blargg's `$A000` result byte and the
    /// zero-terminated text log at `$A004`). Routes through the bus, so cart
    /// RAM access still requires the ROM to have enabled MBC RAM.
    pub fn peek_byte(&self, addr: u16) -> u8 {
        self.bus.read(addr)
    }

    /// `true` if the cartridge has battery-backed RAM that frontends should
    /// persist between sessions (Pokémon, Zelda, Mario Land 2, etc.).
    pub fn cart_has_battery(&self) -> bool {
        self.bus.cart_has_battery()
    }

    /// Current battery-RAM contents, suitable for writing to a `.sav` file.
    /// Returns `None` for cartridges without external RAM.
    pub fn cart_ram(&self) -> Option<&[u8]> {
        self.bus.cart_ram()
    }

    /// Replace battery-RAM contents from a `.sav` file at load time. Size
    /// mismatches are silently clamped to the cartridge's RAM size.
    pub fn load_cart_ram(&mut self, data: &[u8]) {
        self.bus.load_cart_ram(data);
    }
}
