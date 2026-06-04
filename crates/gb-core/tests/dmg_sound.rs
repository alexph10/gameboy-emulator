//! Blargg `dmg_sound` integration tests — APU register / power-on behaviour.
//!
//! Unlike `cpu_instrs`, the `dmg_sound` ROMs do **not** emit their result over
//! serial. They write a status byte to `$A000` (with signature `DE B0 61` at
//! `$A001..=$A003`) and a zero-terminated text log starting at `$A004` of
//! cartridge RAM. We poll those bytes through [`Gameboy::peek_byte`] to detect
//! pass/fail and recover the log for diagnostic output.
//!
//! Gate 1 hard-asserts `01-registers`. The remaining sub-tests are exposed as
//! `#[ignore]`d stretch tests — run with `cargo test ... -- --ignored` to see
//! which currently pass.

use gb_core::{Gameboy, GameboyOptions};

const BUDGET: u64 = 500_000_000;
const SIG: [u8; 3] = [0xDE, 0xB0, 0x61];

/// Run a Blargg dmg_sound ROM until `$A000` reports a non-running status.
/// Returns `(status_byte, captured_text_log)`. `status_byte == 0` is pass.
fn run_blargg_sound(rom_path: &str, max_t_cycles: u64) -> (u8, String) {
    let rom = std::fs::read(rom_path)
        .unwrap_or_else(|e| panic!("missing ROM {rom_path}: {e} — run scripts/fetch_test_roms.ps1"));
    let mut gb = Gameboy::new(rom, GameboyOptions::default()).expect("invalid ROM");

    let mut total: u64 = 0;
    let mut saw_sig = false;
    loop {
        total += gb.run_frame() as u64;
        // Discard generated audio so the APU buffer doesn't grow unbounded.
        let _ = gb.take_audio_samples();

        if !saw_sig {
            let sig = [gb.peek_byte(0xA001), gb.peek_byte(0xA002), gb.peek_byte(0xA003)];
            if sig == SIG {
                saw_sig = true;
            }
        }
        if saw_sig {
            let status = gb.peek_byte(0xA000);
            if status != 0x80 {
                return (status, read_text_log(&gb));
            }
        }

        if total >= max_t_cycles {
            return (0xFE, read_text_log(&gb));
        }
    }
}

fn read_text_log(gb: &Gameboy) -> String {
    let mut s = String::new();
    let mut addr: u16 = 0xA004;
    // Hard cap so a corrupted/never-terminated string can't OOM us.
    for _ in 0..4096 {
        let b = gb.peek_byte(addr);
        if b == 0 {
            break;
        }
        // Skip stray non-ASCII bytes silently.
        if (0x20..=0x7E).contains(&b) || b == b'\n' || b == b'\r' || b == b'\t' {
            s.push(b as char);
        }
        addr = addr.wrapping_add(1);
    }
    s
}

const ROM_DIR: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/roms/blargg/dmg_sound/rom_singles");

#[test]
fn dmg_sound_01_registers() {
    let (status, log) = run_blargg_sound(&format!("{ROM_DIR}/01-registers.gb"), BUDGET);
    println!("dmg_sound 01-registers status=0x{status:02X} log:\n{log}");
    assert_eq!(status, 0, "Blargg dmg_sound/01-registers failed (status=0x{status:02X}). Log:\n{log}");
    assert!(log.to_lowercase().contains("passed"), "expected 'Passed' in log:\n{log}");
}

// The remaining sub-tests are not currently guaranteed to pass — they're
// gated behind `--ignored`. Run with `cargo test ... -- --ignored` to see.

#[test]
#[ignore = "stretch goal — depends on cycle-accurate length-counter timing"]
fn dmg_sound_02_len_ctr() {
    let (status, log) = run_blargg_sound(&format!("{ROM_DIR}/02-len ctr.gb"), BUDGET);
    println!("dmg_sound 02-len ctr status=0x{status:02X} log:\n{log}");
    assert_eq!(status, 0, "log:\n{log}");
}

#[test]
#[ignore = "stretch goal — trigger / extra-length-clock quirks"]
fn dmg_sound_03_trigger() {
    let (status, log) = run_blargg_sound(&format!("{ROM_DIR}/03-trigger.gb"), BUDGET);
    println!("dmg_sound 03-trigger status=0x{status:02X} log:\n{log}");
    assert_eq!(status, 0, "log:\n{log}");
}

#[test]
#[ignore = "stretch goal — power-cycle register state"]
fn dmg_sound_11_regs_after_power() {
    let (status, log) =
        run_blargg_sound(&format!("{ROM_DIR}/11-regs after power.gb"), BUDGET);
    println!("dmg_sound 11-regs after power status=0x{status:02X} log:\n{log}");
    assert_eq!(status, 0, "log:\n{log}");
}
