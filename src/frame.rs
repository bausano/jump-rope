use crate::prelude::*;
use ffmpeg::format::{context::Input, input, Pixel};
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame;
use ffmpeg::{decoder, Rational};
use image::{GrayImage, ImageBuffer};
use std::path::Path;

pub struct FrameIter {
    ictx: Input,
    decoder: decoder::Video,
    scaler: Context,
    video_stream_index: usize,
    // This is set to true when input emits eof, so we won't attempt to fetch
    // any more packets on next iteration.
    eof: bool,
    // To avoid reallocation, we keep a buffer where ffmpeg loads input
    // (original) video frames.
    input_frame_buffer: frame::video::Video,
    // To avoid reallocation, we keep a buffer where ffmpeg stores normalized
    // frames (rescaled, grayscaled).
    converted_frame_buffer: frame::video::Video,
}

impl FrameIter {
    pub fn from_file(video_path: impl AsRef<Path>) -> Result<Self> {
        let ictx = input(&video_path)?;
        let input = ictx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)?;

        let video_stream_index = input.index();

        let decoder = input.codec().decoder().video()?;

        let scaler = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::GRAY8,
            decoder.width(),
            decoder.height(),
            Flags::BILINEAR,
        )?;

        Ok(Self {
            ictx,
            decoder,
            scaler,
            video_stream_index,
            eof: false,
            input_frame_buffer: frame::video::Video::empty(),
            converted_frame_buffer: frame::video::Video::empty(),
        })
    }

    pub fn width(&self) -> u32 {
        self.decoder.width()
    }

    pub fn height(&self) -> u32 {
        self.decoder.height()
    }

    pub fn frame_rate(&self) -> usize {
        let Rational(numerator, denominator) =
            self.decoder.frame_rate().expect("Cannot get frame rate");

        (numerator / denominator) as usize
    }
}

impl Iterator for FrameIter {
    type Item = GrayImage;

    fn next(&mut self) -> Option<Self::Item> {
        if self
            .decoder
            .receive_frame(&mut self.input_frame_buffer)
            .is_ok()
        {
            self.read_input_frame()
        } else if self.eof {
            None
        } else {
            let mut packets = self.ictx.packets();

            if let Some((stream, packet)) = packets.next() {
                if stream.index() == self.video_stream_index {
                    self.decoder.send_packet(&packet).ok()?;
                }

                self.next()
            } else {
                self.decoder.send_eof().ok()?;
                self.eof = true;
                self.next()
            }
        }
    }
}

impl FrameIter {
    fn read_input_frame(&mut self) -> Option<GrayImage> {
        // this function must be called after decoder loads data into this
        // buffer with "receive_frame"
        let frame = &mut self.converted_frame_buffer;

        // first copy
        self.scaler.run(&self.input_frame_buffer, frame).ok()?;
        debug_assert_eq!(self.decoder.width(), frame.width());
        debug_assert_eq!(self.decoder.height(), frame.height());
        let frame_bytes_len = (frame.width() * frame.height()) as usize;

        // second copy
        debug_assert_eq!(frame_bytes_len, frame.data(0).len());
        let frame_bytes = frame.data(0).to_vec();
        ImageBuffer::from_raw(frame.width(), frame.height(), frame_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_loads_video() {
        let file = "test/assets/sample_1.mp4";
        let frames = FrameIter::from_file(file).expect("Cannot load video");

        for frame in frames.take(100) {
            assert_eq!(frame.width(), 1280);
            assert_eq!(frame.height(), 720);
        }
    }
}
