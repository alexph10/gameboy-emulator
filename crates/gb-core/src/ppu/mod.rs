//! Picture Processing Unit. See `docs/ppu.md`.

use crate::interrupts::Interrupts;

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

/// Indexed-color (0..=3) framebuffer; frontends apply palettes.
pub type FrameBuffer = [u8; SCREEN_WIDTH * SCREEN_HEIGHT];

#[derive(Debug)]
pub struct Ppu {
    vram: [u8; 0x2000],
    oam: [u8; 0xA0],
    frame: FrameBuffer,
    // TODO: LCDC, STAT, SCY, SCX, LY, LYC, BGP, OBP0, OBP1, WY, WX, dot counter…
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            vram: [0; 0x2000],
            oam: [0; 0xA0],
            frame: [0; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    pub fn frame_buffer(&self) -> &FrameBuffer {
        &self.frame
    }

    pub fn tick(&mut self, _t_cycles: u32, _ints: &mut Interrupts) {
        // TODO: scanline timing, mode transitions, FIFO, interrupts.
    }

    pub fn read_vram(&self, addr: u16) -> u8 {
        self.vram[(addr - 0x8000) as usize]
    }
    pub fn write_vram(&mut self, addr: u16, val: u8) {
        self.vram[(addr - 0x8000) as usize] = val;
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[(addr - 0xFE00) as usize]
    }
    pub fn write_oam(&mut self, addr: u16, val: u8) {
        self.oam[(addr - 0xFE00) as usize] = val;
    }

    pub fn read_reg(&self, _addr: u16) -> u8 {
        // TODO: dispatch by address.
        0xFF
    }
    pub fn write_reg(&mut self, _addr: u16, _val: u8) {
        // TODO
    }
}

impl Default for Ppu {
    fn default() -> Self { Self::new() }
}
