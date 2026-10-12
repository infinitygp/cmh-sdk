//! Stateless decoding of input report payloads.
//!
//! Tags are `family base + channel`. Channel 0 carries control events as
//! `<type> <count> <one byte per control>`; channel 1 carries the base
//! heartbeat.
//!
//! | Module | Tag | Type | Meaning |
//! |---|---|---|---|
//! | Base | `0x2b` | - | heartbeat, one orientation byte |
//! | 3 Knobs | `0x2a` | `0x00` | rotation step per knob |
//! | 3 Knobs | `0x2a` | `0x03` | button state per knob |
//! | 2 Rollers | `0x2a` | `0x01` | rotation step per roller |
//! | 15 LCD Keys | `0x34` | `0x02` | button state per key |
//! | 5 Faders | `0x2a` | `0x02` | position per fader, 0..=100 |
//! | Dial Display | `0x44` | `0x03` | rotation step |
//! | Dial Display | `0x44` | `0x04` | button state |

use crate::devices::{Family, ModuleKind};
use crate::error::{Error, Result};
use crate::protocol::frame::InputFrame;

/// Rotation step code `00`: counter-clockwise on the knobs, away from the
/// operator on the rollers, but clockwise on the dial.
pub const STEP_CODE_0: u8 = 0x00;
/// Rotation step code `01`: the opposite direction of [`STEP_CODE_0`].
pub const STEP_CODE_1: u8 = 0x01;
/// Rotation step code: this encoder did not move.
pub const STEP_NONE: u8 = 0xff;
/// Highest fader position.
pub const FADER_MAX: u8 = 100;

/// A decoded input report. Slices hold one byte per control, in index order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMessage<'a> {
    /// Periodic report sent by the base: `1` while it lies flat, `0` while
    /// it stands upright on its short side.
    Heartbeat { status: u8 },
    /// Per-encoder step codes ([`STEP_CODE_0`], [`STEP_CODE_1`], [`STEP_NONE`]).
    EncoderSteps(&'a [u8]),
    /// Per-button states, `0` released and `1` pressed.
    ButtonStates(&'a [u8]),
    /// Per-fader positions, `0..=FADER_MAX`.
    FaderPositions(&'a [u8]),
    /// A well-framed report this SDK does not understand yet.
    Unknown { tag: u8, payload: &'a [u8] },
}

struct Layout {
    tag_base: u8,
    encoders: Option<(u8, u8)>,
    buttons: Option<(u8, u8)>,
    faders: Option<(u8, u8)>,
}

const fn layout(kind: ModuleKind) -> Layout {
    let tag_base = match kind.family() {
        Family::Basic => 0x2a,
        Family::Keys => 0x34,
        Family::Dial => 0x44,
    };
    let (encoders, buttons, faders) = match kind {
        ModuleKind::Knobs3 => (Some((0x00, 3)), Some((0x03, 3)), None),
        ModuleKind::Rollers2 => (Some((0x01, 2)), None, None),
        ModuleKind::Keys15 => (None, Some((0x02, 15)), None),
        ModuleKind::DialDisplay => (Some((0x03, 1)), Some((0x04, 1)), None),
        ModuleKind::Faders5 => (None, None, Some((0x02, 5))),
        ModuleKind::Base => (None, None, None),
    };
    Layout {
        tag_base,
        encoders,
        buttons,
        faders,
    }
}

/// Signed step for a raw step code of `kind`, `None` when the encoder did
/// not move.
///
/// `+1` is clockwise on the knobs and on the dial, and towards the operator
/// on the rollers. The dial reports clockwise as code `00`, the knobs as
/// code `01`.
pub fn step_delta(kind: ModuleKind, code: u8) -> Option<i8> {
    let delta = match code {
        STEP_CODE_0 => -1,
        STEP_CODE_1 => 1,
        _ => return None,
    };
    Some(if kind == ModuleKind::DialDisplay {
        -delta
    } else {
        delta
    })
}

impl<'a> InputMessage<'a> {
    /// Decodes a raw input report (including its report ID) sent by `kind`.
    pub fn parse(kind: ModuleKind, report: &'a [u8]) -> Result<Self> {
        let frame = InputFrame::parse(report)?;
        let layout = layout(kind);
        let unknown = InputMessage::Unknown {
            tag: frame.tag,
            payload: frame.payload,
        };

        if kind == ModuleKind::Base && frame.tag == layout.tag_base + 1 {
            return match frame.payload {
                [status] => Ok(InputMessage::Heartbeat { status: *status }),
                _ => Ok(unknown),
            };
        }
        if frame.tag != layout.tag_base {
            return Ok(unknown);
        }
        let [kind_code, count, values @ ..] = frame.payload else {
            return Ok(unknown);
        };

        let expect = |expected: u8, valid: fn(u8) -> bool| -> Result<()> {
            if *count != expected || values.len() != usize::from(expected) {
                return Err(Error::MalformedReport(format!(
                    "type {kind_code:#04x} from {kind}: expected {expected} values, header says {count}, got {}",
                    values.len()
                )));
            }
            match values.iter().find(|v| !valid(**v)) {
                Some(bad) => Err(Error::MalformedReport(format!(
                    "type {kind_code:#04x} from {kind}: unexpected value {bad:#04x}"
                ))),
                None => Ok(()),
            }
        };

        if let Some((code, n)) = layout.encoders
            && code == *kind_code
        {
            expect(n, |v| matches!(v, STEP_CODE_0 | STEP_CODE_1 | STEP_NONE))?;
            return Ok(InputMessage::EncoderSteps(values));
        }
        if let Some((code, n)) = layout.buttons
            && code == *kind_code
        {
            expect(n, |v| v <= 1)?;
            return Ok(InputMessage::ButtonStates(values));
        }
        if let Some((code, n)) = layout.faders
            && code == *kind_code
        {
            expect(n, |v| v <= FADER_MAX)?;
            return Ok(InputMessage::FaderPositions(values));
        }
        Ok(unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(kind: ModuleKind, bytes: &[u8]) -> Result<InputMessage<'_>> {
        InputMessage::parse(kind, bytes)
    }

    // Every report below was recorded from hardware (docs/experiments/0002).

    #[test]
    fn base_heartbeat() {
        assert_eq!(
            parse(ModuleKind::Base, &[0x01, 0x2b, 0x01, 0x00, 0x01]).unwrap(),
            InputMessage::Heartbeat { status: 1 }
        );
    }

    #[test]
    fn knob_rotation_and_press() {
        assert_eq!(
            parse(
                ModuleKind::Knobs3,
                &[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0xff, 0x01, 0xff]
            )
            .unwrap(),
            InputMessage::EncoderSteps(&[0xff, 0x01, 0xff])
        );
        assert_eq!(
            parse(
                ModuleKind::Knobs3,
                &[0x01, 0x2a, 0x05, 0x00, 0x03, 0x03, 0x00, 0x00, 0x01]
            )
            .unwrap(),
            InputMessage::ButtonStates(&[0, 0, 1])
        );
    }

    #[test]
    fn roller_rotation() {
        assert_eq!(
            parse(
                ModuleKind::Rollers2,
                &[0x01, 0x2a, 0x04, 0x00, 0x01, 0x02, 0x00, 0xff]
            )
            .unwrap(),
            InputMessage::EncoderSteps(&[0x00, 0xff])
        );
    }

    #[test]
    fn key_states() {
        let mut report = vec![0x01, 0x34, 0x11, 0x00, 0x02, 0x0f];
        report.extend([0u8; 15]);
        report[6 + 14] = 1;
        let InputMessage::ButtonStates(states) = parse(ModuleKind::Keys15, &report).unwrap() else {
            panic!("expected button states");
        };
        assert_eq!(states.len(), 15);
        assert_eq!(states[14], 1);
    }

    #[test]
    fn dial_rotation_and_press() {
        assert_eq!(
            parse(
                ModuleKind::DialDisplay,
                &[0x01, 0x44, 0x03, 0x00, 0x03, 0x01, 0x01]
            )
            .unwrap(),
            InputMessage::EncoderSteps(&[0x01])
        );
        assert_eq!(
            parse(
                ModuleKind::DialDisplay,
                &[0x01, 0x44, 0x03, 0x00, 0x04, 0x01, 0x01]
            )
            .unwrap(),
            InputMessage::ButtonStates(&[1])
        );
    }

    #[test]
    fn clockwise_is_positive_on_knobs_and_dial() {
        // Experiment 0009: one detent clockwise was `01` on a knob and `00`
        // on the dial.
        assert_eq!(step_delta(ModuleKind::Knobs3, 0x01), Some(1));
        assert_eq!(step_delta(ModuleKind::DialDisplay, 0x00), Some(1));
        assert_eq!(step_delta(ModuleKind::DialDisplay, 0x01), Some(-1));
        assert_eq!(step_delta(ModuleKind::Rollers2, 0x00), Some(-1));
        assert_eq!(step_delta(ModuleKind::Knobs3, 0xff), None);
    }

    #[test]
    fn same_type_code_means_different_things_per_module() {
        // 0x03 is "buttons" on the knobs but "rotation" on the dial.
        let knobs = [0x01, 0x2a, 0x05, 0x00, 0x03, 0x03, 0x01, 0x00, 0x00];
        assert!(matches!(
            parse(ModuleKind::Knobs3, &knobs).unwrap(),
            InputMessage::ButtonStates(_)
        ));
    }

    #[test]
    fn unknown_reports_are_preserved() {
        assert_eq!(
            parse(ModuleKind::Knobs3, &[0x01, 0x2c, 0x02, 0x00, 0xaa, 0xbb]).unwrap(),
            InputMessage::Unknown {
                tag: 0x2c,
                payload: &[0xaa, 0xbb]
            }
        );
    }

    #[test]
    fn fader_positions() {
        // Recorded in experiment 0005.
        assert_eq!(
            parse(
                ModuleKind::Faders5,
                &[
                    0x01, 0x2a, 0x07, 0x00, 0x02, 0x05, 0x64, 0x64, 0x64, 0x64, 0x61
                ]
            )
            .unwrap(),
            InputMessage::FaderPositions(&[100, 100, 100, 100, 97])
        );
        assert!(
            parse(
                ModuleKind::Faders5,
                &[0x01, 0x2a, 0x07, 0x00, 0x02, 0x05, 0x65, 0, 0, 0, 0]
            )
            .is_err()
        );
    }

    #[test]
    fn malformed_known_reports_are_rejected() {
        // Count byte disagrees with the module.
        assert!(
            parse(
                ModuleKind::Knobs3,
                &[0x01, 0x2a, 0x04, 0x00, 0x00, 0x02, 0xff, 0xff]
            )
            .is_err()
        );
        // Step code outside 00 / 01 / ff.
        assert!(
            parse(
                ModuleKind::Knobs3,
                &[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0x02, 0xff, 0xff]
            )
            .is_err()
        );
        // Button state outside 0 / 1.
        assert!(
            parse(
                ModuleKind::DialDisplay,
                &[0x01, 0x44, 0x03, 0x00, 0x04, 0x01, 0x07]
            )
            .is_err()
        );
    }
}
