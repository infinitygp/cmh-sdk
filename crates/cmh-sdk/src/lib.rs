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
//! - `watch` (private): the system's announcements of arriving and leaving devices
//! - [`module`]: typed operations on one open module
//! - [`manager`]: discovery and hot-plug tracking
//! - [`deck`]: all modules as one event stream; start here
//! - [`placement`]: where a module sits on the base
//! - `picture` (cargo feature `image`): converts pictures for the displays
//! - [`fixture`]: text format for recorded reports
//! - `simulator` (cargo feature `test-support`): modules without hardware,
//!   for an application's own tests
//!
//! Only behaviour that was confirmed against real hardware is implemented;
//! see `docs/feature-matrix.md` for the current status.
//!
//! ```no_run
//! use std::time::Duration;
//! use cmh_sdk::{Deck, DeckEvent, InputEvent};
//!
//! let mut deck = Deck::new()?;
//! loop {
//!     for event in deck.poll(Duration::from_millis(50))? {
//!         if let DeckEvent::Input { id, event: InputEvent::ButtonDown { index }, .. } = event {
//!             println!("button {index} on {id}");
//!         }
//!     }
//! }
//! # Ok::<(), cmh_sdk::Error>(())
//! ```

pub mod deck;
pub mod devices;
pub mod error;
pub mod fixture;
pub mod input;
mod location;
pub mod manager;
pub mod module;
#[cfg(feature = "image")]
pub mod picture;
pub mod placement;
pub mod protocol;
#[cfg(feature = "test-support")]
pub mod simulator;
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
mod testing;
mod transport;
mod watch;

pub use deck::{Deck, DeckEvent, DeckOptions, DeckWaker, ModuleId, ModuleInfo};
pub use devices::{Capabilities, DisplaySpec, ModuleKind, VENDOR_ID};
pub use error::{Error, Result};
pub use input::{InputDecoder, InputEvent};
pub use manager::{HotplugEvent, Manager};
pub use module::Module;
pub use placement::{Placement, Rotation};
pub use protocol::led::LedMode;
pub use transport::{HidInterface, enumerate};
