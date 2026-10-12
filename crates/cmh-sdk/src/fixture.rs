//! Text format for recorded HID reports.
//!
//! One report per line, `#` starts a comment:
//!
//! ```text
//! <seconds> <product id> <direction> <hex bytes>
//!   119.248 0202 in 01 2a 05 00 00 03 00 ff ff
//! ```
//!
//! Directions: `in` (input report), `out` (output report), `get` (feature
//! read response) and `set` (feature write). Trailing zero padding is
//! omitted; [`Record::report`] restores it.

use std::fmt;

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::protocol::feature::FEATURE_REPORT_LEN;
use crate::protocol::frame::INPUT_REPORT_LEN;
use crate::protocol::image::IMAGE_REPORT_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    Get,
    Set,
}

impl Direction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Direction::In => "in",
            Direction::Out => "out",
            Direction::Get => "get",
            Direction::Set => "set",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Record {
    pub seconds: f64,
    pub kind: ModuleKind,
    pub direction: Direction,
    /// Report bytes without trailing zero padding.
    pub bytes: Vec<u8>,
}

impl Record {
    pub fn new(seconds: f64, kind: ModuleKind, direction: Direction, report: &[u8]) -> Self {
        let len = report.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
        Self {
            seconds,
            kind,
            direction,
            bytes: report[..len].to_vec(),
        }
    }

    /// The report padded with zeros to its on-wire size.
    pub fn report(&self) -> Vec<u8> {
        let full = match self.direction {
            Direction::In => INPUT_REPORT_LEN,
            Direction::Get | Direction::Set => FEATURE_REPORT_LEN,
            Direction::Out if self.kind.capabilities().display.is_some() => IMAGE_REPORT_LEN,
            Direction::Out => 64,
        };
        let mut report = self.bytes.clone();
        if report.len() < full {
            report.resize(full, 0);
        }
        report
    }
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:9.3} {:04x} {}",
            self.seconds,
            self.kind.product_id(),
            self.direction.as_str()
        )?;
        for byte in &self.bytes {
            write!(f, " {byte:02x}")?;
        }
        Ok(())
    }
}

/// A parsed fixture file.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    records: Vec<Record>,
}

impl Fixture {
    pub fn parse(text: &str) -> Result<Self> {
        let mut records = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let record = parse_line(line).ok_or_else(|| {
                Error::InvalidArgument(format!(
                    "fixture line {}: cannot parse {line:?}",
                    number + 1
                ))
            })?;
            records.push(record);
        }
        Ok(Self { records })
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }
}

fn parse_line(line: &str) -> Option<Record> {
    let mut fields = line.split_whitespace();
    let seconds: f64 = fields.next()?.parse().ok()?;
    if !seconds.is_finite() {
        return None;
    }
    let product = fields.next()?;
    if product.len() != 4 || !is_hex(product) {
        return None;
    }
    let kind = ModuleKind::from_product_id(u16::from_str_radix(product, 16).ok()?)?;
    let direction = match fields.next()? {
        "in" => Direction::In,
        "out" => Direction::Out,
        "get" => Direction::Get,
        "set" => Direction::Set,
        _ => return None,
    };
    let bytes = fields
        .map(|f| {
            (f.len() == 2 && is_hex(f))
                .then(|| u8::from_str_radix(f, 16).ok())
                .flatten()
        })
        .collect::<Option<Vec<u8>>>()?;
    Some(Record {
        seconds,
        kind,
        direction,
        bytes,
    })
}

fn is_hex(token: &str) -> bool {
    token.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_round_trip() {
        let text = "  119.248 0202 in 01 2a 05 00 00 03 00 ff ff";
        let fixture = Fixture::parse(text).unwrap();
        let record = &fixture.records()[0];
        assert_eq!(record.kind, ModuleKind::Knobs3);
        assert_eq!(record.to_string(), text);
        assert_eq!(record.report().len(), INPUT_REPORT_LEN);
    }

    #[test]
    fn trims_padding_and_skips_comments() {
        let record = Record::new(
            1.0,
            ModuleKind::Base,
            Direction::In,
            &[1, 0x2b, 1, 0, 1, 0, 0],
        );
        assert_eq!(record.bytes, [1, 0x2b, 1, 0, 1]);
        let fixture = Fixture::parse("# header\n\n1.0 01f6 in 01 2b # tail\n").unwrap();
        assert_eq!(fixture.records().len(), 1);
        assert_eq!(fixture.records()[0].bytes, [0x01, 0x2b]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(Fixture::parse("1.0 01f6 sideways 01").is_err());
        assert!(Fixture::parse("1.0 ffff in 01").is_err());
        assert!(Fixture::parse("1.0 01f6 in 1").is_err());
        assert!(Fixture::parse("x 01f6 in 01").is_err());
        assert!(Fixture::parse("nan 01f6 in 01").is_err());
        assert!(Fixture::parse("1.0 +1f6 in 01").is_err());
        assert!(Fixture::parse("1.0 01f6 in +1").is_err());
    }

    #[test]
    fn restores_on_wire_sizes() {
        let size = |line: &str| Fixture::parse(line).unwrap().records()[0].report().len();
        assert_eq!(size("0 0202 in 01"), 64);
        assert_eq!(size("0 0202 out 02"), 64);
        assert_eq!(size("0 01fe out 02"), 320);
        assert_eq!(size("0 0206 out 02"), 320);
        assert_eq!(size("0 01fe get 3b"), 32);
        assert_eq!(size("0 01fe set 03"), 32);
    }
}
