extern crate ffmpeg_next as ffmpeg;

mod frame;
mod frequency;
mod oscillator;
mod prelude;

use crate::frame::FrameIter;
use crate::frequency::AnalyzerBuilder;
use std::sync::Arc;

fn main() {
    ffmpeg::init().unwrap();

    let file = "test/assets/sample_2.mp4";
    let frames = FrameIter::from_file(file).expect("Cannot load video");
    let frame_rate = frames.frame_rate();
    println!("FPS: {}", frame_rate);

    const WINDOW_MULTIPLIERS: &[usize] = &[4, 8, 12];
    let channels: Vec<_> = WINDOW_MULTIPLIERS
        .iter()
        .map(|multiplier| {
            frequency::analyzer_channel(AnalyzerBuilder {
                frame_rate,
                window: frame_rate * *multiplier,
                frame_height: frames.height(),
                frame_width: frames.width(),
            })
        })
        .collect();

    for frame in frames {
        let frame = Arc::new(frame);
        channels.iter().for_each(|(frame_sender, _)| {
            frame_sender.send(Arc::clone(&frame)).expect("Channel dead")
        });

        for (_, frequency_recv) in &channels {
            if let Some(report) = frequency_recv.try_iter().last() {
                println!("{:?}", report);
            }
        }
    }
}
