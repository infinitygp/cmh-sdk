//! MasterHUB module models.

use std::fmt;

/// USB vendor ID shared by every MasterHUB module (Cooler Master).
pub const VENDOR_ID: u16 = 0x2516;

/// A MasterHUB module model, identified by its USB product ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModuleKind {
    /// Base plate (4x6 grid) hosting the other modules.
    Base,
    /// 15 LCD keys.
    Keys15,
    /// 5 motorless faders.
    Faders5,
    /// 3 rotary knobs with push.
    Knobs3,
    /// 2 rollers.
    Rollers2,
    /// Single dial with an IPS display.
    DialDisplay,
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
    /// Observed on real units for every model except [`ModuleKind::Faders5`],
    /// whose prefix comes from third-party documentation.
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
