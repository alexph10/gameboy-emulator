//! CGB HDMA controller — registers `FF51..=FF55`.
//!
//! Two transfer modes share the same source/dest/length registers:
//!
//! * **General-purpose** (FF55 bit 7 = 0): copy all `(len+1) * 16` bytes
//!   immediately when FF55 is written, then mark transfer complete.
//! * **HBlank** (FF55 bit 7 = 1): copy 16 bytes during each HBlank; the
//!   length counts down by one block per HBlank until exhausted.
//!
//! For non-cycle-accurate emulation the copies happen synchronously; the CPU
//! is not stalled. This is sufficient for the cgb-acid2 test (which does not
//! exercise HDMA at all) and works for all known commercial CGB titles.

#[derive(Debug)]
pub struct Hdma {
    src_hi: u8,
    src_lo: u8,
    dst_hi: u8,
    dst_lo: u8,
    /// Remaining 16-byte blocks for an active HBlank transfer.
    blocks_remaining: u8,
    /// `true` while an HBlank-mode transfer is active.
    hblank_active: bool,
    /// Mirrors what the CPU last read at FF55: bit 7 = "not active", bits 0..6
    /// = blocks_remaining - 1. After a completed transfer, latched as 0xFF.
    last_status: u8,
}

impl Hdma {
    pub fn new() -> Self {
        Self {
            src_hi: 0,
            src_lo: 0,
            dst_hi: 0,
            dst_lo: 0,
            blocks_remaining: 0,
            hblank_active: false,
            last_status: 0xFF,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF51 => self.src_hi,
            0xFF52 => self.src_lo,
            0xFF53 => self.dst_hi,
            0xFF54 => self.dst_lo,
            0xFF55 => self.last_status,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF51 => self.src_hi = val,
            0xFF52 => self.src_lo = val & 0xF0,
            0xFF53 => self.dst_hi = (val & 0x1F) | 0x80,
            0xFF54 => self.dst_lo = val & 0xF0,
            _ => {}
        }
    }

    /// Source base address (16-byte aligned, must point at `0x0000..0x7FFF`
    /// ROM or `0xA000..0xDFFF` cart/WRAM).
    pub fn source(&self) -> u16 {
        ((self.src_hi as u16) << 8) | (self.src_lo as u16)
    }

    /// Destination base address inside VRAM (always `0x8000..0x9FFF`).
    pub fn dest(&self) -> u16 {
        ((self.dst_hi as u16) << 8) | (self.dst_lo as u16)
    }

    /// Called by the bus when FF55 is written.
    ///
    /// Returns:
    /// * `HdmaWrite::General { len }` — copy `len * 16` bytes immediately.
    /// * `HdmaWrite::HBlankStart` — set up periodic HBlank transfer.
    /// * `HdmaWrite::HBlankCancel` — caller should stop any in-progress
    ///   transfer and update status appropriately.
    pub fn write_ff55(&mut self, val: u8) -> HdmaWrite {
        let blocks = (val & 0x7F) + 1; // 1..=128
        if (val & 0x80) == 0 {
            // Bit 7 = 0
            if self.hblank_active {
                // Stop current HBlank transfer. Status bit 7 latches to 1 so
                // a subsequent read of FF55 indicates "stopped" while keeping
                // the remaining-block count.
                self.hblank_active = false;
                let remaining = self.blocks_remaining.saturating_sub(1);
                self.last_status = 0x80 | (remaining & 0x7F);
                HdmaWrite::HBlankCancel
            } else {
                self.last_status = 0xFF;
                HdmaWrite::General { len: blocks }
            }
        } else {
            self.hblank_active = true;
            self.blocks_remaining = blocks;
            // While active, last_status bit 7 = 0 + length-1.
            self.last_status = (blocks - 1) & 0x7F;
            HdmaWrite::HBlankStart
        }
    }

    /// Called on each HBlank entry while `is_hblank_active()`. Advances the
    /// source/dest pointers by 16 bytes and returns the count of blocks
    /// remaining *after* this one (0 → caller copies, then transfer ends).
    /// Returns `None` when no transfer is active.
    pub fn step_hblank(&mut self) -> Option<HdmaBlock> {
        if !self.hblank_active {
            return None;
        }
        let src = self.source();
        let dst = self.dest();
        // Advance pointers for next time.
        let new_src = src.wrapping_add(16);
        self.src_hi = (new_src >> 8) as u8;
        self.src_lo = (new_src as u8) & 0xF0;
        let new_dst = dst.wrapping_add(16);
        // Destination stays in 0x8000..0xA000.
        self.dst_hi = ((new_dst >> 8) as u8 & 0x1F) | 0x80;
        self.dst_lo = (new_dst as u8) & 0xF0;

        self.blocks_remaining -= 1;
        if self.blocks_remaining == 0 {
            self.hblank_active = false;
            self.last_status = 0xFF;
        } else {
            self.last_status = (self.blocks_remaining - 1) & 0x7F;
        }
        Some(HdmaBlock { src, dst })
    }

    pub fn is_hblank_active(&self) -> bool {
        self.hblank_active
    }
}

impl Default for Hdma {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HdmaWrite {
    General { len: u8 },
    HBlankStart,
    HBlankCancel,
}

#[derive(Debug, Clone, Copy)]
pub struct HdmaBlock {
    pub src: u16,
    pub dst: u16,
}
