//! Instruction dispatch — unprefixed (256) + CB-prefixed (256).
//!
//! Cycle counts follow https://gbdev.io/gb-opcodes/optables/ (M-cycles).

use super::alu;
use super::registers::Flags;
use super::Cpu;
use crate::bus::Bus;

#[inline]
fn read_hl(cpu: &Cpu, bus: &Bus) -> u8 {
    bus.read(cpu.regs.hl())
}
#[inline]
fn write_hl(cpu: &Cpu, bus: &mut Bus, v: u8) {
    bus.write(cpu.regs.hl(), v);
}

#[inline]
fn cond_z(cpu: &Cpu) -> bool {
    cpu.regs.f.contains(Flags::Z)
}
#[inline]
fn cond_c(cpu: &Cpu) -> bool {
    cpu.regs.f.contains(Flags::C)
}

/// Execute one instruction. Returns M-cycles consumed.
pub fn step(cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    let op = cpu.fetch8(bus);
    dispatch(cpu, bus, op)
}

fn dispatch(cpu: &mut Cpu, bus: &mut Bus, op: u8) -> u32 {
    match op {
        // 0x00  NOP
        0x00 => 1,

        // 0x01  LD BC, d16
        0x01 => {
            let v = cpu.fetch16(bus);
            cpu.regs.set_bc(v);
            3
        }
        // 0x02  LD (BC), A
        0x02 => {
            bus.write(cpu.regs.bc(), cpu.regs.a);
            2
        }
        // 0x03  INC BC
        0x03 => {
            cpu.regs.set_bc(cpu.regs.bc().wrapping_add(1));
            2
        }
        // 0x04  INC B
        0x04 => {
            cpu.regs.b = alu::inc(cpu.regs.b, &mut cpu.regs.f);
            1
        }
        // 0x05  DEC B
        0x05 => {
            cpu.regs.b = alu::dec(cpu.regs.b, &mut cpu.regs.f);
            1
        }
        // 0x06  LD B, d8
        0x06 => {
            cpu.regs.b = cpu.fetch8(bus);
            2
        }
        // 0x07  RLCA  — Z=0
        0x07 => {
            cpu.regs.a = alu::rlc(cpu.regs.a, &mut cpu.regs.f);
            cpu.regs.f.remove(Flags::Z);
            1
        }
        // 0x08  LD (a16), SP
        0x08 => {
            let addr = cpu.fetch16(bus);
            bus.write(addr, cpu.regs.sp as u8);
            bus.write(addr.wrapping_add(1), (cpu.regs.sp >> 8) as u8);
            5
        }
        // 0x09  ADD HL, BC
        0x09 => {
            let r = alu::add16(cpu.regs.hl(), cpu.regs.bc(), &mut cpu.regs.f);
            cpu.regs.set_hl(r);
            2
        }
        // 0x0A  LD A, (BC)
        0x0A => {
            cpu.regs.a = bus.read(cpu.regs.bc());
            2
        }
        // 0x0B  DEC BC
        0x0B => {
            cpu.regs.set_bc(cpu.regs.bc().wrapping_sub(1));
            2
        }
        // 0x0C  INC C
        0x0C => {
            cpu.regs.c = alu::inc(cpu.regs.c, &mut cpu.regs.f);
            1
        }
        // 0x0D  DEC C
        0x0D => {
            cpu.regs.c = alu::dec(cpu.regs.c, &mut cpu.regs.f);
            1
        }
        // 0x0E  LD C, d8
        0x0E => {
            cpu.regs.c = cpu.fetch8(bus);
            2
        }
        // 0x0F  RRCA  — Z=0
        0x0F => {
            cpu.regs.a = alu::rrc(cpu.regs.a, &mut cpu.regs.f);
            cpu.regs.f.remove(Flags::Z);
            1
        }

        // 0x10  STOP — implemented as 2-byte NOP per spec.
        0x10 => {
            let _ = cpu.fetch8(bus);
            1
        }
        // 0x11  LD DE, d16
        0x11 => {
            let v = cpu.fetch16(bus);
            cpu.regs.set_de(v);
            3
        }
        // 0x12  LD (DE), A
        0x12 => {
            bus.write(cpu.regs.de(), cpu.regs.a);
            2
        }
        // 0x13  INC DE
        0x13 => {
            cpu.regs.set_de(cpu.regs.de().wrapping_add(1));
            2
        }
        // 0x14  INC D
        0x14 => {
            cpu.regs.d = alu::inc(cpu.regs.d, &mut cpu.regs.f);
            1
        }
        // 0x15  DEC D
        0x15 => {
            cpu.regs.d = alu::dec(cpu.regs.d, &mut cpu.regs.f);
            1
        }
        // 0x16  LD D, d8
        0x16 => {
            cpu.regs.d = cpu.fetch8(bus);
            2
        }
        // 0x17  RLA — Z=0
        0x17 => {
            cpu.regs.a = alu::rl(cpu.regs.a, &mut cpu.regs.f);
            cpu.regs.f.remove(Flags::Z);
            1
        }
        // 0x18  JR r8
        0x18 => {
            let off = cpu.fetch8(bus) as i8;
            cpu.regs.pc = cpu.regs.pc.wrapping_add(off as i16 as u16);
            3
        }
        // 0x19  ADD HL, DE
        0x19 => {
            let r = alu::add16(cpu.regs.hl(), cpu.regs.de(), &mut cpu.regs.f);
            cpu.regs.set_hl(r);
            2
        }
        // 0x1A  LD A, (DE)
        0x1A => {
            cpu.regs.a = bus.read(cpu.regs.de());
            2
        }
        // 0x1B  DEC DE
        0x1B => {
            cpu.regs.set_de(cpu.regs.de().wrapping_sub(1));
            2
        }
        // 0x1C  INC E
        0x1C => {
            cpu.regs.e = alu::inc(cpu.regs.e, &mut cpu.regs.f);
            1
        }
        // 0x1D  DEC E
        0x1D => {
            cpu.regs.e = alu::dec(cpu.regs.e, &mut cpu.regs.f);
            1
        }
        // 0x1E  LD E, d8
        0x1E => {
            cpu.regs.e = cpu.fetch8(bus);
            2
        }
        // 0x1F  RRA — Z=0
        0x1F => {
            cpu.regs.a = alu::rr(cpu.regs.a, &mut cpu.regs.f);
            cpu.regs.f.remove(Flags::Z);
            1
        }

        // 0x20  JR NZ, r8
        0x20 => jr_cond(cpu, bus, !cond_z(cpu)),
        // 0x21  LD HL, d16
        0x21 => {
            let v = cpu.fetch16(bus);
            cpu.regs.set_hl(v);
            3
        }
        // 0x22  LD (HL+), A
        0x22 => {
            bus.write(cpu.regs.hl(), cpu.regs.a);
            cpu.regs.set_hl(cpu.regs.hl().wrapping_add(1));
            2
        }
        // 0x23  INC HL
        0x23 => {
            cpu.regs.set_hl(cpu.regs.hl().wrapping_add(1));
            2
        }
        // 0x24  INC H
        0x24 => {
            cpu.regs.h = alu::inc(cpu.regs.h, &mut cpu.regs.f);
            1
        }
        // 0x25  DEC H
        0x25 => {
            cpu.regs.h = alu::dec(cpu.regs.h, &mut cpu.regs.f);
            1
        }
        // 0x26  LD H, d8
        0x26 => {
            cpu.regs.h = cpu.fetch8(bus);
            2
        }
        // 0x27  DAA
        0x27 => {
            cpu.regs.a = alu::daa(cpu.regs.a, &mut cpu.regs.f);
            1
        }
        // 0x28  JR Z, r8
        0x28 => jr_cond(cpu, bus, cond_z(cpu)),
        // 0x29  ADD HL, HL
        0x29 => {
            let r = alu::add16(cpu.regs.hl(), cpu.regs.hl(), &mut cpu.regs.f);
            cpu.regs.set_hl(r);
            2
        }
        // 0x2A  LD A, (HL+)
        0x2A => {
            cpu.regs.a = bus.read(cpu.regs.hl());
            cpu.regs.set_hl(cpu.regs.hl().wrapping_add(1));
            2
        }
        // 0x2B  DEC HL
        0x2B => {
            cpu.regs.set_hl(cpu.regs.hl().wrapping_sub(1));
            2
        }
        // 0x2C  INC L
        0x2C => {
            cpu.regs.l = alu::inc(cpu.regs.l, &mut cpu.regs.f);
            1
        }
        // 0x2D  DEC L
        0x2D => {
            cpu.regs.l = alu::dec(cpu.regs.l, &mut cpu.regs.f);
            1
        }
        // 0x2E  LD L, d8
        0x2E => {
            cpu.regs.l = cpu.fetch8(bus);
            2
        }
        // 0x2F  CPL
        0x2F => {
            cpu.regs.a = !cpu.regs.a;
            cpu.regs.f.insert(Flags::N | Flags::H);
            1
        }

        // 0x30  JR NC, r8
        0x30 => jr_cond(cpu, bus, !cond_c(cpu)),
        // 0x31  LD SP, d16
        0x31 => {
            cpu.regs.sp = cpu.fetch16(bus);
            3
        }
        // 0x32  LD (HL-), A
        0x32 => {
            bus.write(cpu.regs.hl(), cpu.regs.a);
            cpu.regs.set_hl(cpu.regs.hl().wrapping_sub(1));
            2
        }
        // 0x33  INC SP
        0x33 => {
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            2
        }
        // 0x34  INC (HL)
        0x34 => {
            let v = read_hl(cpu, bus);
            let r = alu::inc(v, &mut cpu.regs.f);
            write_hl(cpu, bus, r);
            3
        }
        // 0x35  DEC (HL)
        0x35 => {
            let v = read_hl(cpu, bus);
            let r = alu::dec(v, &mut cpu.regs.f);
            write_hl(cpu, bus, r);
            3
        }
        // 0x36  LD (HL), d8
        0x36 => {
            let v = cpu.fetch8(bus);
            write_hl(cpu, bus, v);
            3
        }
        // 0x37  SCF
        0x37 => {
            cpu.regs.f.remove(Flags::N | Flags::H);
            cpu.regs.f.insert(Flags::C);
            1
        }
        // 0x38  JR C, r8
        0x38 => jr_cond(cpu, bus, cond_c(cpu)),
        // 0x39  ADD HL, SP
        0x39 => {
            let r = alu::add16(cpu.regs.hl(), cpu.regs.sp, &mut cpu.regs.f);
            cpu.regs.set_hl(r);
            2
        }
        // 0x3A  LD A, (HL-)
        0x3A => {
            cpu.regs.a = bus.read(cpu.regs.hl());
            cpu.regs.set_hl(cpu.regs.hl().wrapping_sub(1));
            2
        }
        // 0x3B  DEC SP
        0x3B => {
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            2
        }
        // 0x3C  INC A
        0x3C => {
            cpu.regs.a = alu::inc(cpu.regs.a, &mut cpu.regs.f);
            1
        }
        // 0x3D  DEC A
        0x3D => {
            cpu.regs.a = alu::dec(cpu.regs.a, &mut cpu.regs.f);
            1
        }
        // 0x3E  LD A, d8
        0x3E => {
            cpu.regs.a = cpu.fetch8(bus);
            2
        }
        // 0x3F  CCF
        0x3F => {
            cpu.regs.f.remove(Flags::N | Flags::H);
            cpu.regs.f.toggle(Flags::C);
            1
        }

        // 0x40..=0x7F — LD r, r' / HALT (0x76)
        0x76 => {
            // HALT
            let pending_irq = !bus.pending_interrupts().is_empty();
            if !cpu.ime && pending_irq {
                // IME=0, pending != 0 → HALT bug: next byte fetched twice.
                cpu.halt_bug = true;
            } else {
                cpu.halted = true;
            }
            1
        }
        0x40..=0x7F => ld_r_r(cpu, bus, op),

        // 0x80..=0x87  ADD A, r/(HL)
        0x80..=0x87 => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            cpu.regs.a = alu::add(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }
        // 0x88..=0x8F  ADC A, r
        0x88..=0x8F => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            cpu.regs.a = alu::adc(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }
        // 0x90..=0x97  SUB r
        0x90..=0x97 => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            cpu.regs.a = alu::sub(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }
        // 0x98..=0x9F  SBC A, r
        0x98..=0x9F => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            cpu.regs.a = alu::sbc(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }
        // 0xA0..=0xA7  AND r
        0xA0..=0xA7 => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            cpu.regs.a = alu::and(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }
        // 0xA8..=0xAF  XOR r
        0xA8..=0xAF => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            cpu.regs.a = alu::xor(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }
        // 0xB0..=0xB7  OR r
        0xB0..=0xB7 => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            cpu.regs.a = alu::or(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }
        // 0xB8..=0xBF  CP r
        0xB8..=0xBF => {
            let (v, c) = read_r(cpu, bus, op & 0x07);
            alu::cp(cpu.regs.a, v, &mut cpu.regs.f);
            c
        }

        // 0xC0  RET NZ
        0xC0 => ret_cond(cpu, bus, !cond_z(cpu)),
        // 0xC1  POP BC
        0xC1 => {
            let v = cpu.pop16(bus);
            cpu.regs.set_bc(v);
            3
        }
        // 0xC2  JP NZ, a16
        0xC2 => jp_cond(cpu, bus, !cond_z(cpu)),
        // 0xC3  JP a16
        0xC3 => {
            let addr = cpu.fetch16(bus);
            cpu.regs.pc = addr;
            4
        }
        // 0xC4  CALL NZ, a16
        0xC4 => call_cond(cpu, bus, !cond_z(cpu)),
        // 0xC5  PUSH BC
        0xC5 => {
            cpu.push16(bus, cpu.regs.bc());
            4
        }
        // 0xC6  ADD A, d8
        0xC6 => {
            let v = cpu.fetch8(bus);
            cpu.regs.a = alu::add(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xC7  RST 00H
        0xC7 => rst(cpu, bus, 0x00),
        // 0xC8  RET Z
        0xC8 => ret_cond(cpu, bus, cond_z(cpu)),
        // 0xC9  RET
        0xC9 => {
            cpu.regs.pc = cpu.pop16(bus);
            4
        }
        // 0xCA  JP Z, a16
        0xCA => jp_cond(cpu, bus, cond_z(cpu)),
        // 0xCB  prefix — handled in cb.rs
        0xCB => super::exec::cb_dispatch(cpu, bus),
        // 0xCC  CALL Z, a16
        0xCC => call_cond(cpu, bus, cond_z(cpu)),
        // 0xCD  CALL a16
        0xCD => {
            let addr = cpu.fetch16(bus);
            cpu.push16(bus, cpu.regs.pc);
            cpu.regs.pc = addr;
            6
        }
        // 0xCE  ADC A, d8
        0xCE => {
            let v = cpu.fetch8(bus);
            cpu.regs.a = alu::adc(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xCF  RST 08H
        0xCF => rst(cpu, bus, 0x08),

        // 0xD0  RET NC
        0xD0 => ret_cond(cpu, bus, !cond_c(cpu)),
        // 0xD1  POP DE
        0xD1 => {
            let v = cpu.pop16(bus);
            cpu.regs.set_de(v);
            3
        }
        // 0xD2  JP NC, a16
        0xD2 => jp_cond(cpu, bus, !cond_c(cpu)),
        // 0xD3 — illegal
        0xD3 => illegal(op),
        // 0xD4  CALL NC, a16
        0xD4 => call_cond(cpu, bus, !cond_c(cpu)),
        // 0xD5  PUSH DE
        0xD5 => {
            cpu.push16(bus, cpu.regs.de());
            4
        }
        // 0xD6  SUB d8
        0xD6 => {
            let v = cpu.fetch8(bus);
            cpu.regs.a = alu::sub(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xD7  RST 10H
        0xD7 => rst(cpu, bus, 0x10),
        // 0xD8  RET C
        0xD8 => ret_cond(cpu, bus, cond_c(cpu)),
        // 0xD9  RETI
        0xD9 => {
            cpu.regs.pc = cpu.pop16(bus);
            cpu.ime = true;
            cpu.ime_pending = false;
            4
        }
        // 0xDA  JP C, a16
        0xDA => jp_cond(cpu, bus, cond_c(cpu)),
        // 0xDB — illegal
        0xDB => illegal(op),
        // 0xDC  CALL C, a16
        0xDC => call_cond(cpu, bus, cond_c(cpu)),
        // 0xDD — illegal
        0xDD => illegal(op),
        // 0xDE  SBC A, d8
        0xDE => {
            let v = cpu.fetch8(bus);
            cpu.regs.a = alu::sbc(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xDF  RST 18H
        0xDF => rst(cpu, bus, 0x18),

        // 0xE0  LDH (a8), A
        0xE0 => {
            let off = cpu.fetch8(bus) as u16;
            bus.write(0xFF00 | off, cpu.regs.a);
            3
        }
        // 0xE1  POP HL
        0xE1 => {
            let v = cpu.pop16(bus);
            cpu.regs.set_hl(v);
            3
        }
        // 0xE2  LD (C), A
        0xE2 => {
            bus.write(0xFF00 | cpu.regs.c as u16, cpu.regs.a);
            2
        }
        // 0xE3 — illegal
        0xE3 => illegal(op),
        // 0xE4 — illegal
        0xE4 => illegal(op),
        // 0xE5  PUSH HL
        0xE5 => {
            cpu.push16(bus, cpu.regs.hl());
            4
        }
        // 0xE6  AND d8
        0xE6 => {
            let v = cpu.fetch8(bus);
            cpu.regs.a = alu::and(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xE7  RST 20H
        0xE7 => rst(cpu, bus, 0x20),
        // 0xE8  ADD SP, r8
        0xE8 => {
            let r8 = cpu.fetch8(bus) as i8;
            cpu.regs.sp = alu::add_sp_i8(cpu.regs.sp, r8, &mut cpu.regs.f);
            4
        }
        // 0xE9  JP HL
        0xE9 => {
            cpu.regs.pc = cpu.regs.hl();
            1
        }
        // 0xEA  LD (a16), A
        0xEA => {
            let addr = cpu.fetch16(bus);
            bus.write(addr, cpu.regs.a);
            4
        }
        // 0xEB — illegal
        0xEB => illegal(op),
        // 0xEC — illegal
        0xEC => illegal(op),
        // 0xED — illegal
        0xED => illegal(op),
        // 0xEE  XOR d8
        0xEE => {
            let v = cpu.fetch8(bus);
            cpu.regs.a = alu::xor(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xEF  RST 28H
        0xEF => rst(cpu, bus, 0x28),

        // 0xF0  LDH A, (a8)
        0xF0 => {
            let off = cpu.fetch8(bus) as u16;
            cpu.regs.a = bus.read(0xFF00 | off);
            3
        }
        // 0xF1  POP AF
        0xF1 => {
            let v = cpu.pop16(bus);
            cpu.regs.set_af(v);
            3
        }
        // 0xF2  LD A, (C)
        0xF2 => {
            cpu.regs.a = bus.read(0xFF00 | cpu.regs.c as u16);
            2
        }
        // 0xF3  DI
        0xF3 => {
            cpu.ime = false;
            cpu.ime_pending = false;
            1
        }
        // 0xF4 — illegal
        0xF4 => illegal(op),
        // 0xF5  PUSH AF
        0xF5 => {
            cpu.push16(bus, cpu.regs.af());
            4
        }
        // 0xF6  OR d8
        0xF6 => {
            let v = cpu.fetch8(bus);
            cpu.regs.a = alu::or(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xF7  RST 30H
        0xF7 => rst(cpu, bus, 0x30),
        // 0xF8  LD HL, SP+r8
        0xF8 => {
            let r8 = cpu.fetch8(bus) as i8;
            let v = alu::add_sp_i8(cpu.regs.sp, r8, &mut cpu.regs.f);
            cpu.regs.set_hl(v);
            3
        }
        // 0xF9  LD SP, HL
        0xF9 => {
            cpu.regs.sp = cpu.regs.hl();
            2
        }
        // 0xFA  LD A, (a16)
        0xFA => {
            let addr = cpu.fetch16(bus);
            cpu.regs.a = bus.read(addr);
            4
        }
        // 0xFB  EI
        0xFB => {
            cpu.ime_pending = true;
            1
        }
        // 0xFC — illegal
        0xFC => illegal(op),
        // 0xFD — illegal
        0xFD => illegal(op),
        // 0xFE  CP d8
        0xFE => {
            let v = cpu.fetch8(bus);
            alu::cp(cpu.regs.a, v, &mut cpu.regs.f);
            2
        }
        // 0xFF  RST 38H
        0xFF => rst(cpu, bus, 0x38),
    }
}

// ---------------- helpers ----------------

#[inline]
fn illegal(op: u8) -> u32 {
    panic!("illegal/unused opcode 0x{:02X}", op);
}

/// Read register encoded by 3-bit `r` field (B,C,D,E,H,L,(HL),A).
/// Returns (value, M-cycle base cost — 1 for register, 2 for (HL)).
#[inline]
fn read_r(cpu: &Cpu, bus: &Bus, r: u8) -> (u8, u32) {
    match r & 0x07 {
        0 => (cpu.regs.b, 1),
        1 => (cpu.regs.c, 1),
        2 => (cpu.regs.d, 1),
        3 => (cpu.regs.e, 1),
        4 => (cpu.regs.h, 1),
        5 => (cpu.regs.l, 1),
        6 => (bus.read(cpu.regs.hl()), 2),
        _ => (cpu.regs.a, 1),
    }
}

#[inline]
fn write_r(cpu: &mut Cpu, bus: &mut Bus, r: u8, v: u8) -> u32 {
    match r & 0x07 {
        0 => {
            cpu.regs.b = v;
            1
        }
        1 => {
            cpu.regs.c = v;
            1
        }
        2 => {
            cpu.regs.d = v;
            1
        }
        3 => {
            cpu.regs.e = v;
            1
        }
        4 => {
            cpu.regs.h = v;
            1
        }
        5 => {
            cpu.regs.l = v;
            1
        }
        6 => {
            bus.write(cpu.regs.hl(), v);
            2
        }
        _ => {
            cpu.regs.a = v;
            1
        }
    }
}

/// `LD r, r'` covering 0x40..=0x7F (excluding 0x76 HALT).
fn ld_r_r(cpu: &mut Cpu, bus: &mut Bus, op: u8) -> u32 {
    let dst = (op >> 3) & 0x07;
    let src = op & 0x07;
    let (v, c_src) = read_r(cpu, bus, src);
    let c_dst = write_r(cpu, bus, dst, v);
    // Standard timing: LD r,r' = 1; LD r,(HL) = 2; LD (HL),r = 2.
    // Just take the max of src/dst cost.
    c_src.max(c_dst)
}

fn jr_cond(cpu: &mut Cpu, bus: &mut Bus, take: bool) -> u32 {
    let off = cpu.fetch8(bus) as i8;
    if take {
        cpu.regs.pc = cpu.regs.pc.wrapping_add(off as i16 as u16);
        3
    } else {
        2
    }
}

fn jp_cond(cpu: &mut Cpu, bus: &mut Bus, take: bool) -> u32 {
    let addr = cpu.fetch16(bus);
    if take {
        cpu.regs.pc = addr;
        4
    } else {
        3
    }
}

fn call_cond(cpu: &mut Cpu, bus: &mut Bus, take: bool) -> u32 {
    let addr = cpu.fetch16(bus);
    if take {
        cpu.push16(bus, cpu.regs.pc);
        cpu.regs.pc = addr;
        6
    } else {
        3
    }
}

fn ret_cond(cpu: &mut Cpu, bus: &mut Bus, take: bool) -> u32 {
    if take {
        cpu.regs.pc = cpu.pop16(bus);
        5
    } else {
        2
    }
}

fn rst(cpu: &mut Cpu, bus: &mut Bus, vec: u16) -> u32 {
    cpu.push16(bus, cpu.regs.pc);
    cpu.regs.pc = vec;
    4
}

// ============================================================================
// CB-prefixed dispatch (256 ops)
// ============================================================================

fn cb_dispatch(cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    let op = cpu.fetch8(bus);
    let r = op & 0x07;
    let (v, base_read) = read_r(cpu, bus, r);
    // CB op groups: high 5 bits define the op.
    let result;
    let writeback;
    match op >> 6 {
        0b00 => {
            // ROT / SHIFT / SWAP family — encoded by (op >> 3) & 7
            result = match (op >> 3) & 0x07 {
                0 => alu::rlc(v, &mut cpu.regs.f),
                1 => alu::rrc(v, &mut cpu.regs.f),
                2 => alu::rl(v, &mut cpu.regs.f),
                3 => alu::rr(v, &mut cpu.regs.f),
                4 => alu::sla(v, &mut cpu.regs.f),
                5 => alu::sra(v, &mut cpu.regs.f),
                6 => alu::swap(v, &mut cpu.regs.f),
                _ => alu::srl(v, &mut cpu.regs.f),
            };
            writeback = true;
        }
        0b01 => {
            // BIT n, r — no writeback; (HL) variant is 3 M-cycles (read only).
            let n = (op >> 3) & 0x07;
            alu::bit(n, v, &mut cpu.regs.f);
            // 1 M for opcode + base_read (1 reg / 2 mem) → but BIT (HL) is 3 cycles total.
            // Our caller adds 1 M for CB itself.
            return if r == 6 { 3 } else { 2 };
        }
        0b10 => {
            // RES n, r
            let n = (op >> 3) & 0x07;
            result = alu::res(n, v);
            writeback = true;
        }
        _ => {
            // SET n, r
            let n = (op >> 3) & 0x07;
            result = alu::set(n, v);
            writeback = true;
        }
    }

    let _ = base_read; // suppress unused
    if writeback {
        write_r(cpu, bus, r, result);
    }

    // Total cycles for read-modify-write CB ops:
    //   register: 2 M-cycles, (HL): 4 M-cycles.
    if r == 6 {
        4
    } else {
        2
    }
}
