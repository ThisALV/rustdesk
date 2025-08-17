use std::time::Instant;

use hbb_common::{
    anyhow::Context,
    bail,
    bytes::Bytes,
    log,
    message_proto::{
        EncodedVideoFrame, EncodedVideoFrames, VideoFrame,
    },
    ResultType,
};

use crate::{codec::{EncoderApi, EncoderCfg}, EncodeInput, EncodeYuvFormat, Pixfmt};

use image::{ExtendedColorType, ImageEncoder, codecs::jpeg::JpegEncoder};

#[derive(Clone, Debug)]
pub struct MjpegEncoderConfig {
    pub width: usize,
    pub height: usize,
    pub quality: f32,  // 0.0-1.0
    pub keyframe_interval: Option<usize>,
}

pub struct MjpegEncoder {
    config: MjpegEncoderConfig,
    yuv_fmt: EncodeYuvFormat,
    jpeg_quality: u8,  // 0-100
    frame_count: u32,
    last_keyframe: u32,
    current_bitrate: u32,
}

impl MjpegEncoder {
    #[inline]
    fn create_frame(data: Vec<u8>, key: bool, pts: i64) -> EncodedVideoFrame {
        EncodedVideoFrame {
            data: Bytes::from(data),
            key,
            pts,
            ..Default::default()
        }
    }

    #[inline]
    pub fn create_video_frame(frames: Vec<EncodedVideoFrame>) -> VideoFrame {
        let mut vf = VideoFrame::new();
        let mjpegs = EncodedVideoFrames {
            frames,
            ..Default::default()
        };
        vf.set_mjpegs(mjpegs);
        vf
    }

    fn encode_rgb_to_jpeg(&mut self, data: &[u8], width: usize, height: usize) -> ResultType<Vec<u8>> {
        let mut jpeg_data = Vec::new();

        // Create a JPEG encoder with the configured quality
        let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, self.jpeg_quality);

        // Encode the RGB data to JPEG
        encoder.encode(data, width as u32, height as u32, ExtendedColorType::Rgb8)
            .context("Failed to encode JPEG")?;

        Ok(jpeg_data)
    }

    fn encode_yuv_to_jpeg(&mut self, yuv: &[u8], width: usize, height: usize) -> ResultType<Vec<u8>> {
        // Convert YUV to RGB
        let mut rgb = vec![0u8; width * height * 3];

        match self.yuv_fmt.pixfmt {
            Pixfmt::I420 => {
                self.yuv420_to_rgb(yuv, &mut rgb, width, height)?;
            },
            Pixfmt::I444 => {
                self.yuv444_to_rgb(yuv, &mut rgb, width, height)?;
            },
            _ => {
                // Fallback: convert to grayscale if format is not supported
                self.yuv_to_grayscale(yuv, &mut rgb, width, height)?;
            }
        }

        // Encode RGB to JPEG
        self.encode_rgb_to_jpeg(&rgb, width, height)
    }

    /// Convert YUV to grayscale RGB (fallback for unsupported formats)
    fn yuv_to_grayscale(&self, yuv: &[u8], rgb: &mut [u8], width: usize, height: usize) -> ResultType<()> {
        for y in 0..height {
            for x in 0..width {
                let y_index = y * self.yuv_fmt.stride[0] + x;
                if y_index < yuv.len() {
                    let y_value = yuv[y_index];
                    let rgb_index = (y * width + x) * 3;
                    if rgb_index + 2 < rgb.len() {
                        rgb[rgb_index] = y_value;     // R
                        rgb[rgb_index + 1] = y_value; // G
                        rgb[rgb_index + 2] = y_value; // B
                    }
                }
            }
        }
        Ok(())
    }

    /// Convert YUV420 to RGB using standard conversion formulas
    fn yuv420_to_rgb(&self, yuv: &[u8], rgb: &mut [u8], width: usize, height: usize) -> ResultType<()> {
        let y_plane = &yuv[0..self.yuv_fmt.u];
        let u_plane = &yuv[self.yuv_fmt.u..self.yuv_fmt.v];
        let v_plane = &yuv[self.yuv_fmt.v..];

        let y_stride = self.yuv_fmt.stride[0];
        let uv_stride = self.yuv_fmt.stride[1];

        for y in 0..height {
            for x in 0..width {
                // Y component (full resolution)
                let y_idx = y * y_stride + x;
                if y_idx >= y_plane.len() {
                    continue;
                }
                let y_val = y_plane[y_idx] as i32;

                // U and V components (half resolution in both dimensions for 420)
                let uv_y = y / 2;
                let uv_x = x / 2;
                let uv_idx = uv_y * uv_stride + uv_x;

                let u_val = if uv_idx < u_plane.len() {
                    u_plane[uv_idx] as i32 - 128
                } else { 0 };

                let v_val = if uv_idx < v_plane.len() {
                    v_plane[uv_idx] as i32 - 128
                } else { 0 };

                // YUV to RGB conversion using standard formulas
                // R = Y + 1.402 * V
                // G = Y - 0.344 * U - 0.714 * V
                // B = Y + 1.772 * U
                let r = (y_val + (1402 * v_val) / 1000).clamp(0, 255) as u8;
                let g = (y_val - (344 * u_val + 714 * v_val) / 1000).clamp(0, 255) as u8;
                let b = (y_val + (1772 * u_val) / 1000).clamp(0, 255) as u8;

                let rgb_idx = (y * width + x) * 3;
                if rgb_idx + 2 < rgb.len() {
                    rgb[rgb_idx] = r;
                    rgb[rgb_idx + 1] = g;
                    rgb[rgb_idx + 2] = b;
                }
            }
        }
        Ok(())
    }

    /// Convert YUV444 to RGB using standard conversion formulas
    fn yuv444_to_rgb(&self, yuv: &[u8], rgb: &mut [u8], width: usize, height: usize) -> ResultType<()> {
        let y_plane = &yuv[0..self.yuv_fmt.u];
        let u_plane = &yuv[self.yuv_fmt.u..self.yuv_fmt.v];
        let v_plane = &yuv[self.yuv_fmt.v..];

        let stride = self.yuv_fmt.stride[0];

        for y in 0..height {
            for x in 0..width {
                let idx = y * stride + x;

                if idx >= y_plane.len() || idx >= u_plane.len() || idx >= v_plane.len() {
                    continue;
                }

                let y_val = y_plane[idx] as i32;
                let u_val = u_plane[idx] as i32 - 128;
                let v_val = v_plane[idx] as i32 - 128;

                // YUV to RGB conversion using standard formulas
                let r = (y_val + (1402 * v_val) / 1000).clamp(0, 255) as u8;
                let g = (y_val - (344 * u_val + 714 * v_val) / 1000).clamp(0, 255) as u8;
                let b = (y_val + (1772 * u_val) / 1000).clamp(0, 255) as u8;

                let rgb_idx = (y * width + x) * 3;
                if rgb_idx + 2 < rgb.len() {
                    rgb[rgb_idx] = r;
                    rgb[rgb_idx + 1] = g;
                    rgb[rgb_idx + 2] = b;
                }
            }
        }
        Ok(())
    }
}

impl EncoderApi for MjpegEncoder {
    fn new(cfg: EncoderCfg, i444: bool) -> ResultType<Self> where Self: Sized {
        match cfg {
            EncoderCfg::MJPEG(config) => {
                // Calculate the base bitrate for the resolution
                let base_bitrate = crate::codec::base_bitrate(config.width as u32, config.height as u32);

                // Calculate JPEG quality (0-100) from the quality ratio (0.0-1.0)
                // Higher quality ratio means better quality (lower compression)
                let jpeg_quality = (config.quality * 100.0).min(100.0).max(1.0) as u8;

                // Setup YUV format
                let stride_y = if i444 {
                    config.width
                } else {
                    (config.width + 1) & !1
                };

                let stride_uv = if i444 {
                    stride_y
                } else {
                    (stride_y / 2 + 1) & !1
                };

                let y_size = stride_y * config.height;
                let u_offset = y_size;
                let v_offset = if i444 {
                    y_size + stride_uv * config.height
                } else {
                    y_size + stride_uv * (config.height / 2)
                };

                let yuv_fmt = EncodeYuvFormat {
                    w: config.width as _,
                    h: config.height as _,
                    stride: [stride_y as _, stride_uv as _, stride_uv as _, 0].to_vec(),
                    u: u_offset,
                    v: v_offset,
                    pixfmt: if i444 { Pixfmt::I444 } else { Pixfmt::I420 },
                };

                Ok(Self {
                    config,
                    yuv_fmt,
                    jpeg_quality,
                    frame_count: 0,
                    last_keyframe: 0,
                    current_bitrate: base_bitrate,
                })
            },
            _ => bail!("Invalid encoder config for MJPEG encoder"),
        }
    }

    fn encode_to_message(&mut self, frame: EncodeInput, ms: i64) -> ResultType<VideoFrame> {
        let start = Instant::now();

        // Increment frame counter
        self.frame_count += 1;

        // Determine if this should be a keyframe
        // For MJPEG, every frame is effectively independent (like a keyframe)
        // but we still respect the keyframe_interval setting if provided
        let is_keyframe = if let Some(interval) = self.config.keyframe_interval {
            interval > 0 && (self.frame_count - self.last_keyframe) >= interval as u32
        } else {
            true
        };

        if is_keyframe {
            self.last_keyframe = self.frame_count;
        }

        // Process the frame based on input type
        let jpeg_data = match frame {
            EncodeInput::YUV(yuv) => {
                self.encode_yuv_to_jpeg(yuv, self.config.width, self.config.height)?
            },
            EncodeInput::Texture(_) => {
                bail!("Texture input not supported by MJPEG encoder");
            }
        };

        // Track the actual bitrate
        let elapsed = start.elapsed();
        if elapsed.as_secs() > 0 {
            let bps = (jpeg_data.len() * 8) as u32 / elapsed.as_secs() as u32;
            // Use an exponential moving average to smooth bitrate reporting
            self.current_bitrate = (self.current_bitrate * 9 + bps) / 10;
        }

        // Create frame and video frame
        let frame = Self::create_frame(jpeg_data, is_keyframe, ms);
        let video_frame = Self::create_video_frame(vec![frame]);

        Ok(video_frame)
    }

    fn yuvfmt(&self) -> EncodeYuvFormat {
        self.yuv_fmt.clone()
    }

    #[cfg(feature = "vram")]
    fn input_texture(&self) -> bool {
        false  // MJPEG doesn't support texture input
    }

    fn set_quality(&mut self, ratio: f32) -> ResultType<()> {
        // Update JPEG quality
        self.jpeg_quality = (ratio * 100.0).min(100.0).max(1.0) as u8;
        Ok(())
    }

    fn bitrate(&self) -> u32 {
        self.current_bitrate
    }

    fn support_changing_quality(&self) -> bool {
        true  // MJPEG supports quality changes
    }

    fn latency_free(&self) -> bool {
        true  // MJPEG has no frame dependencies
    }

    fn is_hardware(&self) -> bool {
        false  // This is a software encoder
    }

    fn disable(&self) {
        // No specific resources to free for MJPEG
    }
}