//! Memory bus / MMU.
//!
//! Routes CPU memory accesses to the appropriate subsystem per the DMG/CGB
//! memory map (`docs/memory-map.md`) and ticks non-CPU subsystems in T-cycles.

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::hdma::{Hdma, HdmaWrite};
use crate::interrupts::{IntFlags, Interrupts};
use crate::joypad::Joypad;
use crate::ppu::Ppu;
use crate::serial::Serial;
use crate::timer::Timer;

#[derive(Debug)]
pub struct Bus {
    cart: Cartridge,
    ppu: Ppu,
    apu: Apu,
    timer: Timer,
    joypad: Joypad,
    serial: Serial,
    interrupts: Interrupts,
    hdma: Hdma,

    /// 32 KiB of WRAM. In DMG mode only the first 8 KiB is used; in CGB mode
    /// the upper bank window at `0xD000..=0xDFFF` is selected by SVBK.
    wram: [u8; 0x8000],
    hram: [u8; 0x7F],

    cgb_mode: bool,
    svbk: u8, // FF70 — WRAM bank select (CGB)

    // KEY1 (FF4D) — CGB CPU speed switch.
    key1: u8,

    /// CGB undocumented r/w registers (FF72, FF73, FF74). FF75 is bit-masked
    /// (only bits 4..=6 are writable).
    undoc_ff72: u8,
    undoc_ff73: u8,
    undoc_ff74: u8,
    undoc_ff75: u8,

    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
}

impl Bus {
    pub fn new(cart: Cartridge, boot_rom: Option<Vec<u8>>) -> Self {
        Self::new_with_mode(cart, boot_rom, false)
    }

    pub fn new_with_mode(cart: Cartridge, boot_rom: Option<Vec<u8>>, cgb_mode: bool) -> Self {
        let boot_rom_enabled = boot_rom.is_some();
        Self {
            cart,
            ppu: Ppu::with_mode(cgb_mode),
            apu: Apu::new(),
            timer: Timer::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            interrupts: Interrupts::new(),
            hdma: Hdma::new(),
            wram: [0; 0x8000],
            hram: [0; 0x7F],
            cgb_mode,
            svbk: 1,
            key1: 0,
            undoc_ff72: 0,
            undoc_ff73: 0,
            undoc_ff74: 0,
            undoc_ff75: 0,
            boot_rom,
            boot_rom_enabled,
        }
    }

    pub fn ppu(&self) -> &Ppu {
        &self.ppu
    }

    pub fn cgb_mode(&self) -> bool {
        self.cgb_mode
    }

    /// Battery-backed cartridge RAM (for `.sav` persistence).
    pub fn cart_ram(&self) -> Option<&[u8]> {
        self.cart.ram()
    }

    /// `true` if the cartridge has battery-backed RAM that should be
    /// persisted across sessions.
    pub fn cart_has_battery(&self) -> bool {
        self.cart.has_battery()
    }

    /// Restore battery RAM contents from a previously saved `.sav` file.
    pub fn load_cart_ram(&mut self, data: &[u8]) {
        self.cart.load_ram(data);
    }

    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.ppu
    }

    /// Drain any audio samples the APU has produced since the last call.
    pub fn take_audio_samples(&mut self) -> Vec<(i16, i16)> {
        self.apu.drain_samples()
    }

    /// Captured serial output (Blargg test ROMs write their results here).
    pub fn serial_output(&self) -> &[u8] {
        &self.serial.output_log
    }

    /// Interrupt-enable register (`FFFF`).
    pub fn ie(&self) -> IntFlags {
        self.interrupts.ie
    }
    /// Interrupt-flag register (`FF0F`).
    pub fn iflag(&self) -> IntFlags {
        self.interrupts.iflag
    }
    /// Clear specific bits in `IF` (used by the CPU when servicing an interrupt).
    pub fn clear_if(&mut self, flag: IntFlags) {
        self.interrupts.iflag.remove(flag);
    }
    /// `IE & IF` — set of interrupts that are both requested and enabled.
    pub fn pending_interrupts(&self) -> IntFlags {
        self.interrupts.pending()
    }

    /// Push the latest host input state into the joypad. Triggers
    /// `IF.JOYPAD` if any button transitioned from released to pressed.
    pub fn set_joypad(&mut self, state: crate::joypad::JoypadState) {
        if self.joypad.set_state(state) {
            self.interrupts.request(IntFlags::JOYPAD);
        }
    }

    /// Advance non-CPU subsystems by `t_cycles` T-cycles.
    pub fn tick(&mut self, t_cycles: u32) {
        self.timer.tick(t_cycles, &mut self.interrupts);
        self.ppu.tick(t_cycles, &mut self.interrupts);
        self.apu.tick(t_cycles);

        // CGB HBlank-DMA: copy one 16-byte block on each HBlank entry.
        // We can fire multiple HBlanks per `tick` if `t_cycles` spans more
        // than one scanline, but the PPU only sets the edge once per HBlank
        // so this is naturally rate-limited.
        if self.cgb_mode && self.hdma.is_hblank_active() {
            while self.ppu.take_hblank_edge() {
                if let Some(block) = self.hdma.step_hblank() {
                    for i in 0..16u16 {
                        let b = self.read(block.src + i);
                        self.ppu.write_vram(block.dst + i, b);
                    }
                } else {
                    break;
                }
            }
        } else {
            // Drain the edge so it doesn't accumulate.
            let _ = self.ppu.take_hblank_edge();
        }
    }

    /// Current state of KEY1 — used by CPU `STOP` for speed switch handling.
    pub fn key1(&self) -> u8 {
        self.key1
    }

    /// CPU `STOP` instruction hook. If KEY1 bit 0 is set, toggle the
    /// current-speed bit (bit 7) and clear the request bit.
    pub fn handle_stop(&mut self) {
        if self.cgb_mode && (self.key1 & 0x01) != 0 {
            self.key1 ^= 0x80;
            self.key1 &= !0x01;
        }
    }

    /// 8-bit read from the CPU.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            // Boot ROM overlay (DMG: 0x00..0xFF; CGB: 0x00..0xFF + 0x200..0x8FF).
            0x0000..=0x00FF if self.boot_rom_enabled => {
                self.boot_rom.as_ref().map_or(0xFF, |b| {
                    b.get(addr as usize).copied().unwrap_or(0xFF)
                })
            }
            0x0200..=0x08FF if self.boot_rom_enabled && self.cgb_mode => self
                .boot_rom
                .as_ref()
                .and_then(|b| b.get(addr as usize).copied())
                .unwrap_or_else(|| self.cart.read_rom(addr)),
            0x0000..=0x7FFF => self.cart.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read_ram(addr),
            0xC000..=0xCFFF => self.wram[(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.wram[self.wram_bank_base() + (addr - 0xD000) as usize],
            0xE000..=0xEFFF => self.wram[(addr - 0xE000) as usize],
            0xF000..=0xFDFF => self.wram[self.wram_bank_base() + (addr - 0xF000) as usize],
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0xFF, // prohibited
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.interrupts.read_ie(),
        }
    }

    /// 8-bit write from the CPU.
    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x7FFF => self.cart.write_rom(addr, val),
            0x8000..=0x9FFF => self.ppu.write_vram(addr, val),
            0xA000..=0xBFFF => self.cart.write_ram(addr, val),
            0xC000..=0xCFFF => self.wram[(addr - 0xC000) as usize] = val,
            0xD000..=0xDFFF => {
                let base = self.wram_bank_base();
                self.wram[base + (addr - 0xD000) as usize] = val;
            }
            0xE000..=0xEFFF => self.wram[(addr - 0xE000) as usize] = val,
            0xF000..=0xFDFF => {
                let base = self.wram_bank_base();
                self.wram[base + (addr - 0xF000) as usize] = val;
            }
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, val),
            0xFEA0..=0xFEFF => {} // prohibited
            0xFF00..=0xFF7F => self.write_io(addr, val),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.interrupts.write_ie(val),
        }
    }

    #[inline]
    fn wram_bank_base(&self) -> usize {
        if self.cgb_mode {
            let bank = (self.svbk & 0x07).max(1) as usize;
            bank * 0x1000
        } else {
            0x1000 // single 4 KiB upper bank
        }
    }

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            0xFF00 => self.joypad.read(),
            0xFF01..=0xFF02 => self.serial.read(addr),
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.interrupts.read_if(),
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF40..=0xFF4B => self.ppu.read_reg(addr),
            0xFF4D => {
                if self.cgb_mode {
                    0x7E | (self.key1 & 0x81)
                } else {
                    0xFF
                }
            }
            0xFF4F => self.ppu.read_reg(addr),
            0xFF50 => 0xFF,
            0xFF51..=0xFF55 => {
                if self.cgb_mode {
                    self.hdma.read(addr)
                } else {
                    0xFF
                }
            }
            0xFF68..=0xFF6C => self.ppu.read_reg(addr),
            0xFF70 => {
                if self.cgb_mode {
                    0xF8 | (self.svbk & 0x07)
                } else {
                    0xFF
                }
            }
            0xFF72 => self.undoc_ff72,
            0xFF73 => self.undoc_ff73,
            0xFF74 => {
                if self.cgb_mode {
                    self.undoc_ff74
                } else {
                    0xFF
                }
            }
            0xFF75 => 0x8F | (self.undoc_ff75 & 0x70),
            _ => 0xFF,
        }
    }

    fn write_io(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF00 => self.joypad.write(val),
            0xFF01..=0xFF02 => self.serial.write(addr, val),
            0xFF04..=0xFF07 => self.timer.write(addr, val),
            0xFF0F => self.interrupts.write_if(val),
            0xFF10..=0xFF3F => self.apu.write(addr, val),
            0xFF46 => self.do_oam_dma(val),
            0xFF40..=0xFF4B => self.ppu.write_reg(addr, val),
            0xFF4D => {
                if self.cgb_mode {
                    // Only bit 0 (request) is writable.
                    self.key1 = (self.key1 & 0x80) | (val & 0x01);
                }
            }
            0xFF4F => self.ppu.write_reg(addr, val),
            0xFF50 if val != 0 => self.boot_rom_enabled = false,
            0xFF51..=0xFF54 => {
                if self.cgb_mode {
                    self.hdma.write(addr, val);
                }
            }
            0xFF55 => {
                if self.cgb_mode {
                    match self.hdma.write_ff55(val) {
                        HdmaWrite::General { len } => self.do_gp_hdma(len),
                        HdmaWrite::HBlankStart => {
                            // Drain any stale edge so the first block waits
                            // for the *next* HBlank.
                            let _ = self.ppu.take_hblank_edge();
                        }
                        HdmaWrite::HBlankCancel => {}
                    }
                }
            }
            0xFF68..=0xFF6C => self.ppu.write_reg(addr, val),
            0xFF70 => {
                if self.cgb_mode {
                    self.svbk = val & 0x07;
                }
            }
            0xFF72 => self.undoc_ff72 = val,
            0xFF73 => self.undoc_ff73 = val,
            0xFF74 => {
                if self.cgb_mode {
                    self.undoc_ff74 = val;
                }
            }
            0xFF75 => self.undoc_ff75 = val & 0x70,
            _ => {}
        }
    }

    /// OAM DMA (`FF46`). Copies 160 bytes from `src<<8` into OAM. We do this
    /// synchronously — the real hardware takes 160 M-cycles and locks most of
    /// the bus, but acid2 doesn't probe that timing.
    fn do_oam_dma(&mut self, src: u8) {
        let base = (src as u16) << 8;
        for i in 0..0xA0u16 {
            let byte = self.read(base + i);
            self.ppu.oam_dma_write(i as u8, byte);
        }
    }

    /// General-purpose HDMA: copy `len * 16` bytes from the controller's
    /// source into VRAM immediately.
    fn do_gp_hdma(&mut self, blocks: u8) {
        let src_base = self.hdma.source();
        let dst_base = self.hdma.dest();
        let total = blocks as u16 * 16;
        for i in 0..total {
            let b = self.read(src_base.wrapping_add(i));
            self.ppu.write_vram(dst_base.wrapping_add(i), b);
        }
    }
}
