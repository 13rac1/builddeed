//! Compiles the window (ui/app.slint) with the widget style of the target
//! system:
//!
//! - Windows: fluent, the style of Windows 11.
//! - macOS: cupertino, the style of macOS.
//! - Linux and other systems: fluent. The native style of Slint on Linux
//!   needs Qt, and the app links no Qt. Fluent has the control sizes of a
//!   desktop program; material has the larger controls of a phone.
//!
//! Each style follows the light or dark setting of the system.

fn main() {
    let style = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => "cupertino",
        _ => "fluent",
    };
    let config = slint_build::CompilerConfiguration::new().with_style(style.into());
    slint_build::compile_with_config("ui/app.slint", config).expect("cannot compile ui/app.slint");
}
