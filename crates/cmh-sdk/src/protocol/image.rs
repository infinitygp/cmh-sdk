//! Chunked JPEG upload to the display modules.
//!
//! Images travel in 320 byte output reports:
//!
//! ```text
//! offset 0      report ID 0x02
//! offset 1      command: 0x36 (15 LCD Keys), 0x45 (Dial Display)
//! offset 2..4   chunk index, u16 little endian, starting at 0
//! offset 4..6   number of image bytes in this report, u16 little endian
//! offset 6      target: key index 0..14 (0 on the Dial Display)
//! offset 7      1 on the final chunk, otherwise 0
//! offset 8..    up to 312 image bytes
//! ```
//!
//! The final chunk makes the module show the image; there is no reply.

use crate::devices::{DisplaySpec, Family, ModuleKind};
use crate::error::{Error, Result};

/// Size of the output report that carries image data, including the report ID.
pub const IMAGE_REPORT_LEN: usize = 320;
/// Image bytes carried by one report.
pub const IMAGE_CHUNK_LEN: usize = 312;
/// Upper bound accepted by this SDK. Uploads of 19.2 kB to the keys and of
/// 28.1 kB to the dial were displayed (experiment 0019); the real device
/// limit is unknown, so the cap stays below what has been seen working.
pub const MAX_IMAGE_LEN: usize = 18 * 1024;

const OUTPUT_REPORT_ID: u8 = 0x02;
const HEADER_LEN: usize = IMAGE_REPORT_LEN - IMAGE_CHUNK_LEN;

/// One decoded image report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageChunk<'a> {
    pub command: u8,
    pub index: u16,
    pub target: u8,
    pub last: bool,
    pub data: &'a [u8],
}

impl<'a> ImageChunk<'a> {
    pub fn parse(report: &'a [u8]) -> Result<Self> {
        if report.len() != IMAGE_REPORT_LEN || report[0] != OUTPUT_REPORT_ID {
            return Err(Error::MalformedReport(
                "not a 320 byte output report".to_owned(),
            ));
        }
        let len = usize::from(u16::from_le_bytes([report[4], report[5]]));
        if len == 0 || len > IMAGE_CHUNK_LEN || report[7] > 1 {
            return Err(Error::MalformedReport(format!(
                "invalid image chunk header: length {len}, final flag {}",
                report[7]
            )));
        }
        Ok(Self {
            command: report[1],
            index: u16::from_le_bytes([report[2], report[3]]),
            target: report[6],
            last: report[7] == 1,
            data: &report[HEADER_LEN..HEADER_LEN + len],
        })
    }

    /// `data` must not exceed [`IMAGE_CHUNK_LEN`].
    pub(crate) fn encode(&self) -> [u8; IMAGE_REPORT_LEN] {
        let mut report = [0u8; IMAGE_REPORT_LEN];
        report[0] = OUTPUT_REPORT_ID;
        report[1] = self.command;
        report[2..4].copy_from_slice(&self.index.to_le_bytes());
        report[4..6].copy_from_slice(&(self.data.len() as u16).to_le_bytes());
        report[6] = self.target;
        report[7] = u8::from(self.last);
        report[HEADER_LEN..HEADER_LEN + self.data.len()].copy_from_slice(self.data);
        report
    }
}

/// Splits `jpeg` into the output reports that display it on `target`.
///
/// `target` is the key index on the 15 LCD Keys and must be 0 on the Dial
/// Display. The image has to be a baseline JPEG of exactly the display size
/// with three components. The modules ignore progressive and greyscale
/// JPEGs and then draw an image decoded earlier instead, so those are
/// refused here. Of the chroma layouts, 4:4:4, 4:2:2 and 4:2:0 have been
/// seen working on both modules and are accepted.
pub fn image_reports(
    kind: ModuleKind,
    target: u8,
    jpeg: &[u8],
) -> Result<impl Iterator<Item = [u8; IMAGE_REPORT_LEN]> + '_> {
    let unsupported = Error::Unsupported {
        module: kind.name(),
        capability: "images",
    };
    let Some(display) = kind.capabilities().display else {
        return Err(unsupported);
    };
    let command = match kind.family() {
        Family::Keys => 0x36,
        Family::Dial => 0x45,
        Family::Basic => return Err(unsupported),
    };
    if target >= display.images {
        return Err(Error::InvalidArgument(format!(
            "{kind} has {} image target(s), got index {target}",
            display.images
        )));
    }
    validate_jpeg(jpeg, display)?;

    let chunks = jpeg.chunks(IMAGE_CHUNK_LEN);
    let count = chunks.len();
    Ok(chunks.enumerate().map(move |(index, data)| {
        ImageChunk {
            command,
            index: index as u16,
            target,
            last: index + 1 == count,
            data,
        }
        .encode()
    }))
}

/// How the two colour components of a three component JPEG are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChromaSampling {
    /// 4:4:4, at full resolution.
    Full,
    /// 4:2:2, at half the horizontal resolution.
    HalfWidth,
    /// 4:2:0, at half the resolution in both directions.
    Quarter,
}

/// Frame header of a JPEG stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct JpegInfo {
    pub width: u16,
    pub height: u16,
    /// Sequential baseline DCT (SOF0), as opposed to progressive and others.
    pub baseline: bool,
    pub components: u8,
    /// `None` for anything but a three component image in one of the three
    /// common layouts.
    pub sampling: Option<ChromaSampling>,
}

fn chroma_sampling(specs: &[u8]) -> Option<ChromaSampling> {
    // Three bytes per component; the middle one holds the sampling factors.
    match specs {
        [_, 0x11, _, _, 0x11, _, _, 0x11, _] => Some(ChromaSampling::Full),
        [_, 0x21, _, _, 0x11, _, _, 0x11, _] => Some(ChromaSampling::HalfWidth),
        [_, 0x22, _, _, 0x11, _, _, 0x11, _] => Some(ChromaSampling::Quarter),
        _ => None,
    }
}

/// Reads the frame header of a JPEG stream without decoding it.
pub fn jpeg_info(data: &[u8]) -> Option<JpegInfo> {
    if !data.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut pos = 2;
    loop {
        if *data.get(pos)? != 0xff {
            return None;
        }
        // Any number of 0xff fill bytes may precede a marker.
        while *data.get(pos + 1)? == 0xff {
            pos += 1;
        }
        let marker = data[pos + 1];
        let len = usize::from(u16::from_be_bytes([
            *data.get(pos + 2)?,
            *data.get(pos + 3)?,
        ]));
        match marker {
            // SOF0..SOF15 except DHT (c4), JPG (c8) and DAC (cc).
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                let frame = data.get(pos + 4..pos + 10)?;
                let components = frame[5];
                let specs = data.get(pos + 10..pos + 10 + 3 * usize::from(components))?;
                return Some(JpegInfo {
                    height: u16::from_be_bytes([frame[1], frame[2]]),
                    width: u16::from_be_bytes([frame[3], frame[4]]),
                    baseline: marker == 0xc0,
                    components,
                    sampling: chroma_sampling(specs),
                });
            }
            // Start of scan: no frame header came first.
            0xda => return None,
            _ => pos += 2 + len,
        }
    }
}

fn validate_jpeg(jpeg: &[u8], display: DisplaySpec) -> Result<()> {
    let invalid = |message: String| Err(Error::InvalidArgument(message));
    if jpeg.len() > MAX_IMAGE_LEN {
        return invalid(format!(
            "image is {} bytes, limit is {MAX_IMAGE_LEN}",
            jpeg.len()
        ));
    }
    if !jpeg.ends_with(&[0xff, 0xd9]) {
        return invalid("image is not a complete JPEG stream".to_owned());
    }
    let Some(info) = jpeg_info(jpeg) else {
        return invalid("image is not a JPEG stream".to_owned());
    };
    if !info.baseline || info.components != 3 {
        return invalid(
            "the modules display only baseline JPEG with three components, not progressive or greyscale"
                .to_owned(),
        );
    }
    if info.sampling.is_none() {
        return invalid(
            "only 4:4:4, 4:2:2 and 4:2:0 chroma sampling has been verified on hardware".to_owned(),
        );
    }
    if (info.width, info.height) != (display.width, display.height) {
        return invalid(format!(
            "image is {}x{}, display needs {}x{}",
            info.width, info.height, display.width, display.height
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a syntactically plausible JPEG header followed by filler so
    /// that chunking can be tested without a real encoder.
    pub(crate) fn fake_jpeg(width: u16, height: u16, len: usize) -> Vec<u8> {
        let mut data = vec![0xff, 0xd8];
        data.extend([0xff, 0xe0, 0x00, 0x04, 0x4a, 0x46]);
        data.extend([0xff, 0xc0, 0x00, 0x11, 0x08]);
        data.extend(height.to_be_bytes());
        data.extend(width.to_be_bytes());
        data.extend([0x03, 1, 0x11, 0, 2, 0x11, 1, 3, 0x11, 1]);
        while data.len() < len - 2 {
            data.push((data.len() % 251) as u8);
        }
        data.extend([0xff, 0xd9]);
        data
    }

    fn reassemble(reports: &[[u8; IMAGE_REPORT_LEN]]) -> (Vec<u8>, Vec<ImageChunk<'_>>) {
        let chunks: Vec<_> = reports
            .iter()
            .map(|r| ImageChunk::parse(r).unwrap())
            .collect();
        let data = chunks.iter().flat_map(|c| c.data.iter().copied()).collect();
        (data, chunks)
    }

    #[test]
    fn key_image_round_trip() {
        // 9533 bytes is the size of a real key image from experiment 0003;
        // the application sent it as 30 full chunks plus one of 173 bytes.
        let jpeg = fake_jpeg(72, 72, 9533);
        let reports: Vec<_> = image_reports(ModuleKind::Keys15, 4, &jpeg)
            .unwrap()
            .collect();
        assert_eq!(reports.len(), 31);
        let (data, chunks) = reassemble(&reports);
        assert_eq!(data, jpeg);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.command, 0x36);
            assert_eq!(chunk.target, 4);
            assert_eq!(usize::from(chunk.index), i);
            assert_eq!(chunk.last, i == 30);
        }
        assert_eq!(chunks[30].data.len(), 173);
    }

    #[test]
    fn matches_captured_header_bytes() {
        // First and last report header of key 0 in experiment 0003.
        let jpeg = fake_jpeg(72, 72, 9533);
        let reports: Vec<_> = image_reports(ModuleKind::Keys15, 0, &jpeg)
            .unwrap()
            .collect();
        assert_eq!(
            reports[0][..10],
            [0x02, 0x36, 0x00, 0x00, 0x38, 0x01, 0x00, 0x00, 0xff, 0xd8]
        );
        assert_eq!(
            reports[30][..8],
            [0x02, 0x36, 0x1e, 0x00, 0xad, 0x00, 0x00, 0x01]
        );
    }

    #[test]
    fn dial_image_round_trip() {
        let jpeg = fake_jpeg(240, 240, 13383);
        let reports: Vec<_> = image_reports(ModuleKind::DialDisplay, 0, &jpeg)
            .unwrap()
            .collect();
        assert_eq!(reports.len(), 43);
        assert_eq!(reports[0][..2], [0x02, 0x45]);
        assert_eq!(reassemble(&reports).0, jpeg);
    }

    #[test]
    fn exact_multiple_of_chunk_size_ends_on_a_full_chunk() {
        let jpeg = fake_jpeg(72, 72, IMAGE_CHUNK_LEN * 2);
        let reports: Vec<_> = image_reports(ModuleKind::Keys15, 0, &jpeg)
            .unwrap()
            .collect();
        assert_eq!(reports.len(), 2);
        assert!(ImageChunk::parse(&reports[1]).unwrap().last);
    }

    #[test]
    fn rejects_invalid_requests() {
        let key = fake_jpeg(72, 72, 500);
        assert!(image_reports(ModuleKind::Keys15, 15, &key).is_err());
        assert!(image_reports(ModuleKind::DialDisplay, 0, &key).is_err());
        assert!(image_reports(ModuleKind::Knobs3, 0, &key).is_err());
        assert!(image_reports(ModuleKind::Keys15, 0, &key[..400]).is_err());
        assert!(image_reports(ModuleKind::Keys15, 0, b"not a jpeg").is_err());
        // Both were ignored by the keys in experiment 0019.
        let mut progressive = key.clone();
        progressive[9] = 0xc2;
        assert!(image_reports(ModuleKind::Keys15, 0, &progressive).is_err());
        let mut grey = key.clone();
        grey[17] = 1;
        assert!(image_reports(ModuleKind::Keys15, 0, &grey).is_err());
        let huge = fake_jpeg(72, 72, MAX_IMAGE_LEN + 1);
        assert!(image_reports(ModuleKind::Keys15, 0, &huge).is_err());
    }

    #[test]
    fn only_verified_chroma_layouts_are_sent() {
        // Offset 19 holds the sampling factors of the first component.
        let with_luma = |width, factors| {
            let mut jpeg = fake_jpeg(width, width, 500);
            jpeg[19] = factors;
            jpeg
        };
        for (factors, sampling) in [
            (0x21, ChromaSampling::HalfWidth),
            (0x22, ChromaSampling::Quarter),
        ] {
            let key = with_luma(72, factors);
            assert_eq!(jpeg_info(&key).unwrap().sampling, Some(sampling));
            assert!(image_reports(ModuleKind::Keys15, 0, &key).is_ok());
            let dial = with_luma(240, factors);
            assert!(image_reports(ModuleKind::DialDisplay, 0, &dial).is_ok());
        }
        // Layouts nobody tried, such as 4:4:0 or 4:1:1.
        for factors in [0x12, 0x41] {
            let key = with_luma(72, factors);
            assert_eq!(jpeg_info(&key).unwrap().sampling, None);
            assert!(image_reports(ModuleKind::Keys15, 0, &key).is_err());
        }
        // Subsampled the other way round: chroma finer than luma.
        let mut odd = fake_jpeg(72, 72, 500);
        odd[22] = 0x22;
        assert!(image_reports(ModuleKind::Keys15, 0, &odd).is_err());
    }

    #[test]
    fn reads_jpeg_geometry() {
        let info = JpegInfo {
            width: 240,
            height: 120,
            baseline: true,
            components: 3,
            sampling: Some(ChromaSampling::Full),
        };
        assert_eq!(jpeg_info(&fake_jpeg(240, 120, 200)), Some(info));
        // Fill bytes before a marker are legal.
        let mut filled = fake_jpeg(240, 120, 200);
        filled.splice(8..8, [0xff, 0xff]);
        assert_eq!(jpeg_info(&filled), Some(info));
        assert_eq!(jpeg_info(&[0xff, 0xd8, 0xff]), None);
        assert_eq!(jpeg_info(&[]), None);
    }

    #[test]
    fn captured_upload_round_trips() {
        use crate::fixture::{Direction, Fixture};

        let text = include_str!("../../fixtures/images/key-00-sdk-upload.txt");
        let fixture = Fixture::parse(text).unwrap();
        let captured: Vec<Vec<u8>> = fixture
            .records()
            .iter()
            .filter(|r| r.direction == Direction::Out)
            .map(|r| r.report())
            .collect();
        let jpeg: Vec<u8> = captured
            .iter()
            .flat_map(|r| ImageChunk::parse(r).unwrap().data.to_vec())
            .collect();
        let info = jpeg_info(&jpeg).unwrap();
        assert_eq!((info.width, info.height), (72, 72));
        assert!(info.baseline && info.components == 3);
        assert_eq!(info.sampling, Some(ChromaSampling::Full));

        let encoded: Vec<Vec<u8>> = image_reports(ModuleKind::Keys15, 0, &jpeg)
            .unwrap()
            .map(|r| r.to_vec())
            .collect();
        assert_eq!(encoded, captured);
    }

    #[test]
    fn parsers_never_panic_on_arbitrary_bytes() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 400) as usize;
            let mut bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            if len > 4 && next() % 2 == 0 {
                bytes[..2].copy_from_slice(&[0xff, 0xd8]);
                bytes[2] = 0xff;
            }
            let _ = jpeg_info(&bytes);
            let _ = image_reports(ModuleKind::Keys15, 0, &bytes).map(|r| r.count());
            bytes.resize(IMAGE_REPORT_LEN, 0);
            bytes[0] = 0x02;
            let _ = ImageChunk::parse(&bytes);
        }
    }
}
