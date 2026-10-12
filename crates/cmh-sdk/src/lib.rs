//! Independent SDK for the Cooler Master MasterHUB modular control deck.
//!
//! Every MasterHUB module (and the base itself) enumerates as its own USB HID
//! device. This crate is split into layers so that packet handling can be
//! tested without hardware:
//!
//! - [`devices`]: module models and static capabilities
//! - [`protocol`]: packet encoding and decoding, free of I/O
//! - [`transport`]: HID enumeration and raw report I/O
//!
//! Only behaviour that was confirmed against real hardware is implemented;
//! see `docs/feature-matrix.md` for the current status.

pub mod devices;
pub mod error;
pub mod protocol;
pub mod transport;

pub use devices::{ModuleKind, VENDOR_ID};
pub use error::{Error, Result};
