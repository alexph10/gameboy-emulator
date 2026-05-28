//! `gb-info` — dump the cartridge header of a ROM.

use anyhow::{Context, Result};
use clap::Parser;
use gb_core::cartridge::Header;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "gb-info", about = "Print cartridge header info for a ROM")]
struct Args {
    rom: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let rom = std::fs::read(&args.rom)
        .with_context(|| format!("reading {:?}", args.rom))?;
    let h = Header::parse(&rom).map_err(|e| anyhow::anyhow!("{e}"))?;

    println!("Title           : {}", h.title);
    println!("Cartridge type  : 0x{:02X}", h.cart_type);
    println!("ROM size        : {} KiB", h.rom_size_bytes / 1024);
    println!("RAM size        : {} KiB", h.ram_size_bytes / 1024);
    println!("CGB flag        : 0x{:02X}", h.cgb_flag);
    println!("SGB flag        : 0x{:02X}", h.sgb_flag);
    println!("Header checksum : {}", if h.header_checksum_ok { "OK" } else { "BAD" });
    Ok(())
}
