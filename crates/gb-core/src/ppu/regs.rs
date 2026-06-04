//! PPU register bit decoders.
//!
//! These are pure helpers around `u8` values that match the names used in the
//! Pan Docs PPU chapter so call sites read like the spec.

/// LCDC (`FF40`) — LCD control. See Pan Docs § LCD Control.
#[derive(Debug, Clone, Copy)]
pub struct Lcdc(pub u8);

impl Lcdc {
    /// Window tile map area: false → `0x9800`, true → `0x9C00`.
    pub fn win_map_hi(self) -> bool {
        self.0 & 0x40 != 0
    }
    pub fn window_enabled(self) -> bool {
        self.0 & 0x20 != 0
    }
    /// BG/Window tile data area: true → `0x8000` (unsigned), false → `0x8800` (signed).
    pub fn bg_data_8000(self) -> bool {
        self.0 & 0x10 != 0
    }
    /// BG tile map area: false → `0x9800`, true → `0x9C00`.
    pub fn bg_map_hi(self) -> bool {
        self.0 & 0x08 != 0
    }
    /// Sprite size: false → 8×8, true → 8×16.
    pub fn obj_8x16(self) -> bool {
        self.0 & 0x04 != 0
    }
    pub fn obj_enabled(self) -> bool {
        self.0 & 0x02 != 0
    }
    /// BG/Window display enable. When 0, BG+window are forced to color 0.
    pub fn bg_enabled(self) -> bool {
        self.0 & 0x01 != 0
    }
}

/// STAT (`FF41`) — LCD status. Upper bits select STAT interrupt sources.
#[derive(Debug, Clone, Copy)]
pub struct Stat(pub u8);

impl Stat {
    pub fn lyc_irq(self) -> bool {
        self.0 & 0x40 != 0
    }
    pub fn mode2_irq(self) -> bool {
        self.0 & 0x20 != 0
    }
    pub fn mode1_irq(self) -> bool {
        self.0 & 0x10 != 0
    }
    pub fn mode0_irq(self) -> bool {
        self.0 & 0x08 != 0
    }
}
