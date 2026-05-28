//! Joypad — register `P1/JOYP` at `0xFF00`.

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct JoypadState: u8 {
        const RIGHT  = 1 << 0;
        const LEFT   = 1 << 1;
        const UP     = 1 << 2;
        const DOWN   = 1 << 3;
        const A      = 1 << 4;
        const B      = 1 << 5;
        const SELECT = 1 << 6;
        const START  = 1 << 7;
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Button {
    Right, Left, Up, Down, A, B, Select, Start,
}

#[derive(Debug, Default)]
pub struct Joypad {
    state: JoypadState,
    select_buttons: bool,
    select_dpad: bool,
}

impl Joypad {
    pub fn new() -> Self { Self::default() }

    pub fn set_state(&mut self, state: JoypadState) { self.state = state; }

    pub fn read(&self) -> u8 {
        // Hardware reads 0 when a button is pressed.
        let mut nibble = 0x0F;
        if self.select_dpad {
            if self.state.contains(JoypadState::RIGHT) { nibble &= !0x01; }
            if self.state.contains(JoypadState::LEFT)  { nibble &= !0x02; }
            if self.state.contains(JoypadState::UP)    { nibble &= !0x04; }
            if self.state.contains(JoypadState::DOWN)  { nibble &= !0x08; }
        }
        if self.select_buttons {
            if self.state.contains(JoypadState::A)      { nibble &= !0x01; }
            if self.state.contains(JoypadState::B)      { nibble &= !0x02; }
            if self.state.contains(JoypadState::SELECT) { nibble &= !0x04; }
            if self.state.contains(JoypadState::START)  { nibble &= !0x08; }
        }
        let select_bits = (!(self.select_buttons as u8) & 1) << 5
                        | (!(self.select_dpad    as u8) & 1) << 4;
        0xC0 | select_bits | nibble
    }
    pub fn write(&mut self, val: u8) {
        // Bits 4 (dpad) and 5 (buttons) are active-low select lines.
        self.select_dpad    = (val & 0x10) == 0;
        self.select_buttons = (val & 0x20) == 0;
    }
}
