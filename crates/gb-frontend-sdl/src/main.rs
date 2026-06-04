//! Desktop SDL2 frontend for `gb-core`.
//!
//! Modeled on [Azayaka](https://github.com/7thSamurai/Azayaka)'s
//! `src/sdl/main.cpp`: an event loop pumps host input → `Gameboy::run_frame()`
//! → blits the 160×144 indexed framebuffer to a streaming texture → optional
//! fixed-timestep pacing.
//!
//! Builds without SDL by default so CI is happy. Enable a runtime with one of:
//!   * `--features sdl` — link to a system / next-to-exe SDL2.
//!   * `--features sdl-bundled` — build SDL2 from source, static-link.
//!     Requires a C toolchain + CMake. Best UX on Windows (MSVC).
//!
//! Keyboard mapping (Azayaka defaults):
//!   Z = A    X = B    Enter = Start    RShift = Select    Arrows = D-pad
//! Hotkeys:
//!   Esc        Quit
//!   Ctrl+P     Pause / resume
//!   Ctrl+R     Reset
//!   Ctrl+F     Toggle fullscreen
//!   Tab (hold) Turbo (uncap framerate)

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
    /// Integer scale factor for the window (default 4 → 640×576).
    #[arg(long, default_value_t = 4)]
    scale: u32,
    /// Start in fullscreen mode.
    #[arg(long)]
    fullscreen: bool,
    /// Don't cap framerate to ~59.7 Hz (run as fast as possible).
    #[arg(long)]
    no_sync: bool,
    /// Disable audio entirely (don't open an audio device, don't queue samples).
    #[arg(long)]
    mute: bool,
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

    // Battery-RAM persistence: a `.sav` file sits next to the ROM and is
    // re-loaded into cart RAM on startup. On clean shutdown we write it back.
    let sav_path = args.rom.with_extension("sav");
    if gb.cart_has_battery() {
        match std::fs::read(&sav_path) {
            Ok(bytes) => {
                gb.load_cart_ram(&bytes);
                log::info!("loaded {} bytes from {}", bytes.len(), sav_path.display());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("could not read {}: {e}", sav_path.display()),
        }
    }

    #[cfg(not(feature = "sdl"))]
    {
        // Suppress unused-field warnings on the no-sdl build.
        let _ = (args.scale, args.fullscreen, args.no_sync, args.mute);
        log::info!("Built without `sdl` feature; running 1 frame headless.");
        let t = gb.run_frame();
        println!("advanced {t} T-cycles");
        save_battery(&gb, &sav_path);
        Ok(())
    }

    #[cfg(feature = "sdl")]
    {
        let result = sdl_main(&mut gb, &args);
        save_battery(&gb, &sav_path);
        result
    }
}

fn save_battery(gb: &Gameboy, sav_path: &std::path::Path) {
    if !gb.cart_has_battery() {
        return;
    }
    let Some(ram) = gb.cart_ram() else { return };
    match std::fs::write(sav_path, ram) {
        Ok(()) => log::info!("wrote {} bytes to {}", ram.len(), sav_path.display()),
        Err(e) => log::warn!("could not write {}: {e}", sav_path.display()),
    }
}

#[cfg(feature = "sdl")]
mod sdl_runtime {
    use super::*;
    use gb_core::{JoypadState, SCREEN_HEIGHT, SCREEN_WIDTH};
    use sdl2::audio::{AudioQueue, AudioSpecDesired};
    use sdl2::event::{Event, WindowEvent};
    use sdl2::keyboard::{Keycode, Mod, Scancode};
    use sdl2::pixels::PixelFormatEnum;
    use sdl2::render::TextureAccess;
    use std::time::{Duration, Instant};

    /// Host audio sample rate. Must match `gb_core::apu::SAMPLE_RATE_HZ`.
    const AUDIO_HZ: i32 = 48_000;
    /// Drop new samples once the queue holds more than this many bytes
    /// (~85 ms of stereo i16 at 48 kHz). Prevents unbounded growth in
    /// `--no-sync` / turbo mode.
    const AUDIO_QUEUE_MAX_BYTES: u32 = 8 * 1024 * 2 * 2;

    /// Classic DMG green palette (shade 0..3 → ARGB).
    /// See Azayaka `core/gpu/dmg_palette.cpp`.
    const PALETTE_ARGB: [u32; 4] = [0xFF_9B_BC_0F, 0xFF_8B_AC_0F, 0xFF_30_62_30, 0xFF_0F_38_0F];

    const FRAME_NS: u64 = 16_742_006; // 1_000_000_000 / 59.7275

    pub fn run(gb: &mut Gameboy, args: &Args) -> Result<()> {
        let sdl = sdl2::init().map_err(|e| anyhow::anyhow!("SDL_Init: {e}"))?;
        let video = sdl.video().map_err(|e| anyhow::anyhow!("SDL_VideoInit: {e}"))?;

        let audio_queue = if args.mute {
            log::info!("audio: --mute given, audio device not opened");
            None
        } else {
            match open_audio(&sdl) {
                Ok(q) => Some(q),
                Err(e) => {
                    log::warn!("audio: failed to open device, continuing muted: {e}");
                    None
                }
            }
        };

        let scale = args.scale.max(1);
        let (win_w, win_h) = (SCREEN_WIDTH as u32 * scale, SCREEN_HEIGHT as u32 * scale);

        let mut window_builder = video.window("gb — gameboy-emulator", win_w, win_h);
        window_builder.position_centered().allow_highdpi().resizable();
        if args.fullscreen {
            window_builder.fullscreen_desktop();
        }
        let window = window_builder.build().context("creating window")?;

        let mut canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .context("creating renderer")?;
        canvas
            .set_logical_size(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)
            .map_err(|e| anyhow::anyhow!("set_logical_size: {e}"))?;

        let texture_creator = canvas.texture_creator();
        let mut texture = texture_creator
            .create_texture(
                PixelFormatEnum::ARGB8888,
                TextureAccess::Streaming,
                SCREEN_WIDTH as u32,
                SCREEN_HEIGHT as u32,
            )
            .context("creating streaming texture")?;

        let mut events = sdl.event_pump().map_err(|e| anyhow::anyhow!("event pump: {e}"))?;
        let mut pixels = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4];
        let mut paused = false;
        let mut fps_acc_start = Instant::now();
        let mut frames_since: u32 = 0;

        'main: loop {
            let frame_start = Instant::now();

            for ev in events.poll_iter() {
                match ev {
                    Event::Quit { .. } => break 'main,
                    Event::Window {
                        win_event: WindowEvent::Close,
                        ..
                    } => break 'main,
                    Event::KeyDown {
                        keycode: Some(kc),
                        keymod,
                        repeat: false,
                        ..
                    } => match kc {
                        Keycode::Escape => break 'main,
                        Keycode::P if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) => {
                            paused = !paused;
                            log::info!("{}", if paused { "paused" } else { "resumed" });
                        }
                        Keycode::R if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) => {
                            gb.reset();
                            log::info!("reset");
                        }
                        Keycode::F if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) => {
                            toggle_fullscreen(&mut canvas);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }

            // Sample held keys for joypad + turbo, just like Azayaka's
            // `InputSDL::update()`.
            let kb = events.keyboard_state();
            let buttons = poll_buttons(&kb);
            let turbo = kb.is_scancode_pressed(Scancode::Tab);

            if !paused {
                gb.set_buttons(buttons);
                gb.run_frame();

                framebuffer_to_argb(gb.frame_buffer(), &mut pixels);
                texture
                    .update(None, &pixels, SCREEN_WIDTH * 4)
                    .map_err(|e| anyhow::anyhow!("texture update: {e}"))?;

                // Drain whatever audio the APU has produced this frame and
                // forward it to SDL — but only if the queue isn't already
                // backlogged (matters in `--no-sync`/turbo mode).
                let samples = gb.take_audio_samples();
                if let Some(q) = audio_queue.as_ref() {
                    if q.size() < AUDIO_QUEUE_MAX_BYTES {
                        let mut interleaved = Vec::with_capacity(samples.len() * 2);
                        for (l, r) in samples {
                            interleaved.push(l);
                            interleaved.push(r);
                        }
                        if let Err(e) = q.queue_audio(&interleaved) {
                            log::warn!("audio: queue_audio failed: {e}");
                        }
                    }
                }
            }

            canvas.clear();
            canvas
                .copy(&texture, None, None)
                .map_err(|e| anyhow::anyhow!("render copy: {e}"))?;
            canvas.present();

            // FPS in title (refresh once per second). Azayaka does the same.
            frames_since += 1;
            let elapsed = fps_acc_start.elapsed();
            if elapsed >= Duration::from_secs(1) {
                let fps = frames_since as f64 / elapsed.as_secs_f64();
                let _ = canvas
                    .window_mut()
                    .set_title(&format!("gb — gameboy-emulator [{fps:.1} fps]"));
                fps_acc_start = Instant::now();
                frames_since = 0;
            }

            // Fixed-timestep pacing. `vsync` already caps us on most displays
            // but explicitly sleeping anchors timing for non-vsync setups.
            if !args.no_sync && !turbo {
                let target = Duration::from_nanos(FRAME_NS);
                let used = frame_start.elapsed();
                if used < target {
                    std::thread::sleep(target - used);
                }
            }
        }

        Ok(())
    }

    /// Open a 48 kHz / stereo / i16 SDL audio queue and start playback.
    fn open_audio(sdl: &sdl2::Sdl) -> Result<AudioQueue<i16>> {
        let audio = sdl.audio().map_err(|e| anyhow::anyhow!("SDL_AudioInit: {e}"))?;
        let spec = AudioSpecDesired {
            freq: Some(AUDIO_HZ),
            channels: Some(2),
            samples: Some(1024),
        };
        let queue: AudioQueue<i16> = audio
            .open_queue(None, &spec)
            .map_err(|e| anyhow::anyhow!("SDL_OpenAudio: {e}"))?;
        log::info!(
            "audio: device opened — {} Hz, {} ch, buffer {} samples",
            queue.spec().freq,
            queue.spec().channels,
            queue.spec().samples
        );
        queue.resume();
        Ok(queue)
    }

    fn poll_buttons(kb: &sdl2::keyboard::KeyboardState) -> JoypadState {
        let mut s = JoypadState::empty();
        if kb.is_scancode_pressed(Scancode::Right) { s |= JoypadState::RIGHT; }
        if kb.is_scancode_pressed(Scancode::Left)  { s |= JoypadState::LEFT; }
        if kb.is_scancode_pressed(Scancode::Up)    { s |= JoypadState::UP; }
        if kb.is_scancode_pressed(Scancode::Down)  { s |= JoypadState::DOWN; }
        if kb.is_scancode_pressed(Scancode::Z)      { s |= JoypadState::A; }
        if kb.is_scancode_pressed(Scancode::X)      { s |= JoypadState::B; }
        if kb.is_scancode_pressed(Scancode::RShift) { s |= JoypadState::SELECT; }
        if kb.is_scancode_pressed(Scancode::Return) { s |= JoypadState::START; }
        s
    }

    fn framebuffer_to_argb(frame: &[u8], out: &mut [u8]) {
        debug_assert_eq!(frame.len() * 4, out.len());
        for (i, &shade) in frame.iter().enumerate() {
            let argb = PALETTE_ARGB[(shade & 0b11) as usize].to_le_bytes();
            // SDL ARGB8888 with little-endian byte order = B, G, R, A.
            let o = i * 4;
            out[o] = argb[0];
            out[o + 1] = argb[1];
            out[o + 2] = argb[2];
            out[o + 3] = argb[3];
        }
    }

    fn toggle_fullscreen(canvas: &mut sdl2::render::WindowCanvas) {
        use sdl2::video::FullscreenType;
        let win = canvas.window_mut();
        let next = match win.fullscreen_state() {
            FullscreenType::Off => FullscreenType::Desktop,
            _ => FullscreenType::Off,
        };
        if let Err(e) = win.set_fullscreen(next) {
            log::warn!("set_fullscreen failed: {e}");
        }
    }
}

#[cfg(feature = "sdl")]
fn sdl_main(gb: &mut Gameboy, args: &Args) -> Result<()> {
    sdl_runtime::run(gb, args)
}
