//! Static-link helpers for the bundled SDL2 path on Windows.
//!
//! `sdl2-sys 0.37` doesn't declare `advapi32` (registry APIs used by
//! `WIN_LookupAudioDeviceName`) when statically linking SDL2 on Windows.
//! Add it ourselves so `--features sdl-bundled` links cleanly.
fn main() {
    let bundled =
        std::env::var_os("CARGO_FEATURE_SDL_BUNDLED").is_some();
    let windows_msvc =
        std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
            && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if bundled && windows_msvc {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
