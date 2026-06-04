//! Picture Processing Unit (PPU) — line-based DMG/CGB renderer.
//!
//! See `docs/ppu.md`. This implementation is per-scanline, not pixel-FIFO; it
//! is sufficient for dmg-acid2 and cgb-acid2 but does **not** model
//! mid-mode-3 register writes (Mealybug Tearoom). Both acid2 ROMs only
//! rewrite registers during mode 2 / VBlank so the snapshot taken at the end
//! of mode 3 matches hardware.
//!
//! **Framebuffer format.** Always 15-bit RGB555 packed into a `u16`:
//! `0 BBBBB GGGGG RRRRR`. DMG mode applies a fixed greyscale palette before
//! writing; CGB mode reads BG/OBJ CRAM. Frontends convert to ARGB8888 via
//! `(c << 3) | (c >> 2)` per channel.

mod regs;
mod render;

use crate::interrupts::{IntFlags, Interrupts};
use regs::Stat;

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

/// 15-bit RGB555 framebuffer: `0 BBBBB GGGGG RRRRR` (bit 15 unused).
/// Frontends expand each channel with `(c << 3) | (c >> 2)`.
pub type FrameBuffer = [u16; SCREEN_WIDTH * SCREEN_HEIGHT];

const DOTS_PER_LINE: u32 = 456;
const MODE2_DOTS: u32 = 80;
const MODE3_DOTS: u32 = 172; // fixed (acid2 doesn't change SCX mid-line)
const LINES_PER_FRAME: u8 = 154;

/// Fixed DMG → RGB555 greyscale palette, indexed by 2-bit shade.
pub(super) const DMG_SHADE_RGB555: [u16; 4] = [
    rgb555(31, 31, 31), // 0 = white
    rgb555(21, 21, 21), // 1 = light grey
    rgb555(10, 10, 10), // 2 = dark grey
    rgb555(0, 0, 0),    // 3 = black
];

#[inline]
pub(super) const fn rgb555(r: u8, g: u8, b: u8) -> u16 {
    ((b as u16) << 10) | ((g as u16) << 5) | (r as u16)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    HBlank = 0,
    VBlank = 1,
    OamScan = 2,
    Drawing = 3,
}

#[derive(Debug)]
pub struct Ppu {
    /// CGB mode enabled (decided at boot from cartridge header / options).
    pub(super) cgb_mode: bool,

    /// Two 8 KiB VRAM banks. Bank 0 = `0x0000..0x2000`, bank 1 = `0x2000..0x4000`.
    /// In DMG mode only bank 0 is used.
    pub(super) vram: [u8; 0x4000],
    pub(super) vbk: u8, // FF4F — bit 0 selects active bank
    pub(super) oam: [u8; 0xA0],
    pub(super) frame: FrameBuffer,

    // Registers (FF40..=FF4B; FF46 DMA is handled at the Bus level).
    pub(super) lcdc: u8,
    stat: u8, // upper bits only (3..=6); mode + coincidence are computed.
    pub(super) scy: u8,
    pub(super) scx: u8,
    ly: u8,
    lyc: u8,
    pub(super) bgp: u8,
    pub(super) obp0: u8,
    pub(super) obp1: u8,
    pub(super) wy: u8,
    pub(super) wx: u8,

    // CGB color RAM and indexing.
    pub(super) bg_cram: [u8; 64],
    pub(super) obj_cram: [u8; 64],
    pub(super) bcps: u8, // FF68
    pub(super) ocps: u8, // FF6A
    /// FF6C — OBJ priority mode. Bit 0: 0 = CGB (by OAM index), 1 = DMG (by X).
    pub(super) opri: u8,

    // Timing / state.
    mode: Mode,
    line_dot: u32, // 0..456
    /// Window's internal Y counter; only advances on lines where window draws.
    pub(super) window_line_counter: u8,
    /// Edge state for the unified STAT IRQ line.
    stat_line: bool,
    /// Rising edge of VBlank — used by `Gameboy::run_frame` to know when a
    /// settled framebuffer is ready.
    frame_ready: bool,
    /// Rising edge of HBlank entry — consumed by HDMA (CGB).
    hblank_edge: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Self::with_mode(false)
    }

    pub fn with_mode(cgb_mode: bool) -> Self {
        Self {
            cgb_mode,
            vram: [0; 0x4000],
            vbk: 0,
            oam: [0; 0xA0],
            frame: [0; SCREEN_WIDTH * SCREEN_HEIGHT],

            // Post-boot defaults (Pan Docs — Power Up Sequence).
            lcdc: 0x91,
            stat: 0x00,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,

            bg_cram: [0xFF; 64],
            obj_cram: [0xFF; 64],
            bcps: 0,
            ocps: 0,
            // OPRI default 0 (CGB priority by OAM index) — cgb-acid2's "mole"
            // section relies on this.
            opri: 0,

            mode: Mode::OamScan,
            line_dot: 0,
            window_line_counter: 0,
            stat_line: false,
            frame_ready: false,
            hblank_edge: false,
        }
    }

    pub fn frame_buffer(&self) -> &FrameBuffer {
        &self.frame
    }

    pub fn cgb_mode(&self) -> bool {
        self.cgb_mode
    }

    /// True (once) when the PPU has just finished a frame. Reading clears it.
    pub fn take_frame_ready(&mut self) -> bool {
        let v = self.frame_ready;
        self.frame_ready = false;
        v
    }

    /// True (once) when the PPU has just entered HBlank for a visible line.
    /// Consumed by the CGB HDMA controller.
    pub fn take_hblank_edge(&mut self) -> bool {
        let v = self.hblank_edge;
        self.hblank_edge = false;
        v
    }

    pub fn tick(&mut self, t_cycles: u32, ints: &mut Interrupts) {
        if (self.lcdc & 0x80) == 0 {
            return; // LCD off
        }
        for _ in 0..t_cycles {
            self.tick_one_dot(ints);
        }
    }

    fn tick_one_dot(&mut self, ints: &mut Interrupts) {
        self.line_dot += 1;

        match self.mode {
            Mode::OamScan if self.line_dot == MODE2_DOTS => {
                self.mode = Mode::Drawing;
            }
            Mode::Drawing if self.line_dot == MODE2_DOTS + MODE3_DOTS => {
                render::render_scanline(self);
                self.mode = Mode::HBlank;
                self.hblank_edge = true;
            }
            Mode::HBlank if self.line_dot == DOTS_PER_LINE => {
                self.line_dot = 0;
                self.ly += 1;
                if self.ly == 144 {
                    self.mode = Mode::VBlank;
                    self.frame_ready = true;
                    ints.request(IntFlags::VBLANK);
                } else {
                    self.mode = Mode::OamScan;
                }
            }
            Mode::VBlank if self.line_dot == DOTS_PER_LINE => {
                self.line_dot = 0;
                self.ly += 1;
                if self.ly >= LINES_PER_FRAME {
                    self.ly = 0;
                    self.mode = Mode::OamScan;
                    self.window_line_counter = 0;
                }
            }
            _ => {}
        }

        self.update_stat_line(ints);
    }

    /// STAT IRQ is the rising edge of `(any enabled source active)` —
    /// matches real-hardware STAT-blocking and avoids double-triggering on
    /// successive dots that share the same condition (e.g. LY==LYC for an
    /// entire scanline).
    fn update_stat_line(&mut self, ints: &mut Interrupts) {
        let s = Stat(self.stat);
        let coincidence = self.ly == self.lyc;
        let new_line = (s.lyc_irq() && coincidence)
            || (s.mode0_irq() && self.mode == Mode::HBlank)
            || (s.mode1_irq() && self.mode == Mode::VBlank)
            || (s.mode2_irq() && self.mode == Mode::OamScan);

        if new_line && !self.stat_line {
            ints.request(IntFlags::STAT);
        }
        self.stat_line = new_line;
    }

    // ---- VRAM / OAM ----

    #[inline]
    pub(super) fn vram_bank_base(&self) -> usize {
        if self.cgb_mode && (self.vbk & 1) != 0 {
            0x2000
        } else {
            0
        }
    }

    pub fn read_vram(&self, addr: u16) -> u8 {
        let off = (addr - 0x8000) as usize;
        self.vram[self.vram_bank_base() + off]
    }
    pub fn write_vram(&mut self, addr: u16, val: u8) {
        let off = (addr - 0x8000) as usize;
        let base = self.vram_bank_base();
        self.vram[base + off] = val;
    }

    /// Read VRAM at an explicit bank (used by the renderer for CGB BG attrs
    /// and bank-1 tile data regardless of the CPU-visible VBK selection).
    #[inline]
    pub(super) fn read_vram_bank(&self, bank: u8, addr: u16) -> u8 {
        let off = (addr - 0x8000) as usize;
        let base = if (bank & 1) != 0 { 0x2000 } else { 0 };
        self.vram[base + off]
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[(addr - 0xFE00) as usize]
    }
    pub fn write_oam(&mut self, addr: u16, val: u8) {
        self.oam[(addr - 0xFE00) as usize] = val;
    }
    /// Used by the Bus during OAM DMA (FF46).
    pub fn oam_dma_write(&mut self, offset: u8, val: u8) {
        self.oam[offset as usize] = val;
    }

    // ---- Registers ----

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => {
                let coincidence = if self.ly == self.lyc { 0x04 } else { 0 };
                0x80 | (self.stat & 0x78) | coincidence | (self.mode as u8)
            }
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF46 => 0xFF,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            0xFF4F => {
                if self.cgb_mode {
                    0xFE | (self.vbk & 1)
                } else {
                    0xFF
                }
            }
            0xFF68 => {
                if self.cgb_mode {
                    self.bcps | 0x40
                } else {
                    0xFF
                }
            }
            0xFF69 => {
                if self.cgb_mode {
                    self.bg_cram[(self.bcps & 0x3F) as usize]
                } else {
                    0xFF
                }
            }
            0xFF6A => {
                if self.cgb_mode {
                    self.ocps | 0x40
                } else {
                    0xFF
                }
            }
            0xFF6B => {
                if self.cgb_mode {
                    self.obj_cram[(self.ocps & 0x3F) as usize]
                } else {
                    0xFF
                }
            }
            0xFF6C => {
                if self.cgb_mode {
                    0xFE | (self.opri & 1)
                } else {
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF40 => {
                let was_on = self.lcdc & 0x80 != 0;
                self.lcdc = val;
                let now_on = self.lcdc & 0x80 != 0;
                if was_on && !now_on {
                    self.mode = Mode::HBlank;
                    self.ly = 0;
                    self.line_dot = 0;
                    self.stat_line = false;
                    self.window_line_counter = 0;
                } else if !was_on && now_on {
                    self.mode = Mode::OamScan;
                    self.ly = 0;
                    self.line_dot = 0;
                    self.window_line_counter = 0;
                }
            }
            0xFF41 => {
                // Only bits 3..=6 are writable; lower 3 bits + bit 7 are RO.
                self.stat = (self.stat & 0x07) | (val & 0x78);
            }
            0xFF42 => self.scy = val,
            0xFF43 => self.scx = val,
            0xFF44 => {} // LY read-only
            0xFF45 => self.lyc = val,
            0xFF46 => {} // OAM DMA handled at Bus level
            0xFF47 => self.bgp = val,
            0xFF48 => self.obp0 = val,
            0xFF49 => self.obp1 = val,
            0xFF4A => self.wy = val,
            0xFF4B => self.wx = val,
            0xFF4F => {
                if self.cgb_mode {
                    self.vbk = val & 1;
                }
            }
            0xFF68 => {
                if self.cgb_mode {
                    self.bcps = val & 0xBF;
                }
            }
            0xFF69 => {
                if self.cgb_mode {
                    let idx = (self.bcps & 0x3F) as usize;
                    self.bg_cram[idx] = val;
                    if (self.bcps & 0x80) != 0 {
                        let next = (self.bcps + 1) & 0x3F;
                        self.bcps = (self.bcps & 0x80) | next;
                    }
                }
            }
            0xFF6A => {
                if self.cgb_mode {
                    self.ocps = val & 0xBF;
                }
            }
            0xFF6B => {
                if self.cgb_mode {
                    let idx = (self.ocps & 0x3F) as usize;
                    self.obj_cram[idx] = val;
                    if (self.ocps & 0x80) != 0 {
                        let next = (self.ocps + 1) & 0x3F;
                        self.ocps = (self.ocps & 0x80) | next;
                    }
                }
            }
            0xFF6C => {
                if self.cgb_mode {
                    self.opri = val & 1;
                }
            }
            _ => {}
        }
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}
