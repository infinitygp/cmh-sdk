//! MasterHUB module models and their static capabilities.

use std::fmt;

/// USB vendor ID shared by every MasterHUB module (Cooler Master).
pub const VENDOR_ID: u16 = 0x2516;

/// A MasterHUB module model, identified by its USB product ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModuleKind {
    /// Base plate hosting the other modules.
    Base,
    /// 15 LCD keys.
    Keys15,
    /// 5 faders.
    Faders5,
    /// 3 rotary knobs with push.
    Knobs3,
    /// 2 rollers.
    Rollers2,
    /// Single dial with a round IPS display.
    DialDisplay,
}

/// Modules that share report IDs, command codes and report sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    /// Base, faders, knobs and rollers.
    Basic,
    Keys,
    Dial,
}

/// A display that accepts JPEG images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DisplaySpec {
    /// Number of independently addressable images.
    pub images: u8,
    pub width: u16,
    pub height: u16,
}

/// What a module model offers. Only hardware-verified items are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    pub buttons: u8,
    pub encoders: u8,
    /// Absolute controls reporting a position of 0..=100.
    pub faders: u8,
    pub display: Option<DisplaySpec>,
}

impl ModuleKind {
    pub const ALL: [ModuleKind; 6] = [
        ModuleKind::Base,
        ModuleKind::Keys15,
        ModuleKind::Faders5,
        ModuleKind::Knobs3,
        ModuleKind::Rollers2,
        ModuleKind::DialDisplay,
    ];

    pub fn from_product_id(pid: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.product_id() == pid)
    }

    pub const fn product_id(self) -> u16 {
        match self {
            ModuleKind::Base => 0x01f6,
            ModuleKind::Keys15 => 0x01fe,
            ModuleKind::Faders5 => 0x0200,
            ModuleKind::Knobs3 => 0x0202,
            ModuleKind::Rollers2 => 0x0204,
            ModuleKind::DialDisplay => 0x0206,
        }
    }

    /// Human readable name.
    pub const fn name(self) -> &'static str {
        match self {
            ModuleKind::Base => "Base",
            ModuleKind::Keys15 => "15 LCD Keys",
            ModuleKind::Faders5 => "5 Faders",
            ModuleKind::Knobs3 => "3 Knobs",
            ModuleKind::Rollers2 => "2 Rollers",
            ModuleKind::DialDisplay => "Dial Display",
        }
    }

    /// Prefix shared by the serial numbers of this model, e.g. `MHKB03AA22`.
    ///
    /// Observed on real units of every model.
    pub const fn serial_prefix(self) -> &'static str {
        match self {
            ModuleKind::Base => "MHBS01AA46",
            ModuleKind::Keys15 => "MHKD15AA43",
            ModuleKind::Faders5 => "MHFD05AA42",
            ModuleKind::Knobs3 => "MHKB03AA22",
            ModuleKind::Rollers2 => "MHRR02AA41",
            ModuleKind::DialDisplay => "MHDD01AA22",
        }
    }

    /// Verified capabilities of this model.
    ///
    /// LEDs are not listed yet because their reports have not been confirmed
    /// on hardware.
    pub const fn capabilities(self) -> Capabilities {
        const NONE: Capabilities = Capabilities {
            buttons: 0,
            encoders: 0,
            faders: 0,
            display: None,
        };
        match self {
            ModuleKind::Base => NONE,
            ModuleKind::Faders5 => Capabilities { faders: 5, ..NONE },
            ModuleKind::Keys15 => Capabilities {
                buttons: 15,
                display: Some(DisplaySpec {
                    images: 15,
                    width: 72,
                    height: 72,
                }),
                ..NONE
            },
            ModuleKind::Knobs3 => Capabilities {
                buttons: 3,
                encoders: 3,
                ..NONE
            },
            ModuleKind::Rollers2 => Capabilities {
                encoders: 2,
                ..NONE
            },
            ModuleKind::DialDisplay => Capabilities {
                buttons: 1,
                encoders: 1,
                display: Some(DisplaySpec {
                    images: 1,
                    width: 240,
                    height: 240,
                }),
                ..NONE
            },
        }
    }

    pub(crate) const fn family(self) -> Family {
        match self {
            ModuleKind::Base | ModuleKind::Faders5 | ModuleKind::Knobs3 | ModuleKind::Rollers2 => {
                Family::Basic
            }
            ModuleKind::Keys15 => Family::Keys,
            ModuleKind::DialDisplay => Family::Dial,
        }
    }
}

impl fmt::Display for ModuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_id_round_trip() {
        for kind in ModuleKind::ALL {
            assert_eq!(ModuleKind::from_product_id(kind.product_id()), Some(kind));
        }
        assert_eq!(ModuleKind::from_product_id(0xffff), None);
    }
}
