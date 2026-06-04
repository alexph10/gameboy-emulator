//! Blargg `cpu_instrs` integration test.
//!
//! Loads the test ROM, runs the emulator headlessly, and scrapes the serial
//! port (Blargg's standard result-reporting channel) until either a pass or a
//! failure phrase appears.

use gb_core::{Gameboy, GameboyOptions};

/// Run the emulator until `needle` appears in the serial log, the ROM prints a
/// `Failed` line, or `max_t_cycles` elapses. Returns the captured serial log.
fn run_until_serial(rom_path: &str, max_t_cycles: u64, needle: &str) -> String {
    let rom = std::fs::read(rom_path)
        .unwrap_or_else(|e| panic!("missing ROM {rom_path}: {e} — run scripts/fetch_test_roms.ps1"));
    let mut gb = Gameboy::new(rom, GameboyOptions::default()).expect("invalid ROM");

    let mut total: u64 = 0;
    loop {
        total += gb.run_frame() as u64;
        let log = String::from_utf8_lossy(gb.serial_output()).to_string();
        if log.contains(needle) || log.contains("Failed") {
            return log;
        }
        assert!(
            total < max_t_cycles,
            "timeout after {total} T-cycles waiting for `{needle}`; log so far:\n{log}",
        );
    }
}

const BUDGET: u64 = 500_000_000;

#[test]
fn cpu_instrs() {
    let log = run_until_serial(
        "../../tests/roms/blargg/cpu_instrs/cpu_instrs.gb",
        BUDGET,
        "Passed all tests",
    );
    println!("Blargg cpu_instrs serial log:\n{log}");
    assert!(
        log.contains("Passed all tests"),
        "Blargg cpu_instrs did not pass. Final log:\n{log}",
    );
}
