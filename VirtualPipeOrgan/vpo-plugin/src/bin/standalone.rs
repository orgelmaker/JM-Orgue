//! Standalone executable variant van de plugin — handig voor lokaal testen
//! zonder DAW. Start een eigen audio + MIDI host via nih-plug.
//!
//! Build: cargo build --release --bin vpo-plugin-standalone --features standalone
//! Run:   ./vpo-plugin-standalone

use nih_plug::prelude::*;
use vpo_plugin::JmOrgue;

fn main() {
    nih_export_standalone::<JmOrgue>();
}
