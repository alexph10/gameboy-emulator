//! Memory bus / MMU.
//!
//! Routes CPU memory accesses to the appropriate subsystem per the DMG memory
//! map (`docs/memory-map.md`) and ticks non-CPU subsystems in T-cycles.

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::interrupts::Interrupts;
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

    wram: [u8; 0x2000], // 8 KiB
    hram: [u8; 0x7F],   // 0xFF80–0xFFFE

    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
}

impl Bus {
    pub fn new(cart: Cartridge, boot_rom: Option<Vec<u8>>) -> Self {
        let boot_rom_enabled = boot_rom.is_some();
        Self {
            cart,
            ppu: Ppu::new(),
            apu: Apu::new(),
            timer: Timer::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            interrupts: Interrupts::new(),
            wram: [0; 0x2000],
            hram: [0; 0x7F],
            boot_rom,
            boot_rom_enabled,
        }
    }

    pub fn ppu(&self) -> &Ppu {
        &self.ppu
    }

    /// Advance non-CPU subsystems by `t_cycles` T-cycles.
    pub fn tick(&mut self, t_cycles: u32) {
        self.timer.tick(t_cycles, &mut self.interrupts);
        self.ppu.tick(t_cycles, &mut self.interrupts);
        self.apu.tick(t_cycles);
    }

    /// 8-bit read from the CPU.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            // Boot ROM overlay
            0x0000..=0x00FF if self.boot_rom_enabled => {
                self.boot_rom.as_ref().map_or(0xFF, |b| b[addr as usize])
            }
            0x0000..=0x7FFF => self.cart.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read_ram(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize], // echo
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
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = val,
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize] = val,
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, val),
            0xFEA0..=0xFEFF => {} // prohibited
            0xFF00..=0xFF7F => self.write_io(addr, val),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.interrupts.write_ie(val),
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
            0xFF50 => 0xFF,
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
            0xFF40..=0xFF4B => self.ppu.write_reg(addr, val),
            0xFF50 if val != 0 => self.boot_rom_enabled = false,
            _ => {}
        }
    }
}
