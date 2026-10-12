//! Turning ordinary images into what the display modules accept.
//!
//! The modules take one JPEG flavour only (see `docs/protocol/keys15.md`):
//! baseline, three components without chroma subsampling, exactly the size
//! of the display, and small. This module produces that from pixels or from
//! an encoded PNG / JPEG file. Requires the `image` cargo feature.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{ExtendedColorType, ImageReader, Limits, RgbImage, RgbaImage};

use crate::devices::DisplaySpec;
use crate::error::{Error, Result};
use crate::protocol::image::MAX_IMAGE_LEN;

/// Quality steps tried in order until the result fits [`MAX_IMAGE_LEN`].
const QUALITIES: [u8; 6] = [90, 80, 70, 60, 45, 30];
/// Largest accepted side of a source picture. Sources are scaled down to a
/// display of at most 240 pixels, so anything bigger is wasted work.
const MAX_SOURCE_SIDE: u32 = 4096;
/// Memory a source picture may take while being decoded.
const MAX_SOURCE_ALLOC: u64 = 64 * 1024 * 1024;

fn invalid(message: String) -> Error {
    Error::InvalidArgument(message)
}

/// Decodes a PNG or JPEG file and re-encodes it for `display`.
///
/// The picture is stretched to the display size; transparent areas become
/// black. Sources larger than 4096 pixels on a side are refused.
pub fn convert(display: DisplaySpec, file: &[u8]) -> Result<Vec<u8>> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_SIDE);
    limits.max_image_height = Some(MAX_SOURCE_SIDE);
    limits.max_alloc = Some(MAX_SOURCE_ALLOC);

    let mut reader = ImageReader::new(Cursor::new(file))
        .with_guessed_format()
        .map_err(|err| invalid(format!("cannot read image: {err}")))?;
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|err| invalid(format!("cannot decode image: {err}")))?;
    encode(display, decoded.to_rgba8())
}

/// Encodes `width` x `height` RGBA pixels (4 bytes each, row by row) for
/// `display`.
pub fn encode_rgba(display: DisplaySpec, width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    // Checked in this order so that the size computation cannot overflow.
    let sides_ok =
        (1..=MAX_SOURCE_SIDE).contains(&width) && (1..=MAX_SOURCE_SIDE).contains(&height);
    if !sides_ok || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4 {
        return Err(invalid(format!(
            "{} bytes do not make a {width}x{height} RGBA image of at most {MAX_SOURCE_SIDE} pixels a side",
            rgba.len()
        )));
    }
    let pixels = RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| invalid("pixel buffer does not match its dimensions".to_owned()))?;
    encode(display, pixels)
}

fn encode(display: DisplaySpec, picture: RgbaImage) -> Result<Vec<u8>> {
    let (width, height) = (u32::from(display.width), u32::from(display.height));
    // Transparency is resolved before scaling: scaling first would let the
    // colour stored under fully transparent pixels bleed into the edges.
    let flat = flatten(&picture);
    let rgb = if (flat.width(), flat.height()) == (width, height) {
        flat
    } else {
        imageops::resize(&flat, width, height, FilterType::Lanczos3)
    };

    for quality in QUALITIES {
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
            .map_err(|err| invalid(format!("cannot encode image: {err}")))?;
        if jpeg.len() <= MAX_IMAGE_LEN {
            return Ok(jpeg);
        }
    }
    Err(invalid(format!(
        "image does not fit {MAX_IMAGE_LEN} bytes even at the lowest quality"
    )))
}

/// Blends the pixels over black, which is what a dark display shows where
/// nothing is drawn.
fn flatten(rgba: &RgbaImage) -> RgbImage {
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let blend = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        image::Rgb([blend(r), blend(g), blend(b)])
    })
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageFormat};

    use super::*;
    use crate::devices::ModuleKind;
    use crate::protocol::image::{image_reports, jpeg_info};

    fn display(kind: ModuleKind) -> DisplaySpec {
        kind.capabilities().display.unwrap()
    }

    fn gradient(width: u32, height: u32) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| {
                let (x, y) = (i % width, i / width);
                [(x * 255 / width) as u8, (y * 255 / height) as u8, 128, 255]
            })
            .collect()
    }

    fn png(picture: RgbaImage) -> Vec<u8> {
        let mut file = Vec::new();
        DynamicImage::ImageRgba8(picture)
            .write_to(&mut Cursor::new(&mut file), ImageFormat::Png)
            .unwrap();
        file
    }

    fn brightest(jpeg: &[u8]) -> u8 {
        let back = image::load_from_memory(jpeg).unwrap().to_rgb8();
        back.pixels().flat_map(|p| p.0).max().unwrap()
    }

    #[test]
    fn output_is_what_the_modules_accept() {
        for kind in [ModuleKind::Keys15, ModuleKind::DialDisplay] {
            let spec = display(kind);
            let jpeg = encode_rgba(spec, 300, 200, &gradient(300, 200)).unwrap();
            let info = jpeg_info(&jpeg).unwrap();
            assert_eq!((info.width, info.height), (spec.width, spec.height));
            assert!(info.baseline && info.components == 3 && !info.subsampled);
            // The upload path applies the same checks before sending.
            assert!(image_reports(kind, 0, &jpeg).unwrap().count() > 0);
        }
    }

    #[test]
    fn busy_pictures_are_compressed_until_they_fit() {
        // Noise is the worst case for JPEG.
        let mut state = 0x1234_5678u32;
        let noise: Vec<u8> = (0..240 * 240 * 4)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        let jpeg = encode_rgba(display(ModuleKind::DialDisplay), 240, 240, &noise);
        match jpeg {
            Ok(jpeg) => assert!(jpeg.len() <= MAX_IMAGE_LEN),
            Err(Error::InvalidArgument(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn transparent_pixels_become_black_whatever_colour_they_hide() {
        let spec = display(ModuleKind::Keys15);
        // Same size as the display: no scaling involved.
        let hidden_red = RgbaImage::from_pixel(72, 72, image::Rgba([255, 0, 0, 0]));
        assert!(brightest(&convert(spec, &png(hidden_red)).unwrap()) < 8);

        // Scaled: a black disc on transparent white must not get a halo.
        let disc = RgbaImage::from_fn(288, 288, |x, y| {
            let (dx, dy) = (x as i32 - 144, y as i32 - 144);
            if dx * dx + dy * dy < 100 * 100 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 0])
            }
        });
        assert!(brightest(&convert(spec, &png(disc)).unwrap()) < 8);
    }

    #[test]
    fn rejects_bad_input() {
        let spec = display(ModuleKind::Keys15);
        assert!(convert(spec, b"not an image").is_err());
        // Pixel buffers must match their dimensions exactly.
        assert!(encode_rgba(spec, 10, 10, &[0; 7]).is_err());
        assert!(encode_rgba(spec, 10, 10, &[0; 4000]).is_err());
        assert!(encode_rgba(spec, 0, 5, &[]).is_err());
        assert!(encode_rgba(spec, u32::MAX, u32::MAX, &[0; 16]).is_err());
        // Oversized sources are refused instead of decoded.
        let huge = png(RgbaImage::new(MAX_SOURCE_SIDE + 1, 1));
        assert!(convert(spec, &huge).is_err());
    }
}
