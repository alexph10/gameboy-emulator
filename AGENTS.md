# Agent Guidance — gameboy-emulator

## Project intent
A cycle-accurate, well-documented Nintendo Game Boy (DMG) emulator in Rust.
Game Boy Color (CGB) is an explicit non-goal for the initial milestones — get
DMG rock-solid first.

## Conventions
- Rust 2021, stable toolchain, `rustfmt` + `clippy` enforced (`-D warnings`).
- The `gb-core` crate **must not** depend on SDL, OS APIs, files, or threads.
  Frontends own all I/O. This keeps `gb-core` portable to web (wasm),
  headless test runners, and embedded targets.
- Every module that emulates hardware should cite the relevant section of
  Pan Docs / GB CTR / TCAGBD in a doc comment.
- Prefer `u8`/`u16` exact types over `usize` where modeling hardware registers.
- Tick model: drive the bus in T-cycles (4.194304 MHz). M-cycle (÷4) helpers
  are fine for CPU instruction stepping.

## Validation hierarchy
When in doubt, prefer behavior matching, in order:
1. The test ROMs in [`docs/testing.md`](docs/testing.md)
2. Pan Docs (https://gbdev.io/pandocs/)
3. Game Boy: Complete Technical Reference (gekkio/gb-ctr)
4. SameBoy / Mooneye-GB source as reference implementations

## Don't
- Don't commit copyrighted ROMs, bootroms, or game data.
- Don't add CGB/SGB-only features until DMG passes dmg-acid2 + Blargg cpu_instrs.
- Don't introduce panics on guest-controlled inputs; emulators must never
  crash on a malformed ROM.
