//! Test doubles shared by the unit tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::module::Module;
use crate::protocol::frame::INPUT_REPORT_LEN;
use crate::transport::Transport;

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
    /// Makes every operation fail the way a pulled module does.
    pub unplugged: bool,
}

/// A scripted transport. Clones share their state, so a test can keep one
/// handle while the module owns another.
#[derive(Clone, Default)]
pub(crate) struct Mock(pub Arc<Mutex<State>>);

impl Mock {
    pub fn with_features(features: &[&[u8]]) -> Self {
        let mock = Mock::default();
        mock.0.lock().unwrap().features = features.iter().map(|f| f.to_vec()).collect();
        mock
    }

    pub fn push_input(&self, report: &[u8]) {
        self.0.lock().unwrap().inputs.push_back(report.to_vec());
    }

    pub fn sent(&self) -> Vec<Sent> {
        self.0.lock().unwrap().sent.clone()
    }

    pub fn unplug(&self) {
        self.0.lock().unwrap().unplugged = true;
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
    fn read_input(&self, buf: &mut [u8], _timeout: Duration) -> Result<usize> {
        self.check()?;
        match self.0.lock().unwrap().inputs.pop_front() {
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
        self.0
            .lock()
            .unwrap()
            .sent
            .push(Sent::Feature(report.to_vec()));
        Ok(())
    }

    fn report_descriptor(&self) -> Result<Vec<u8>> {
        self.check()?;
        Ok(Vec::new())
    }
}
