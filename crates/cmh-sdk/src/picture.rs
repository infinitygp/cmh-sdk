//! Turning ordinary images into what the display modules accept.
//!
//! The modules take one JPEG flavour only (see `docs/protocol/keys15.md`):
//! baseline, three components without chroma subsampling, exactly the size
//! of the display, and small. This module produces that from pixels or from
//! an encoded PNG / JPEG file. Requires the `image` cargo feature.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ExtendedColorType, RgbImage, RgbaImage};

use crate::devices::DisplaySpec;
use crate::error::{Error, Result};
use crate::protocol::image::MAX_IMAGE_LEN;

/// Quality steps tried in order until the result fits [`MAX_IMAGE_LEN`].
const QUALITIES: [u8; 6] = [90, 80, 70, 60, 45, 30];

/// Decodes a PNG or JPEG file and re-encodes it for `display`.
///
/// The picture is stretched to the display size; transparent areas become
/// black.
pub fn convert(display: DisplaySpec, file: &[u8]) -> Result<Vec<u8>> {
    let decoded = image::load_from_memory(file)
        .map_err(|err| Error::InvalidArgument(format!("cannot decode image: {err}")))?;
    encode(display, decoded)
}

/// Encodes `width` x `height` RGBA pixels (4 bytes each, row by row) for
/// `display`.
pub fn encode_rgba(display: DisplaySpec, width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let pixels = RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or_else(|| {
        Error::InvalidArgument(format!(
            "{} bytes do not make a {width}x{height} RGBA image",
            rgba.len()
        ))
    })?;
    encode(display, DynamicImage::ImageRgba8(pixels))
}

fn encode(display: DisplaySpec, picture: DynamicImage) -> Result<Vec<u8>> {
    let (width, height) = (u32::from(display.width), u32::from(display.height));
    let picture = if (picture.width(), picture.height()) == (width, height) {
        picture
    } else {
        picture.resize_exact(width, height, FilterType::Lanczos3)
    };
    let rgb = flatten(&picture.to_rgba8());

    for quality in QUALITIES {
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
            .map_err(|err| Error::InvalidArgument(format!("cannot encode image: {err}")))?;
        if jpeg.len() <= MAX_IMAGE_LEN {
            return Ok(jpeg);
        }
    }
    Err(Error::InvalidArgument(format!(
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
    fn converts_encoded_files_and_flattens_transparency() {
        let spec = display(ModuleKind::Keys15);
        let mut png = Vec::new();
        // Fully transparent red must come out black.
        let transparent = RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 0]));
        DynamicImage::ImageRgba8(transparent)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let jpeg = convert(spec, &png).unwrap();
        let back = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (72, 72));
        assert!(back.pixels().all(|p| p.0.iter().all(|c| *c < 8)));
    }

    #[test]
    fn rejects_bad_input() {
        let spec = display(ModuleKind::Keys15);
        assert!(convert(spec, b"not an image").is_err());
        assert!(encode_rgba(spec, 10, 10, &[0; 7]).is_err());
    }
}
