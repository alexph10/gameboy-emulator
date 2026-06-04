//! dmg-acid2 pixel-perfect framebuffer test.
//!
//! Runs the dmg-acid2 ROM headlessly for enough frames to let the test
//! pattern settle, then asserts the 160×144 indexed framebuffer matches the
//! reference PNG byte-for-byte.

use gb_core::{Gameboy, GameboyOptions, SCREEN_HEIGHT, SCREEN_WIDTH};

const REFERENCE_PNG: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/roms/dmg-acid2/img/reference-dmg.png");
const ROM_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/roms/dmg-acid2/dmg-acid2.gb");

fn decode_reference() -> Vec<u8> {
    let f = std::fs::File::open(REFERENCE_PNG).expect("reference PNG missing");
    let mut decoder = png::Decoder::new(f);
    // Force expansion of paletted / grayscale-alpha images into a
    // straightforward RGB(A)-or-greyscale buffer so we get the actual sample
    // bytes ($00/$55/$AA/$FF) regardless of how the file is encoded.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.width, SCREEN_WIDTH as u32);
    assert_eq!(info.height, SCREEN_HEIGHT as u32);
    let bytes = &buf[..info.buffer_size()];
    let bpp = info.color_type.samples();
    bytes
        .chunks(bpp)
        .map(|px| match px[0] {
            0xFF => 0,
            0xAA => 1,
            0x55 => 2,
            0x00 => 3,
            other => panic!(
                "unexpected reference shade {other:#x} (color_type={:?})",
                info.color_type
            ),
        })
        .collect()
}

#[test]
fn dmg_acid2() {
    let rom = std::fs::read(ROM_PATH).expect("ROM missing");
    let mut gb = Gameboy::new(rom, GameboyOptions::default()).expect("invalid ROM");
    for _ in 0..60 {
        gb.run_frame();
    }

    let expected = decode_reference();
    let got = gb.frame_buffer();
    assert_eq!(got.len(), expected.len());

    let mismatches: Vec<(usize, u8, u8)> = got
        .iter()
        .zip(expected.iter())
        .enumerate()
        .filter_map(|(i, (g, e))| if g != e { Some((i, *g, *e)) } else { None })
        .collect();

    if !mismatches.is_empty() {
        eprintln!("{} mismatching pixels (of {}):", mismatches.len(), got.len());
        for (i, g, e) in mismatches.iter().take(20) {
            let x = i % SCREEN_WIDTH;
            let y = i / SCREEN_WIDTH;
            eprintln!("  ({x:>3},{y:>3}): got shade {g}, expected {e}");
        }
        panic!("dmg-acid2 framebuffer does not match reference");
    }
}
