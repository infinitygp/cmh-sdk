//! Typed input events.

use crate::devices::ModuleKind;
use crate::error::Result;
use crate::protocol::input::{InputMessage, step_delta};

/// Something the user did on a module. `index` counts controls from 0.
///
/// Index order as verified on hardware:
/// - 15 LCD Keys: row by row, starting at the key furthest from the logo
///   bar on the left.
/// - 5 Faders: left to right with the faders upright and position 0 at
///   the bottom.
/// - 2 Rollers: index 0 is the right roller when the module sits along the
///   edge nearest the operator.
/// - 3 Knobs: with the single knob of the triangle pointing away from the
///   operator, 0 is the left knob, 1 the right knob and 2 the single one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InputEvent {
    ButtonDown {
        index: u8,
    },
    ButtonUp {
        index: u8,
    },
    /// One detent of rotation. `+1` is clockwise on the knobs and the dial,
    /// and towards the operator (down) on the rollers.
    EncoderDelta {
        index: u8,
        delta: i8,
    },
    /// A fader moved; `value` is its new position, `0..=100`.
    ///
    /// A fader resting between two positions may alternate between them.
    AnalogChanged {
        index: u8,
        value: u8,
    },
}

const MAX_BUTTONS: usize = 15;
const MAX_FADERS: usize = 5;

/// Turns raw input reports of one module into [`InputEvent`]s.
///
/// Button and fader reports carry the state of every control, so the decoder
/// keeps the previous state to emit only what changed. Use one decoder per
/// connected module and [`reset`](Self::reset) it after a reconnect.
#[derive(Debug, Clone)]
pub struct InputDecoder {
    kind: ModuleKind,
    pressed: [bool; MAX_BUTTONS],
    /// `None` until a position is known; the first report then yields an
    /// event for every fader.
    positions: [Option<u8>; MAX_FADERS],
}

impl InputDecoder {
    pub fn new(kind: ModuleKind) -> Self {
        Self {
            kind,
            pressed: [false; MAX_BUTTONS],
            positions: [None; MAX_FADERS],
        }
    }

    pub fn kind(&self) -> ModuleKind {
        self.kind
    }

    /// Forgets which buttons are held and where the faders are.
    pub fn reset(&mut self) {
        self.pressed = [false; MAX_BUTTONS];
        self.positions = [None; MAX_FADERS];
    }

    /// Records fader positions obtained out of band (feature report), so
    /// that only later movement produces events.
    pub fn set_fader_positions(&mut self, positions: &[u8]) {
        for (known, position) in self.positions.iter_mut().zip(positions) {
            *known = Some(*position);
        }
    }

    /// Decodes one raw input report (including its report ID).
    ///
    /// Heartbeats and reports that are not understood yield no events.
    pub fn decode(&mut self, report: &[u8]) -> Result<Vec<InputEvent>> {
        let mut events = Vec::new();
        match InputMessage::parse(self.kind, report)? {
            InputMessage::EncoderSteps(steps) => {
                for (index, step) in steps.iter().enumerate() {
                    if let Some(delta) = step_delta(self.kind, *step) {
                        events.push(InputEvent::EncoderDelta {
                            index: index as u8,
                            delta,
                        });
                    }
                }
            }
            InputMessage::ButtonStates(states) => {
                for (index, (state, pressed)) in
                    states.iter().zip(self.pressed.iter_mut()).enumerate()
                {
                    let now = *state != 0;
                    if now != *pressed {
                        *pressed = now;
                        let index = index as u8;
                        events.push(if now {
                            InputEvent::ButtonDown { index }
                        } else {
                            InputEvent::ButtonUp { index }
                        });
                    }
                }
            }
            InputMessage::FaderPositions(positions) => {
                for (index, (position, known)) in
                    positions.iter().zip(self.positions.iter_mut()).enumerate()
                {
                    if *known != Some(*position) {
                        *known = Some(*position);
                        events.push(InputEvent::AnalogChanged {
                            index: index as u8,
                            value: *position,
                        });
                    }
                }
            }
            InputMessage::Heartbeat { .. } | InputMessage::Unknown { .. } => {}
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{Direction, Fixture};

    const SESSION: &str = include_str!("../../../fixtures/inputs/all-controls.txt");

    fn decode_session() -> Vec<(ModuleKind, InputEvent)> {
        let fixture = Fixture::parse(SESSION).unwrap();
        let mut decoders: Vec<InputDecoder> = Vec::new();
        let mut events = Vec::new();
        for record in fixture.records() {
            assert_eq!(record.direction, Direction::In);
            let kind = record.kind;
            let decoder = match decoders.iter_mut().position(|d| d.kind() == kind) {
                Some(i) => &mut decoders[i],
                None => {
                    decoders.push(InputDecoder::new(kind));
                    decoders.last_mut().unwrap()
                }
            };
            for event in decoder.decode(&record.report()).unwrap() {
                events.push((kind, event));
            }
        }
        events
    }

    fn of(events: &[(ModuleKind, InputEvent)], kind: ModuleKind) -> Vec<InputEvent> {
        events
            .iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, e)| *e)
            .collect()
    }

    use InputEvent::{ButtonDown as Down, ButtonUp as Up, EncoderDelta as Turn};

    fn turns(index: u8, delta: i8, count: usize) -> Vec<InputEvent> {
        vec![Turn { index, delta }; count]
    }

    fn clicks(index: u8, count: usize) -> Vec<InputEvent> {
        [Down { index }, Up { index }].repeat(count)
    }

    // The expectations below restate what was physically done during the
    // recording (docs/experiments/0002-input-controls.md).

    #[test]
    fn knobs_session() {
        let expected = [
            turns(0, -1, 3),
            turns(0, 1, 3),
            clicks(0, 3),
            turns(2, -1, 3),
            turns(2, 1, 3),
            clicks(2, 3),
            turns(1, -1, 3),
            turns(1, 1, 3),
            clicks(1, 1),
        ]
        .concat();
        assert_eq!(of(&decode_session(), ModuleKind::Knobs3), expected);
    }

    #[test]
    fn rollers_session() {
        let expected = [
            turns(1, -1, 4),
            turns(1, 1, 2),
            turns(0, -1, 4),
            turns(0, 1, 4),
        ]
        .concat();
        assert_eq!(of(&decode_session(), ModuleKind::Rollers2), expected);
    }

    #[test]
    fn keys_session() {
        let mut expected: Vec<InputEvent> = (0..15).flat_map(|i| clicks(i, 1)).collect();
        // Two keys held together, released in the same report.
        expected.extend([
            Down { index: 0 },
            Down { index: 1 },
            Up { index: 0 },
            Up { index: 1 },
        ]);
        assert_eq!(of(&decode_session(), ModuleKind::Keys15), expected);
    }

    #[test]
    fn dial_session() {
        // The dial was turned clockwise first (raw step code 00).
        let expected = [turns(0, 1, 2), turns(0, -1, 3), clicks(0, 4)].concat();
        assert_eq!(of(&decode_session(), ModuleKind::DialDisplay), expected);
    }

    #[test]
    fn base_heartbeat_yields_no_events() {
        assert!(of(&decode_session(), ModuleKind::Base).is_empty());
    }

    #[test]
    fn reset_forgets_held_buttons() {
        let mut decoder = InputDecoder::new(ModuleKind::DialDisplay);
        let down = [0x01, 0x44, 0x03, 0x00, 0x04, 0x01, 0x01];
        assert_eq!(decoder.decode(&down).unwrap(), [Down { index: 0 }]);
        assert_eq!(decoder.decode(&down).unwrap(), []);
        decoder.reset();
        assert_eq!(decoder.decode(&down).unwrap(), [Down { index: 0 }]);
    }

    #[test]
    fn faders_session() {
        let text = include_str!("../../../fixtures/inputs/faders.txt");
        let fixture = Fixture::parse(text).unwrap();
        let mut decoder = InputDecoder::new(ModuleKind::Faders5);
        let mut events = Vec::new();
        for record in fixture.records() {
            events.extend(decoder.decode(&record.report()).unwrap());
        }
        // The first report announces all five positions.
        assert_eq!(
            events[..5],
            [100, 100, 100, 100, 97].map({
                let mut index = 0;
                move |value| {
                    index += 1;
                    InputEvent::AnalogChanged {
                        index: index - 1,
                        value,
                    }
                }
            })
        );
        // Afterwards exactly one fader changes per report. Every fader was
        // pulled to 0; the recording starts with them at or near 100.
        assert_eq!(events.len(), fixture.records().len() + 4);
        for index in 0..5 {
            let values: Vec<u8> = events
                .iter()
                .filter_map(|e| match e {
                    InputEvent::AnalogChanged { index: i, value } if *i == index => Some(*value),
                    _ => None,
                })
                .collect();
            assert_eq!(values.iter().min(), Some(&0));
            assert!(values.iter().max() >= Some(&97));
        }
    }

    #[test]
    fn a_rejected_report_leaves_state_untouched() {
        let mut decoder = InputDecoder::new(ModuleKind::DialDisplay);
        let down = [0x01, 0x44, 0x03, 0x00, 0x04, 0x01, 0x01];
        let bad = [0x01, 0x44, 0x03, 0x00, 0x04, 0x01, 0x09];
        assert_eq!(decoder.decode(&down).unwrap(), [Down { index: 0 }]);
        assert!(decoder.decode(&bad).is_err());
        assert_eq!(decoder.decode(&down).unwrap(), []);
    }

    #[test]
    fn never_panics_on_arbitrary_bytes() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for kind in ModuleKind::ALL {
            let mut decoder = InputDecoder::new(kind);
            for _ in 0..20_000 {
                let len = (next() % 70) as usize;
                let mut report: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                // Bias towards well-framed reports so the payload parsers run.
                if len >= 6 && next() % 4 != 0 {
                    report[0] = 0x01;
                    report[1] = [0x2a, 0x2b, 0x34, 0x44][(next() % 4) as usize];
                    report[2] = (next() % 20) as u8;
                    report[3] = 0;
                    report[4] = (next() % 6) as u8;
                }
                let _ = decoder.decode(&report);
            }
        }
    }

    #[test]
    fn one_detent_clockwise_per_control() {
        // Experiment 0009: left knob, right knob, apex knob and dial, each
        // turned one detent clockwise, in a known order.
        let text = include_str!("../../../fixtures/inputs/clockwise-checks.txt");
        let fixture = Fixture::parse(text).unwrap();
        let mut knobs = InputDecoder::new(ModuleKind::Knobs3);
        let mut dial = InputDecoder::new(ModuleKind::DialDisplay);
        let mut events = Vec::new();
        for record in fixture.records() {
            let decoder = match record.kind {
                ModuleKind::Knobs3 => &mut knobs,
                ModuleKind::DialDisplay => &mut dial,
                other => panic!("unexpected module {other}"),
            };
            events.extend(
                decoder
                    .decode(&record.report())
                    .unwrap()
                    .into_iter()
                    .map(|e| (record.kind, e)),
            );
        }
        assert_eq!(
            events,
            [
                (ModuleKind::Knobs3, Turn { index: 2, delta: 1 }),
                (ModuleKind::Knobs3, Turn { index: 0, delta: 1 }),
                (ModuleKind::Knobs3, Turn { index: 1, delta: 1 }),
                (ModuleKind::DialDisplay, Turn { index: 0, delta: 1 }),
            ]
        );
    }
}
