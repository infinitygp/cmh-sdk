//! LED control of the basic modules.
//!
//! 64 byte output reports sent to the HID device that owns the LEDs:
//!
//! ```text
//! 02 2c <module> <index> <mode> <persist>           effect
//! 02 2d <module> <index> <r> <g> <b> <persist>      colour
//! 02 2e <module> <index> <brightness 0..=100>       brightness
//! ```
//!
//! `<module>` is `00` on the base, `01` on the 5 Faders and `02` on the
//! 3 Knobs; `<index>` selects one LED zone. The vendor application sets `<persist>` to `01` only when it
//! stores a setting in the module; this SDK always sends `00`.

use crate::devices::ModuleKind;
use crate::error::{Error, Result};

/// Output report size of the basic modules, including the report ID.
pub const BASIC_OUTPUT_REPORT_LEN: usize = 64;
/// Highest LED brightness.
pub const LED_BRIGHTNESS_MAX: u8 = 100;

const OUTPUT_REPORT_ID: u8 = 0x02;
const CMD_MODE: u8 = 0x2c;
const CMD_COLOR: u8 = 0x2d;
const CMD_BRIGHTNESS: u8 = 0x2e;

/// LED effect, named as in the vendor application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LedMode {
    Off,
    Static,
    Breathing,
    Strobe,
    Cycling,
    Random,
    Wave,
    Water,
    Rainbow,
}

impl LedMode {
    pub const ALL: [LedMode; 9] = [
        LedMode::Off,
        LedMode::Static,
        LedMode::Breathing,
        LedMode::Strobe,
        LedMode::Cycling,
        LedMode::Random,
        LedMode::Wave,
        LedMode::Water,
        LedMode::Rainbow,
    ];

    /// Wire value. `4` is skipped: the vendor application offers no effect
    /// with that code.
    pub const fn code(self) -> u8 {
        match self {
            LedMode::Off => 0,
            LedMode::Static => 1,
            LedMode::Breathing => 2,
            LedMode::Strobe => 3,
            LedMode::Cycling => 5,
            LedMode::Random => 6,
            LedMode::Wave => 7,
            LedMode::Water => 8,
            LedMode::Rainbow => 9,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() == code)
    }
}

/// One LED setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedCommand {
    Mode(LedMode),
    /// Colour used by the effects that take one (static, breathing, strobe).
    Color {
        r: u8,
        g: u8,
        b: u8,
    },
    /// `0..=LED_BRIGHTNESS_MAX`.
    Brightness(u8),
}

/// A decoded LED output report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct LedReport {
    /// `00` base, `01` 5 Faders, `02` 3 Knobs.
    pub module_code: u8,
    pub index: u8,
    pub command: LedCommand,
    /// Whether the report asks the module to store the setting.
    pub persist: bool,
}

/// Module code used in LED reports, for the modules where LED control has
/// been observed.
const fn module_code(kind: ModuleKind) -> Option<u8> {
    match kind {
        ModuleKind::Base => Some(0x00),
        ModuleKind::Faders5 => Some(0x01),
        ModuleKind::Knobs3 => Some(0x02),
        _ => None,
    }
}

/// Builds the report that applies `command` to LED zone `index` of `kind`.
/// The setting is not stored by the module.
pub fn led_report(
    kind: ModuleKind,
    index: u8,
    command: LedCommand,
) -> Result<[u8; BASIC_OUTPUT_REPORT_LEN]> {
    let leds = kind.capabilities().leds;
    let Some(module) = module_code(kind).filter(|_| leds > 0) else {
        return Err(Error::Unsupported {
            module: kind.name(),
            capability: "LED control",
        });
    };
    if index >= leds {
        return Err(Error::InvalidArgument(format!(
            "{kind} has {leds} LED zone(s), got index {index}"
        )));
    }
    let mut report = [0u8; BASIC_OUTPUT_REPORT_LEN];
    report[0] = OUTPUT_REPORT_ID;
    report[2] = module;
    report[3] = index;
    match command {
        LedCommand::Mode(mode) => {
            report[1] = CMD_MODE;
            report[4] = mode.code();
        }
        LedCommand::Color { r, g, b } => {
            report[1] = CMD_COLOR;
            report[4..7].copy_from_slice(&[r, g, b]);
        }
        LedCommand::Brightness(level) => {
            if level > LED_BRIGHTNESS_MAX {
                return Err(Error::InvalidArgument(format!(
                    "LED brightness {level} exceeds {LED_BRIGHTNESS_MAX}"
                )));
            }
            report[1] = CMD_BRIGHTNESS;
            report[4] = level;
        }
    }
    Ok(report)
}

/// Decodes an LED output report, for example from a capture.
///
/// Bytes behind the command's own fields are ignored: the vendor
/// application leaves stale buffer contents there.
pub fn parse_led_report(report: &[u8]) -> Result<LedReport> {
    let malformed = |what: &str| Error::MalformedReport(format!("LED report: {what}"));
    let &[
        OUTPUT_REPORT_ID,
        command,
        module_code,
        index,
        a,
        b,
        c,
        d,
        ..,
    ] = report
    else {
        return Err(malformed("not an output report of at least 8 bytes"));
    };
    let (command, persist) = match command {
        CMD_MODE => (
            LedCommand::Mode(LedMode::from_code(a).ok_or_else(|| malformed("unknown mode"))?),
            b,
        ),
        CMD_COLOR => (LedCommand::Color { r: a, g: b, b: c }, d),
        CMD_BRIGHTNESS if a <= LED_BRIGHTNESS_MAX => (LedCommand::Brightness(a), 0),
        CMD_BRIGHTNESS => return Err(malformed("brightness out of range")),
        _ => return Err(malformed("not an LED command")),
    };
    if persist > 1 {
        return Err(malformed("unexpected persist flag"));
    }
    Ok(LedReport {
        module_code,
        index,
        command,
        persist: persist == 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{Direction, Fixture};

    const SESSION: &str = include_str!("../../../../fixtures/leds/vendor-app-lighting.txt");

    fn captured() -> Vec<(ModuleKind, Vec<u8>)> {
        Fixture::parse(SESSION)
            .unwrap()
            .records()
            .iter()
            .filter(|r| r.direction == Direction::Out)
            .map(|r| (r.kind, r.report()))
            .collect()
    }

    #[test]
    fn decodes_every_captured_report() {
        let reports = captured();
        assert!(reports.len() > 100);
        let mut modes = std::collections::BTreeSet::new();
        for (kind, report) in &reports {
            let led = parse_led_report(report).unwrap();
            assert_eq!(Some(led.module_code), module_code(*kind));
            assert!(led.index < kind.capabilities().leds);
            if let LedCommand::Mode(mode) = led.command {
                modes.insert(mode.code());
            }
        }
        // Every effect of the vendor application's menu was selected once.
        assert_eq!(
            modes.into_iter().collect::<Vec<_>>(),
            [0, 1, 2, 3, 5, 6, 7, 8, 9]
        );
    }

    #[test]
    fn encoder_reproduces_captured_reports() {
        // Compare only the bytes the command defines: the application sends
        // stale data after them.
        for (kind, report) in captured() {
            let led = parse_led_report(&report).unwrap();
            if led.persist {
                continue;
            }
            let ours = led_report(kind, led.index, led.command).unwrap();
            let defined = match led.command {
                LedCommand::Mode(_) => 6,
                LedCommand::Color { .. } => 7,
                LedCommand::Brightness(_) => 5,
            };
            assert_eq!(ours[..defined], report[..defined]);
            assert!(ours[defined..].iter().all(|b| *b == 0));
        }
    }

    #[test]
    fn capture_contains_the_persist_flag_we_never_send() {
        let persisted: Vec<LedReport> = captured()
            .iter()
            .map(|(_, r)| parse_led_report(r).unwrap())
            .filter(|l| l.persist)
            .collect();
        assert!(!persisted.is_empty());
        assert!(
            persisted
                .iter()
                .all(|l| matches!(l.command, LedCommand::Color { .. }))
        );
        for mode in LedMode::ALL {
            let report = led_report(ModuleKind::Faders5, 0, LedCommand::Mode(mode)).unwrap();
            assert_eq!(report[5], 0);
        }
        let color = LedCommand::Color { r: 1, g: 2, b: 3 };
        assert_eq!(led_report(ModuleKind::Base, 0, color).unwrap()[7], 0);
    }

    #[test]
    fn known_examples() {
        assert_eq!(
            led_report(
                ModuleKind::Faders5,
                4,
                LedCommand::Color {
                    r: 0x74,
                    g: 0x3a,
                    b: 0xdf
                }
            )
            .unwrap()[..8],
            [0x02, 0x2d, 0x01, 0x04, 0x74, 0x3a, 0xdf, 0x00]
        );
        assert_eq!(
            led_report(ModuleKind::Base, 0, LedCommand::Brightness(50)).unwrap()[..5],
            [0x02, 0x2e, 0x00, 0x00, 0x32]
        );
        assert_eq!(
            led_report(ModuleKind::Knobs3, 2, LedCommand::Brightness(50)).unwrap()[..5],
            [0x02, 0x2e, 0x02, 0x02, 0x32]
        );
        assert_eq!(
            led_report(ModuleKind::Faders5, 0, LedCommand::Mode(LedMode::Rainbow)).unwrap()[..6],
            [0x02, 0x2c, 0x01, 0x00, 0x09, 0x00]
        );
    }

    #[test]
    fn rejects_unverified_requests() {
        let on = LedCommand::Mode(LedMode::Static);
        for kind in [
            ModuleKind::Rollers2,
            ModuleKind::Keys15,
            ModuleKind::DialDisplay,
        ] {
            assert!(led_report(kind, 0, on).is_err());
        }
        assert!(led_report(ModuleKind::Base, 1, on).is_err());
        assert!(led_report(ModuleKind::Knobs3, 3, on).is_err());
        assert!(led_report(ModuleKind::Faders5, 5, on).is_err());
        assert!(led_report(ModuleKind::Faders5, 0, LedCommand::Brightness(101)).is_err());
    }

    #[test]
    fn parser_rejects_other_reports() {
        assert!(parse_led_report(&[0x02, 0x2a, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(parse_led_report(&[0x02, 0x2c, 0, 0, 4, 0, 0, 0]).is_err());
        assert!(parse_led_report(&[0x02, 0x2e, 0, 0, 101, 0, 0, 0]).is_err());
        assert!(parse_led_report(&[0x02, 0x2c, 0, 0]).is_err());
        assert!(parse_led_report(&[0x01, 0x2c, 0, 0, 1, 0, 0, 0]).is_err());
    }
}
