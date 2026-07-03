//! VPO MIDI - MIDI input/output handling
//!
//! Handles MIDI device discovery, connection, and message parsing.

pub mod device;
pub mod message;
pub mod router;
pub mod ble;
pub mod player;

pub use device::*;
pub use message::*;
pub use router::*;
pub use ble::*;
pub use player::*;
