# Integration tests

This directory hosts cargo integration tests that drive `gb-core` with real
test ROMs. The ROMs themselves are **not** committed (see
[`../scripts/fetch_test_roms.ps1`](../scripts/fetch_test_roms.ps1)).

Future test files (planned):

| File | What it runs |
|---|---|
| `blargg.rs` | Blargg's `cpu_instrs`, `instr_timing`, `mem_timing`, `dmg_sound` |
| `mooneye.rs` | Mooneye Test Suite cycle-accurate ROMs |
| `acid2.rs`   | dmg-acid2 framebuffer hash compare |

See [`../docs/testing.md`](../docs/testing.md) for the validation strategy.
