//! Desktop frontend for `gb-core`. Builds without SDL by default so CI is
//! happy; pass `--features sdl` to enable the windowed runtime.

use anyhow::{Context, Result};
use clap::Parser;
use gb_core::{Gameboy, GameboyOptions};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "gb", about = "Run a Game Boy ROM")]
struct Args {
    /// Path to a .gb ROM.
    rom: PathBuf,
    /// Optional path to a 256-byte DMG boot ROM.
    #[arg(long)]
    bootrom: Option<PathBuf>,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let rom = std::fs::read(&args.rom)
        .with_context(|| format!("reading ROM {:?}", args.rom))?;
    let boot_rom = args
        .bootrom
        .as_ref()
        .map(std::fs::read)
        .transpose()
        .context("reading boot ROM")?;

    let mut gb = Gameboy::new(rom, GameboyOptions { boot_rom })
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    #[cfg(not(feature = "sdl"))]
    {
        log::info!("Built without `sdl` feature; running 1 frame headless.");
        let t = gb.run_frame();
        println!("advanced {t} T-cycles");
        Ok(())
    }

    #[cfg(feature = "sdl")]
    {
        sdl_main(&mut gb)
    }
}

#[cfg(feature = "sdl")]
fn sdl_main(_gb: &mut Gameboy) -> Result<()> {
    // TODO: open SDL window, audio queue, input loop, blit frame_buffer().
    anyhow::bail!("SDL frontend not yet implemented")
}
