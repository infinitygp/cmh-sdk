//! Framing shared by every input report.
//!
//! ```text
//! offset 0      report ID, always 0x01
//! offset 1      channel tag (module family specific)
//! offset 2..4   payload length, u16 little endian
//! offset 4..    payload, zero padded up to the 64 byte report size
//! ```

use crate::error::{Error, Result};

/// Report ID of the single input report declared by every module.
pub const INPUT_REPORT_ID: u8 = 0x01;
/// Input report size including the report ID.
pub const INPUT_REPORT_LEN: usize = 64;

const HEADER_LEN: usize = 4;

/// A de-framed input report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputFrame<'a> {
    pub tag: u8,
    pub payload: &'a [u8],
}

impl<'a> InputFrame<'a> {
    /// Parses a raw input report, including its leading report ID.
    pub fn parse(report: &'a [u8]) -> Result<Self> {
        if report.len() < HEADER_LEN {
            return Err(Error::MalformedReport(format!(
                "input report too short: {} bytes",
                report.len()
            )));
        }
        if report[0] != INPUT_REPORT_ID {
            return Err(Error::MalformedReport(format!(
                "unexpected input report ID {:#04x}",
                report[0]
            )));
        }
        let len = usize::from(u16::from_le_bytes([report[2], report[3]]));
        let payload = report.get(HEADER_LEN..HEADER_LEN + len).ok_or_else(|| {
            Error::MalformedReport(format!(
                "payload length {len} exceeds report of {} bytes",
                report.len()
            ))
        })?;
        Ok(Self {
            tag: report[1],
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Base heartbeat observed on M0064 while idle (docs/experiments/0001).
    const BASE_HEARTBEAT: [u8; 8] = [0x01, 0x2b, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00];

    #[test]
    fn parses_base_heartbeat() {
        let mut report = [0u8; INPUT_REPORT_LEN];
        report[..BASE_HEARTBEAT.len()].copy_from_slice(&BASE_HEARTBEAT);
        let frame = InputFrame::parse(&report).unwrap();
        assert_eq!(frame.tag, 0x2b);
        assert_eq!(frame.payload, [0x01]);
    }

    #[test]
    fn rejects_wrong_report_id() {
        assert!(InputFrame::parse(&[0x02, 0x2b, 0x01, 0x00, 0x01]).is_err());
    }

    #[test]
    fn rejects_truncated_payload() {
        assert!(InputFrame::parse(&[0x01, 0x2b, 0x05, 0x00, 0x01]).is_err());
        assert!(InputFrame::parse(&[0x01, 0x2b]).is_err());
    }
}
