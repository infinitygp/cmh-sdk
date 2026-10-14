//! Test doubles shared by the unit tests.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::module::Module;
use crate::protocol::frame::INPUT_REPORT_LEN;
use crate::transport::{InputReader, Transport};

/// Everything a module was asked to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Sent {
    Output(Vec<u8>),
    Feature(Vec<u8>),
    FeatureRead(u8),
}

#[derive(Default)]
pub(crate) struct State {
    pub inputs: VecDeque<Vec<u8>>,
    pub features: Vec<Vec<u8>>,
    pub sent: Vec<Sent>,
    pub fail_output_after: Option<usize>,
    /// Timeout of every input read that was requested.
    pub read_timeouts: Vec<Duration>,
    /// Makes every operation fail the way a pulled module does.
    pub unplugged: bool,
    /// A display module in standalone mode, as observed in experiment 0021:
    /// the firmware and backlight reads come back empty and backlight
    /// writes do nothing.
    pub standalone: bool,
    /// Set by an empty read: a feature write is then carried out only when
    /// the next one arrives, and dropped by a read.
    pub lagging: bool,
    pub held: Option<Vec<u8>>,
    /// Backlight writes still to be ignored, as right after the module
    /// entered software mode.
    pub ignore_backlight: usize,
    /// Software mode commands have no effect.
    pub stuck: bool,
    /// Readers taken from this mock that are still alive, and those of them
    /// that wait for a report right now.
    pub readers: usize,
    pub blocked: usize,
    /// Reads of a reader that fail once each although the module is there.
    pub glitches: usize,
}

/// Firmware reports for the mocks of the display modules.
pub(crate) const KEYS_FIRMWARE: &[u8] = &[
    0x38, 0x0f, 0x29, 0x63, 0x1c, 0x5d, b'0', b'2', b'.', b'0', b'0', b'.', b'0', b'1', b'.', b'0',
    b'0',
];
pub(crate) const DIAL_FIRMWARE: &[u8] = &[
    0x48, 0x0f, 0x26, 0xa0, 0xd8, 0x0a, b'0', b'1', b'.', b'0', b'0', b'.', b'0', b'4', b'.', b'0',
    b'0',
];

impl State {
    fn carry_out(&mut self, command: &[u8]) {
        match (command[1], command[2]) {
            (0x3a | 0x49, 1) if !self.stuck => self.standalone = false,
            (0x3a | 0x49, 0) => self.standalone = true,
            (0x34 | 0x44, _) if self.standalone => {}
            (0x34 | 0x44, _) if self.ignore_backlight > 0 => self.ignore_backlight -= 1,
            (0x34 | 0x44, level) => {
                let id = command[1] + 0x08;
                self.features.retain(|f| f[0] != id);
                self.features.push(vec![id, 0x01, level]);
            }
            _ => {}
        }
    }
}

/// A scripted transport. Clones share their state, so a test can keep one
/// handle while the module owns another. The second field wakes the readers
/// when something is pushed.
#[derive(Clone, Default)]
pub(crate) struct Mock(pub Arc<Mutex<State>>, Arc<Condvar>);

impl Mock {
    pub fn with_features(features: &[&[u8]]) -> Self {
        let mock = Mock::default();
        mock.0.lock().unwrap().features = features.iter().map(|f| f.to_vec()).collect();
        mock
    }

    pub fn push_input(&self, report: &[u8]) {
        self.0.lock().unwrap().inputs.push_back(report.to_vec());
        self.1.notify_all();
    }

    pub fn sent(&self) -> Vec<Sent> {
        self.0.lock().unwrap().sent.clone()
    }

    pub fn unplug(&self) {
        self.0.lock().unwrap().unplugged = true;
        self.1.notify_all();
    }

    /// Makes the next read of a reader fail once, as hidapi's does when a
    /// signal interrupts its wait, and ends the read that waits now.
    pub fn glitch(&self) {
        self.0.lock().unwrap().glitches += 1;
        self.1.notify_all();
    }

    /// The reading side of this mock, as the deck's reader thread gets it:
    /// a read waits for a report, for the timeout or for the unplugging.
    pub fn reader(&self) -> MockReader {
        self.0.lock().unwrap().readers += 1;
        MockReader(self.clone())
    }

    /// Waits until the readers have taken everything that was pushed and
    /// have passed it on, so that the next poll sees all of it.
    pub fn settle(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.0.lock().unwrap();
        while !(state.inputs.is_empty() && state.blocked == state.readers) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "the readers did not settle");
            state = self.1.wait_timeout(state, left).unwrap().0;
        }
    }

    pub fn module(&self, kind: ModuleKind) -> Module {
        Module::new(kind, None, Box::new(self.clone()))
    }

    fn check(&self) -> Result<()> {
        if self.0.lock().unwrap().unplugged {
            return Err(Error::Disconnected("mock unplugged".to_owned()));
        }
        Ok(())
    }
}

impl Transport for Mock {
    fn read_input(&self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        self.check()?;
        let mut state = self.0.lock().unwrap();
        state.read_timeouts.push(timeout);
        match state.inputs.pop_front() {
            Some(report) => {
                buf[..report.len()].copy_from_slice(&report);
                Ok(INPUT_REPORT_LEN)
            }
            None => Ok(0),
        }
    }

    fn write_output(&self, report: &[u8]) -> Result<()> {
        self.check()?;
        let mut state = self.0.lock().unwrap();
        let outputs = state
            .sent
            .iter()
            .filter(|s| matches!(s, Sent::Output(_)))
            .count();
        if state.fail_output_after == Some(outputs) {
            return Err(Error::Disconnected("mock".to_owned()));
        }
        state.sent.push(Sent::Output(report.to_vec()));
        Ok(())
    }

    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        self.check()?;
        let mut state = self.0.lock().unwrap();
        state.sent.push(Sent::FeatureRead(buf[0]));
        state.held = None;
        if matches!(buf[0], 0x38 | 0x3c | 0x48 | 0x4c) {
            // Seen ending with an answered firmware read; whether the
            // device info read would do is not known.
            state.lagging = state.standalone;
            if state.standalone {
                return Ok(0);
            }
        }
        let report = state
            .features
            .iter()
            .find(|f| f[0] == buf[0])
            .expect("unexpected feature read")
            .clone();
        buf[..report.len()].copy_from_slice(&report);
        Ok(report.len())
    }

    fn send_feature(&self, report: &[u8]) -> Result<()> {
        self.check()?;
        let mut state = self.0.lock().unwrap();
        state.sent.push(Sent::Feature(report.to_vec()));
        let due = if state.lagging {
            state.held.replace(report.to_vec())
        } else {
            Some(report.to_vec())
        };
        if let Some(command) = due {
            state.carry_out(&command);
        }
        Ok(())
    }

    fn report_descriptor(&self) -> Result<Vec<u8>> {
        self.check()?;
        Ok(Vec::new())
    }
}

pub(crate) struct MockReader(Mock);

impl Drop for MockReader {
    fn drop(&mut self) {
        self.0.0.lock().unwrap().readers -= 1;
        self.0.1.notify_all();
    }
}

impl InputReader for MockReader {
    fn read_input(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        let Mock(state, wake) = &self.0;
        let mut state = state.lock().unwrap();
        state.read_timeouts.push(timeout);
        let deadline = Instant::now().checked_add(timeout);
        loop {
            if state.unplugged {
                return Err(Error::Disconnected("mock unplugged".to_owned()));
            }
            if state.glitches > 0 {
                state.glitches -= 1;
                return Err(Error::Disconnected("mock interrupted".to_owned()));
            }
            if let Some(report) = state.inputs.pop_front() {
                buf[..report.len()].copy_from_slice(&report);
                return Ok(INPUT_REPORT_LEN);
            }
            let left = deadline.map_or(Duration::from_secs(3600), |d| {
                d.saturating_duration_since(Instant::now())
            });
            if left.is_zero() {
                return Ok(0);
            }
            state.blocked += 1;
            wake.notify_all();
            state = wake.wait_timeout(state, left).unwrap().0;
            state.blocked -= 1;
        }
    }
}
