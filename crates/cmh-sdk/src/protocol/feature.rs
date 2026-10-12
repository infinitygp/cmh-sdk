//! Feature reports.
//!
//! Every feature report is 32 bytes including the report ID. Reads answer
//! with `<id> <payload length> <payload>`. Writes all go through report
//! `0x03` as `03 <command> <arguments>`.
//!
//! Only reports that the vendor application was observed to use are encoded
//! here (docs/experiments/0003).

use crate::devices::{Family, ModuleKind};
use crate::error::{Error, Result};
use crate::protocol::input::FADER_MAX;

/// Feature report size including the report ID.
pub const FEATURE_REPORT_LEN: usize = 32;
/// Report ID used for every feature write.
pub const COMMAND_REPORT_ID: u8 = 0x03;

/// Feature report holding the running firmware version.
pub const fn firmware_report_id(kind: ModuleKind) -> u8 {
    match kind.family() {
        Family::Basic => 0x2e,
        Family::Keys => 0x38,
        Family::Dial => 0x48,
    }
}

/// Feature report holding the device description.
pub const fn device_info_report_id(kind: ModuleKind) -> u8 {
    match kind.family() {
        Family::Basic => 0x31,
        Family::Keys => 0x3b,
        Family::Dial => 0x4b,
    }
}

/// Feature report holding the current fader positions (5 Faders only).
pub const FADER_POSITIONS_REPORT_ID: u8 = 0x33;
/// Feature report holding the LED brightness of every zone (basic modules
/// with LEDs): one byte per zone, `0..=100`.
pub const LED_BRIGHTNESS_REPORT_ID: u8 = 0x32;

/// Feature report holding the backlight level of a display module.
pub const fn display_brightness_report_id(kind: ModuleKind) -> Option<u8> {
    match kind.family() {
        Family::Keys => Some(0x3c),
        Family::Dial => Some(0x4c),
        Family::Basic => None,
    }
}

/// Checks a per-zone payload: `zones` bytes, none above `max`.
pub fn parse_zone_values(payload: &[u8], zones: u8, max: u8) -> Result<Vec<u8>> {
    if payload.len() != usize::from(zones) {
        return Err(Error::MalformedReport(format!(
            "expected {zones} zone value(s), got {}",
            payload.len()
        )));
    }
    match payload.iter().find(|v| **v > max) {
        Some(bad) => Err(Error::MalformedReport(format!(
            "zone value {bad} exceeds {max}"
        ))),
        None => Ok(payload.to_vec()),
    }
}

/// Parses the payload of the fader position report: one byte per fader,
/// `0..=100`.
pub fn parse_fader_positions(payload: &[u8]) -> Result<[u8; 5]> {
    let positions: [u8; 5] = payload.try_into().map_err(|_| {
        Error::MalformedReport(format!(
            "fader payload has {} bytes, expected 5",
            payload.len()
        ))
    })?;
    match positions.iter().find(|p| **p > FADER_MAX) {
        Some(bad) => Err(Error::MalformedReport(format!(
            "fader position {bad} out of range"
        ))),
        None => Ok(positions),
    }
}

/// Extracts the payload of a feature read response.
pub fn feature_payload(report: &[u8], expected_id: u8) -> Result<&[u8]> {
    let [id, len, rest @ ..] = report else {
        return Err(Error::MalformedReport(format!(
            "feature report too short: {} bytes",
            report.len()
        )));
    };
    if *id != expected_id {
        return Err(Error::MalformedReport(format!(
            "expected feature report {expected_id:#04x}, got {id:#04x}"
        )));
    }
    rest.get(..usize::from(*len)).ok_or_else(|| {
        Error::MalformedReport(format!(
            "feature report {id:#04x} declares {len} payload bytes but carries {}",
            rest.len()
        ))
    })
}

/// Firmware identification of a module.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FirmwareInfo {
    /// Four bytes preceding the version text. They differ per firmware
    /// build; their meaning is unknown (possibly a checksum).
    pub build_tag: [u8; 4],
    /// Version text such as `02.00.04.00`.
    pub version: String,
}

impl FirmwareInfo {
    /// Parses the payload of the firmware feature report.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let Some((tag, text)) = payload.split_first_chunk::<4>() else {
            return Err(Error::MalformedReport(format!(
                "firmware payload too short: {} bytes",
                payload.len()
            )));
        };
        let text = text.split(|b| *b == 0).next().unwrap_or_default();
        if text.is_empty() || !text.iter().all(|b| b.is_ascii_digit() || *b == b'.') {
            return Err(Error::MalformedReport(
                "firmware version is not dotted decimal text".to_owned(),
            ));
        }
        Ok(Self {
            build_tag: *tag,
            version: String::from_utf8_lossy(text).into_owned(),
        })
    }
}

/// Display geometry reported by the modules that have a screen.
///
/// The image size matches the JPEG frames the vendor application uploads.
/// The grid and panel fields are interpretations of the remaining bytes and
/// have not been verified independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DisplayInfo {
    pub rows: u8,
    pub columns: u8,
    pub keys: u8,
    pub panel_width: u16,
    pub panel_height: u16,
    pub image_width: u8,
    pub image_height: u8,
}

impl DisplayInfo {
    /// Parses the payload of the device info report of a display module.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let &[
            _,
            rows,
            columns,
            keys,
            w0,
            w1,
            h0,
            h1,
            _,
            image_width,
            image_height,
        ] = payload
        else {
            return Err(Error::MalformedReport(format!(
                "display info payload has {} bytes, expected 11",
                payload.len()
            )));
        };
        Ok(Self {
            rows,
            columns,
            keys,
            panel_width: u16::from_le_bytes([w0, w1]),
            panel_height: u16::from_le_bytes([h0, h1]),
            image_width,
            image_height,
        })
    }
}

/// Builds the feature write that switches a display module between
/// application control (`true`) and standalone operation (`false`).
///
/// The vendor application sends `true` before it uploads images and
/// `false` to both display modules when it exits (experiment 0026).
pub fn software_mode_report(kind: ModuleKind, enabled: bool) -> Result<[u8; FEATURE_REPORT_LEN]> {
    let command = match (kind.family(), enabled) {
        (Family::Keys, _) => 0x3a,
        (Family::Dial, _) => 0x49,
        (Family::Basic, _) => {
            return Err(Error::Unsupported {
                module: kind.name(),
                capability: "software mode",
            });
        }
    };
    let mut report = [0u8; FEATURE_REPORT_LEN];
    report[0] = COMMAND_REPORT_ID;
    report[1] = command;
    report[2] = u8::from(enabled);
    Ok(report)
}

/// Highest display brightness.
pub const DISPLAY_BRIGHTNESS_MAX: u8 = 100;

/// Builds the feature write that sets the backlight of a display module,
/// `0..=DISPLAY_BRIGHTNESS_MAX`.
///
/// The vendor application has been seen sending 40..=55 in steps of 5; the
/// rest of the range is assumed from the LED brightness scale.
pub fn display_brightness_report(kind: ModuleKind, level: u8) -> Result<[u8; FEATURE_REPORT_LEN]> {
    let command = match kind.family() {
        Family::Keys => 0x34,
        Family::Dial => 0x44,
        Family::Basic => {
            return Err(Error::Unsupported {
                module: kind.name(),
                capability: "display brightness",
            });
        }
    };
    if level > DISPLAY_BRIGHTNESS_MAX {
        return Err(Error::InvalidArgument(format!(
            "display brightness {level} exceeds {DISPLAY_BRIGHTNESS_MAX}"
        )));
    }
    let mut report = [0u8; FEATURE_REPORT_LEN];
    report[0] = COMMAND_REPORT_ID;
    report[1] = command;
    report[2] = level;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(bytes: &[u8]) -> [u8; FEATURE_REPORT_LEN] {
        let mut report = [0u8; FEATURE_REPORT_LEN];
        report[..bytes.len()].copy_from_slice(bytes);
        report
    }

    // Responses recorded while the vendor application started (experiment 0003).
    const BASE_FIRMWARE: &[u8] = &[
        0x2e, 0x0f, 0x12, 0x54, 0x4c, 0x2f, 0x30, 0x32, 0x2e, 0x30, 0x30, 0x2e, 0x30, 0x34, 0x2e,
        0x30, 0x30,
    ];
    const KEYS_INFO: &[u8] = &[
        0x3b, 0x0b, 0x14, 0x03, 0x05, 0x0f, 0xe0, 0x01, 0x10, 0x01, 0x01, 0x48, 0x48,
    ];
    const DIAL_INFO: &[u8] = &[
        0x4b, 0x0b, 0x14, 0x00, 0x00, 0x00, 0xf0, 0x00, 0xf0, 0x00, 0x01, 0xf0, 0xf0,
    ];

    #[test]
    fn parses_firmware_version() {
        let report = padded(BASE_FIRMWARE);
        let payload = feature_payload(&report, firmware_report_id(ModuleKind::Base)).unwrap();
        let info = FirmwareInfo::parse(payload).unwrap();
        assert_eq!(info.version, "02.00.04.00");
        assert_eq!(info.build_tag, [0x12, 0x54, 0x4c, 0x2f]);
    }

    #[test]
    fn parses_display_info() {
        let report = padded(KEYS_INFO);
        let payload = feature_payload(&report, device_info_report_id(ModuleKind::Keys15)).unwrap();
        assert_eq!(
            DisplayInfo::parse(payload).unwrap(),
            DisplayInfo {
                rows: 3,
                columns: 5,
                keys: 15,
                panel_width: 480,
                panel_height: 272,
                image_width: 72,
                image_height: 72,
            }
        );

        let report = padded(DIAL_INFO);
        let payload = feature_payload(&report, 0x4b).unwrap();
        let info = DisplayInfo::parse(payload).unwrap();
        assert_eq!((info.image_width, info.image_height), (240, 240));
        assert_eq!(info.keys, 0);
    }

    #[test]
    fn parses_fader_positions() {
        // Read by the vendor application in experiment 0005.
        let report = padded(&[0x33, 0x05, 0x2c, 0x2d, 0x39, 0x3a, 0x38]);
        let payload = feature_payload(&report, FADER_POSITIONS_REPORT_ID).unwrap();
        assert_eq!(
            parse_fader_positions(payload).unwrap(),
            [44, 45, 57, 58, 56]
        );
        assert!(parse_fader_positions(&[0, 0, 0, 0]).is_err());
        assert!(parse_fader_positions(&[0, 0, 0, 0, 101]).is_err());
    }

    #[test]
    fn display_brightness_matches_capture() {
        use crate::fixture::{Direction, Fixture};

        // Experiment 0009: one knob detent changes the level by 5.
        let text = include_str!("../../../../fixtures/display/vendor-app-brightness.txt");
        let fixture = Fixture::parse(text).unwrap();
        let mut levels = Vec::new();
        for record in fixture.records() {
            assert_eq!(record.direction, Direction::Set);
            let captured = record.report();
            let ours = display_brightness_report(record.kind, captured[2]).unwrap();
            assert_eq!(ours[..3], captured[..3]);
            assert!(ours[3..].iter().all(|b| *b == 0));
            if record.kind == ModuleKind::Keys15 {
                levels.push(captured[2]);
            }
        }
        assert_eq!(levels, [55, 50, 45, 40, 45, 50, 55]);

        assert!(display_brightness_report(ModuleKind::Keys15, 101).is_err());
        assert!(display_brightness_report(ModuleKind::Knobs3, 50).is_err());
        assert_eq!(
            display_brightness_report(ModuleKind::DialDisplay, 100).unwrap()[..3],
            [0x03, 0x44, 0x64]
        );
    }

    #[test]
    fn parses_zone_read_backs() {
        // Read from the 3 Knobs and the base in experiment 0018.
        let knobs = padded(&[0x32, 0x03, 0x64, 0x64, 0x64]);
        let payload = feature_payload(&knobs, LED_BRIGHTNESS_REPORT_ID).unwrap();
        assert_eq!(parse_zone_values(payload, 3, 100).unwrap(), [100, 100, 100]);
        let keys = padded(&[0x3c, 0x01, 0x32]);
        let id = display_brightness_report_id(ModuleKind::Keys15).unwrap();
        assert_eq!(
            parse_zone_values(feature_payload(&keys, id).unwrap(), 1, 100).unwrap(),
            [50]
        );

        assert!(parse_zone_values(&[1, 2], 3, 100).is_err());
        assert!(parse_zone_values(&[101], 1, 100).is_err());
        assert_eq!(
            display_brightness_report_id(ModuleKind::DialDisplay),
            Some(0x4c)
        );
        assert_eq!(display_brightness_report_id(ModuleKind::Knobs3), None);
    }

    #[test]
    fn rejects_bad_feature_reports() {
        assert!(feature_payload(&padded(BASE_FIRMWARE), 0x2d).is_err());
        assert!(feature_payload(&[0x2e, 0x0f, 0x01], 0x2e).is_err());
        assert!(feature_payload(&[0x2e], 0x2e).is_err());
        assert!(FirmwareInfo::parse(&[1, 2, 3]).is_err());
        assert!(FirmwareInfo::parse(&[1, 2, 3, 4, b'x']).is_err());
    }

    #[test]
    fn software_mode_matches_capture() {
        // The application sends the same leading bytes; the rest of its
        // buffer is uninitialised and is not reproduced here.
        let keys_on = software_mode_report(ModuleKind::Keys15, true).unwrap();
        assert_eq!(keys_on[..3], [0x03, 0x3a, 0x01]);
        assert!(keys_on[3..].iter().all(|b| *b == 0));
        assert_eq!(
            software_mode_report(ModuleKind::Keys15, false).unwrap()[..3],
            [0x03, 0x3a, 0x00]
        );
        assert_eq!(
            software_mode_report(ModuleKind::DialDisplay, true).unwrap()[..3],
            [0x03, 0x49, 0x01]
        );
        // Sent by the vendor application when it exits (experiment 0026).
        assert_eq!(
            software_mode_report(ModuleKind::DialDisplay, false).unwrap()[..3],
            [0x03, 0x49, 0x00]
        );
        assert!(software_mode_report(ModuleKind::Knobs3, true).is_err());
    }
}
