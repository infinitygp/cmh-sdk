//! Independent SDK for the Cooler Master MasterHUB modular control deck.
//!
//! Every MasterHUB module (and the base itself) enumerates as its own USB HID
//! device. This crate is split into layers so that packet handling can be
//! tested without hardware:
//!
//! - [`devices`]: module models and static capabilities
//! - [`protocol`]: packet encoding and decoding, free of I/O
//! - [`input`]: typed input events
//! - `transport` (private): HID enumeration and raw report I/O
//! - [`module`]: typed operations on one open module
//! - [`manager`]: discovery and hot-plug tracking
//! - [`fixture`]: text format for recorded reports
//!
//! Only behaviour that was confirmed against real hardware is implemented;
//! see `docs/feature-matrix.md` for the current status.
//!
//! ```no_run
//! use std::time::Duration;
//!
//! let mut manager = cmh_sdk::Manager::new()?;
//! manager.refresh()?;
//! for interface in manager.modules().to_vec() {
//!     let mut module = manager.open_interface(&interface)?;
//!     match module.read_events(Duration::from_millis(100)) {
//!         Ok(events) => {
//!             for event in events {
//!                 println!("{}: {event:?}", module.kind());
//!             }
//!         }
//!         // One odd report does not invalidate the handle.
//!         Err(cmh_sdk::Error::MalformedReport(_)) => {}
//!         Err(err) => return Err(err),
//!     }
//! }
//! # Ok::<(), cmh_sdk::Error>(())
//! ```

pub mod devices;
pub mod error;
pub mod fixture;
pub mod input;
mod location;
pub mod manager;
pub mod module;
pub mod protocol;
mod transport;

pub use devices::{Capabilities, DisplaySpec, ModuleKind, VENDOR_ID};
pub use error::{Error, Result};
pub use input::{InputDecoder, InputEvent};
pub use manager::{HotplugEvent, Manager};
pub use module::Module;
pub use protocol::led::LedMode;
pub use transport::{HidInterface, enumerate};
