//! Per-scanline renderer for the DMG/CGB PPU.
//!
//! Line-based (not pixel-FIFO). Both acid2 ROMs only rewrite registers during
//! mode 2 / VBlank so reading them at the *end* of mode 3 produces the same
//! image as a cycle-accurate fetcher would.
//!
//! In DMG mode the framebuffer is filled with one of four greyscale RGB555
//! values; in CGB mode it is filled directly from BG/OBJ CRAM.

use super::regs::Lcdc;
use super::{Ppu, DMG_SHADE_RGB555, SCREEN_WIDTH};

/// Per-pixel BG/window info, used for sprite priority resolution.
#[derive(Clone, Copy, Default)]
struct BgInfo {
    /// Pre-palette color index (0..=3). 0 means "transparent under sprites".
    color: u8,
    /// CGB only: BG attribute bit 7 (BG-to-OAM priority) for this pixel.
    bg_priority: bool,
}

pub(super) fn render_scanline(ppu: &mut Ppu) {
    let ly = ppu.ly;
    if ly >= 144 {
        return;
    }
    let lcdc = Lcdc(ppu.lcdc);

    let mut bg_line = [BgInfo::default(); SCREEN_WIDTH];

    if ppu.cgb_mode {
        render_bg_and_window_cgb(ppu, lcdc, ly, &mut bg_line);
    } else {
        render_bg_and_window_dmg(ppu, lcdc, ly, &mut bg_line);
    }

    if lcdc.obj_enabled() {
        if ppu.cgb_mode {
            render_sprites_cgb(ppu, lcdc, ly, &bg_line);
        } else {
            render_sprites_dmg(ppu, lcdc, ly, &bg_line);
        }
    }
}

// ============================================================================
// DMG path
// ============================================================================

fn render_bg_and_window_dmg(ppu: &mut Ppu, lcdc: Lcdc, ly: u8, bg_line: &mut [BgInfo]) {
    let bgp = ppu.bgp;
    let bg_on = lcdc.bg_enabled();
    let window_on = lcdc.window_enabled() && bg_on; // DMG: LCDC.0 also gates window
    let window_visible_this_line = window_on && ly >= ppu.wy && ppu.wx < 167;

    for x in 0..SCREEN_WIDTH as u8 {
        let use_window = window_visible_this_line && (x + 7) >= ppu.wx;

        let color_idx: u8 = if use_window {
            let win_x = (x + 7) - ppu.wx;
            let win_y = ppu.window_line_counter;
            fetch_bg_tile_color_dmg(ppu, lcdc, win_x, win_y, lcdc.win_map_hi())
        } else if bg_on {
            let bgx = ppu.scx.wrapping_add(x);
            let bgy = ppu.scy.wrapping_add(ly);
            fetch_bg_tile_color_dmg(ppu, lcdc, bgx, bgy, lcdc.bg_map_hi())
        } else {
            0
        };

        bg_line[x as usize] = BgInfo { color: color_idx, bg_priority: false };
        let shade = apply_palette(bgp, color_idx);
        ppu.frame[ly as usize * SCREEN_WIDTH + x as usize] = DMG_SHADE_RGB555[shade as usize];
    }

    if window_visible_this_line {
        ppu.window_line_counter = ppu.window_line_counter.wrapping_add(1);
    }
}

/// DMG BG/window pixel fetch — bank 0 only, no per-tile attribute.
fn fetch_bg_tile_color_dmg(ppu: &Ppu, lcdc: Lcdc, tx: u8, ty: u8, map_hi: bool) -> u8 {
    let map_base: u16 = if map_hi { 0x9C00 } else { 0x9800 };
    let map_x = (tx / 8) as u16;
    let map_y = (ty / 8) as u16;
    let tile_id = ppu.read_vram_bank(0, map_base + map_y * 32 + map_x);
    let tile_addr = bg_tile_addr(lcdc, tile_id);
    let line = (ty % 8) as u16;
    let lo = ppu.read_vram_bank(0, tile_addr + line * 2);
    let hi = ppu.read_vram_bank(0, tile_addr + line * 2 + 1);
    let bit = 7 - (tx % 8);
    let l = (lo >> bit) & 1;
    let h = (hi >> bit) & 1;
    (h << 1) | l
}

#[inline]
fn bg_tile_addr(lcdc: Lcdc, tile_id: u8) -> u16 {
    if lcdc.bg_data_8000() {
        0x8000u16.wrapping_add((tile_id as u16) * 16)
    } else {
        let signed = tile_id as i8 as i16;
        (0x9000i32 + (signed as i32) * 16) as u16
    }
}

#[inline]
fn apply_palette(palette: u8, color: u8) -> u8 {
    (palette >> (color * 2)) & 0b11
}

fn render_sprites_dmg(ppu: &mut Ppu, lcdc: Lcdc, ly: u8, bg_line: &[BgInfo]) {
    let sprite_h: u8 = if lcdc.obj_8x16() { 16 } else { 8 };
    let mut visible: [(u8, u8, u8, u8, u8); 10] = [(0, 0, 0, 0, 0); 10];
    let mut count = 0usize;
    for i in 0..40u8 {
        let base = i as usize * 4;
        let y = ppu.oam[base];
        let x = ppu.oam[base + 1];
        let tile = ppu.oam[base + 2];
        let attr = ppu.oam[base + 3];
        let top = y as i16 - 16;
        let ly_i = ly as i16;
        if ly_i >= top && ly_i < top + sprite_h as i16 {
            visible[count] = (i, y, x, tile, attr);
            count += 1;
            if count == 10 {
                break;
            }
        }
    }

    // DMG priority: lower X wins; ties broken by lower OAM index.
    let mut order: [usize; 10] = [0; 10];
    for (i, slot) in order.iter_mut().enumerate().take(count) {
        *slot = i;
    }
    order[..count].sort_by(|&a, &b| {
        let (ia, _, xa, _, _) = visible[a];
        let (ib, _, xb, _, _) = visible[b];
        xb.cmp(&xa).then(ib.cmp(&ia))
    });

    for &slot in &order[..count] {
        let (_oam_idx, y, x, mut tile, attr) = visible[slot];
        let flip_x = attr & 0x20 != 0;
        let flip_y = attr & 0x40 != 0;
        let priority_bg = attr & 0x80 != 0;
        let palette = if attr & 0x10 != 0 { ppu.obp1 } else { ppu.obp0 };

        let mut row = (ly as i16 - (y as i16 - 16)) as u8;
        if flip_y {
            row = sprite_h - 1 - row;
        }
        if lcdc.obj_8x16() {
            tile &= 0xFE;
            if row >= 8 {
                tile |= 0x01;
                row -= 8;
            }
        }

        let tile_addr = 0x8000u16 + (tile as u16) * 16;
        let lo = ppu.read_vram_bank(0, tile_addr + row as u16 * 2);
        let hi = ppu.read_vram_bank(0, tile_addr + row as u16 * 2 + 1);

        for px in 0..8u8 {
            let screen_x = x as i16 - 8 + px as i16;
            if !(0..SCREEN_WIDTH as i16).contains(&screen_x) {
                continue;
            }
            let bit = if flip_x { px } else { 7 - px };
            let l = (lo >> bit) & 1;
            let h = (hi >> bit) & 1;
            let color = (h << 1) | l;
            if color == 0 {
                continue;
            }
            let sx = screen_x as usize;
            if priority_bg && bg_line[sx].color != 0 {
                continue;
            }
            let shade = apply_palette(palette, color);
            ppu.frame[ly as usize * SCREEN_WIDTH + sx] = DMG_SHADE_RGB555[shade as usize];
        }
    }
}

// ============================================================================
// CGB path
// ============================================================================

fn render_bg_and_window_cgb(ppu: &mut Ppu, lcdc: Lcdc, ly: u8, bg_line: &mut [BgInfo]) {
    // In CGB mode BG and window are always drawn regardless of LCDC.0.
    // (LCDC.0 in CGB is the master-priority bit, handled when blending sprites.)
    let window_on = lcdc.window_enabled();
    let window_visible_this_line = window_on && ly >= ppu.wy && ppu.wx < 167;

    for x in 0..SCREEN_WIDTH as u8 {
        let use_window = window_visible_this_line && (x + 7) >= ppu.wx;

        let (color_idx, attr) = if use_window {
            let win_x = (x + 7) - ppu.wx;
            let win_y = ppu.window_line_counter;
            fetch_bg_tile_color_cgb(ppu, lcdc, win_x, win_y, lcdc.win_map_hi())
        } else {
            let bgx = ppu.scx.wrapping_add(x);
            let bgy = ppu.scy.wrapping_add(ly);
            fetch_bg_tile_color_cgb(ppu, lcdc, bgx, bgy, lcdc.bg_map_hi())
        };

        let palette_num = (attr & 0x07) as usize;
        let cram_off = palette_num * 8 + (color_idx as usize) * 2;
        let lo = ppu.bg_cram[cram_off] as u16;
        let hi = ppu.bg_cram[cram_off + 1] as u16;
        let rgb555 = ((hi << 8) | lo) & 0x7FFF;

        bg_line[x as usize] = BgInfo {
            color: color_idx,
            bg_priority: (attr & 0x80) != 0,
        };
        ppu.frame[ly as usize * SCREEN_WIDTH + x as usize] = rgb555;
    }

    if window_visible_this_line {
        ppu.window_line_counter = ppu.window_line_counter.wrapping_add(1);
    }
}

/// CGB BG/window fetch. Returns (pre-palette color index, attribute byte).
fn fetch_bg_tile_color_cgb(ppu: &Ppu, lcdc: Lcdc, tx: u8, ty: u8, map_hi: bool) -> (u8, u8) {
    let map_base: u16 = if map_hi { 0x9C00 } else { 0x9800 };
    let map_off = (ty / 8) as u16 * 32 + (tx / 8) as u16;
    let tile_id = ppu.read_vram_bank(0, map_base + map_off);
    let attr = ppu.read_vram_bank(1, map_base + map_off);

    let tile_bank = (attr >> 3) & 1;
    let flip_x = attr & 0x20 != 0;
    let flip_y = attr & 0x40 != 0;

    let tile_addr = bg_tile_addr(lcdc, tile_id);
    let mut line = (ty % 8) as u16;
    if flip_y {
        line = 7 - line;
    }
    let lo = ppu.read_vram_bank(tile_bank, tile_addr + line * 2);
    let hi = ppu.read_vram_bank(tile_bank, tile_addr + line * 2 + 1);
    let bit_idx = if flip_x { tx % 8 } else { 7 - (tx % 8) };
    let l = (lo >> bit_idx) & 1;
    let h = (hi >> bit_idx) & 1;
    ((h << 1) | l, attr)
}

fn render_sprites_cgb(ppu: &mut Ppu, lcdc: Lcdc, ly: u8, bg_line: &[BgInfo]) {
    let sprite_h: u8 = if lcdc.obj_8x16() { 16 } else { 8 };
    let mut visible: [(u8, u8, u8, u8, u8); 10] = [(0, 0, 0, 0, 0); 10];
    let mut count = 0usize;
    for i in 0..40u8 {
        let base = i as usize * 4;
        let y = ppu.oam[base];
        let x = ppu.oam[base + 1];
        let tile = ppu.oam[base + 2];
        let attr = ppu.oam[base + 3];
        let top = y as i16 - 16;
        let ly_i = ly as i16;
        if ly_i >= top && ly_i < top + sprite_h as i16 {
            visible[count] = (i, y, x, tile, attr);
            count += 1;
            if count == 10 {
                break;
            }
        }
    }

    // Sprite drawing order — lowest priority drawn first.
    // OPRI=0 (CGB): priority by OAM index, lower index wins.
    // OPRI=1 (DMG): priority by X, lower X wins, ties by lower OAM index.
    let mut order: [usize; 10] = [0; 10];
    for (i, slot) in order.iter_mut().enumerate().take(count) {
        *slot = i;
    }
    if (ppu.opri & 1) != 0 {
        order[..count].sort_by(|&a, &b| {
            let (ia, _, xa, _, _) = visible[a];
            let (ib, _, xb, _, _) = visible[b];
            xb.cmp(&xa).then(ib.cmp(&ia))
        });
    } else {
        // Lower OAM index = higher priority → draw it last.
        order[..count].sort_by(|&a, &b| {
            let (ia, _, _, _, _) = visible[a];
            let (ib, _, _, _, _) = visible[b];
            ib.cmp(&ia)
        });
    }

    // LCDC.0 = "Master priority". When 1, BG-to-OAM priority bits (BG attr.7
    // and OAM attr.7) are honored. When 0, sprites are always on top.
    let master_priority = (ppu.lcdc & 0x01) != 0;

    for &slot in &order[..count] {
        let (_oam_idx, y, x, mut tile, attr) = visible[slot];
        let flip_x = attr & 0x20 != 0;
        let flip_y = attr & 0x40 != 0;
        let obj_priority_bg = attr & 0x80 != 0;
        let palette_num = (attr & 0x07) as usize;
        let tile_bank = (attr >> 3) & 1;

        let mut row = (ly as i16 - (y as i16 - 16)) as u8;
        if flip_y {
            row = sprite_h - 1 - row;
        }
        if lcdc.obj_8x16() {
            tile &= 0xFE;
            if row >= 8 {
                tile |= 0x01;
                row -= 8;
            }
        }

        let tile_addr = 0x8000u16 + (tile as u16) * 16;
        let lo = ppu.read_vram_bank(tile_bank, tile_addr + row as u16 * 2);
        let hi = ppu.read_vram_bank(tile_bank, tile_addr + row as u16 * 2 + 1);

        for px in 0..8u8 {
            let screen_x = x as i16 - 8 + px as i16;
            if !(0..SCREEN_WIDTH as i16).contains(&screen_x) {
                continue;
            }
            let bit = if flip_x { px } else { 7 - px };
            let l = (lo >> bit) & 1;
            let h = (hi >> bit) & 1;
            let color = (h << 1) | l;
            if color == 0 {
                continue;
            }
            let sx = screen_x as usize;
            let bg = bg_line[sx];

            // CGB priority resolution:
            //   if !master_priority → sprite always wins
            //   else if BG color == 0 → sprite always wins (BG is transparent)
            //   else if BG-attr.7 or OAM.7 set → BG wins
            //   else sprite wins
            if master_priority
                && bg.color != 0
                && (bg.bg_priority || obj_priority_bg)
            {
                continue;
            }

            let cram_off = palette_num * 8 + (color as usize) * 2;
            let lo_c = ppu.obj_cram[cram_off] as u16;
            let hi_c = ppu.obj_cram[cram_off + 1] as u16;
            let rgb555 = ((hi_c << 8) | lo_c) & 0x7FFF;
            ppu.frame[ly as usize * SCREEN_WIDTH + sx] = rgb555;
        }
    }
}
