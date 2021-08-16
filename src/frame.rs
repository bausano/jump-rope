use crate::prelude::*;
use ffmpeg::format::{context::Input, input, Pixel};
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame;
use ffmpeg::{decoder, Rational};
use image::buffer::PixelsMut;
use image::{GrayImage, ImageBuffer, Luma};
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
    // Remember last frame so that we can substract it from the new frame,
    // which allows us to focus on the changes between frames.
    previous_frame: Option<GrayImage>,
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
            previous_frame: None,
        })
    }

    pub fn width(&self) -> usize {
        self.decoder.width() as usize
    }

    pub fn height(&self) -> usize {
        self.decoder.height() as usize
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
            self.difference_between_input_frame_and_previous_frame()
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
    fn difference_between_input_frame_and_previous_frame(
        &mut self,
    ) -> Option<GrayImage> {
        // there's a lot of performance left on the table due to several copies
        // which I was forced into due to the lib APIs, but which I think can
        // be eliminated with some more thought

        let frame = self.read_input_frame()?;

        if let Some(mut buffer) = self.previous_frame.take() {
            // copy only changed pixels from "frame" to "previous_edges" buffer
            copy_only_different_pixels(buffer.pixels_mut(), frame.as_raw());
            self.previous_frame = Some(frame);
            Some(buffer)
        } else {
            // this branch is only taken on the first frame
            self.previous_frame = Some(frame);
            // on the next call, the above branch is taken and we will therefore
            // be consistent with what we return: difference between two frames
            self.difference_between_input_frame_and_previous_frame()
        }
    }

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

fn copy_only_different_pixels(
    previous_frame: PixelsMut<'_, Luma<u8>>,
    frame: &[u8],
) {
    for (dest, src) in previous_frame.zip(frame) {
        let updated_pixel_colour = if dest.0[0] == *src {
            // if there isn't any difference to the previous frame in this
            // pixel, then set to white colour
            0
        } else {
            // otherwise set to level the colour of current frame
            *src
        };

        dest.0[0] = updated_pixel_colour;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_finds_edges() {
        let file = "test/assets/sample_1.mp4";
        let frames = FrameIter::from_file(file).expect("Cannot load video");

        for (index, frame) in frames.enumerate() {
            frame.save(format!("pls/que-{}.png", index)).expect("xdxd");
        }
    }
}
