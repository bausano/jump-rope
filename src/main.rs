extern crate ffmpeg_next as ffmpeg;

mod frame;
mod oscillator;
mod prelude;
mod xd;

use frame::FrameIter;
use image::GrayImage;
use rand::{thread_rng, Rng};
use xd::Xd;

fn main() {
    ffmpeg::init().unwrap();

    let file = "test/assets/sample_1.mp4";
    let frames = FrameIter::from_file(file).expect("Cannot load video");
    let frame_rate = frames.frame_rate();

    let mut rng = thread_rng();
    let mut xd = Xd::new(frame_rate, frame_rate.next_power_of_two());
    let data_size = frames.width() * frames.height();
    for _ in 0..(data_size / 5) {
        let pixel_index = rng.gen_range(0..data_size);
        xd.track_pixel(pixel_index);
    }
    let xd_pixels: Vec<_> = xd.tracked_pixels().map(|k| *k).collect();

    for (frame_index, frame) in frames.enumerate() {
        process_frame(&frame, frame_index, &mut xd, &xd_pixels);
    }
}

fn process_frame(
    frame: &GrayImage,
    index: usize,
    xd: &mut Xd,
    xd_pixels: &[usize],
) {
    let pixels = frame.as_raw();
    for p in xd_pixels {
        xd.update_pixel(*p, pixels[*p]);
    }

    if index % 10 == 0 {
        println!("{} --------------------------", index);
        xd.frequency();
    }
}
