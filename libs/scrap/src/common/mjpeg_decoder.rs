use hbb_common::{
    anyhow::Context,
    bail,
    ResultType,
};

use image::ImageFormat;

pub struct MjpegDecoder {
    width: usize,
    height: usize,
}

impl MjpegDecoder {
    pub fn new() -> ResultType<Self> {
        Ok(Self {
            width: 0,
            height: 0,
        })
    }

    pub fn decode_jpeg_to_rgb(&mut self, data: &[u8]) -> ResultType<Vec<u8>> {
        // Use the image crate to decode JPEG on all platforms
        let img = image::load_from_memory_with_format(data, ImageFormat::Jpeg)
            .context("Failed to decode JPEG")?;

        let rgb_img = img.to_rgb8();
        self.width = rgb_img.width() as usize;
        self.height = rgb_img.height() as usize;

        Ok(rgb_img.into_raw())
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Get decoder information for integration with codec system
    pub fn info(&self) -> DecoderInfo {
        DecoderInfo {
            pixfmt: crate::Pixfmt::BGRA,
            width: self.width,
            height: self.height,
        }
    }
}

/// Simple decoder info structure for MJPEG
pub struct DecoderInfo {
    pub pixfmt: crate::Pixfmt,
    pub width: usize,
    pub height: usize,
}
