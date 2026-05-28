#### Goals

1. **Accuracy first** — target cycle-accurate behavior validated against the
   community test ROM suites (Blargg, Mooneye, dmg-acid2, Mealybug Tearoom).
2. **Clean separation** — the core emulator is a `no_std`-friendly library with
   zero I/O; frontends (SDL, web, headless) consume it.
3. **Documented** — every non-trivial design choice cites a primary source
   (Pan Docs, GB CTR, TCAGBD).
