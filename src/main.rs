extern crate ffmpeg_next as ffmpeg;

mod analyzer;
mod frame;
mod frequency_tracker;
mod oscillator;
mod prelude;
mod ui;

use crate::analyzer::AnalyzerBuilder;
use crate::frame::FrameIter;
use frequency_tracker::FrequencyTracker;
use std::sync::Arc;

fn main() {
    ffmpeg::init().unwrap();

    let file = "test/assets/sample_1.mp4";
    let frames = FrameIter::from_file(file).expect("Cannot load video");
    let frame_rate = frames.frame_rate();
    println!("FPS: {}", frame_rate);

    let frequency_tracker = Arc::new(FrequencyTracker::new(frame_rate));

    ui::start_on_another_thread(Arc::clone(&frequency_tracker));

    start_video_analysis(frequency_tracker, frames);
}

// Starts iterating the video frames with various window sizes and updates the
// tracker with latest values.
//
// # Important
// This method blocks until video stops.
fn start_video_analysis(
    frequency_tracker: Arc<FrequencyTracker>,
    frames: FrameIter,
) {
    let frame_rate = frames.frame_rate();

    // The larger the multiplier, the more granular frequency intervals it can
    // find. However, it takes longer to start reporting and it takes longer to
    // adjust to rapid speed changes.
    //
    // We therefore spawn multiple and let them reach a consensus.
    const WINDOW_MULTIPLIERS: &[usize] = &[4, 8, 12];
    let channels: Vec<_> = WINDOW_MULTIPLIERS
        .iter()
        .map(|multiplier| {
            analyzer::channel(AnalyzerBuilder {
                frame_rate,
                window: frame_rate * *multiplier,
                frame_height: frames.height(),
                frame_width: frames.width(),
            })
        })
        .collect();

    for frame in frames {
        // update each analyzer (they differ by window) with the new frame
        let frame = Arc::new(frame);
        channels.iter().for_each(|(frame_sender, _)| {
            frame_sender.send(Arc::clone(&frame)).expect("Channel dead")
        });

        // check for frequency updates
        for (_, frequency_recv) in &channels {
            // we only care about the freshest value
            if let Some(report) = frequency_recv.try_iter().last() {
                frequency_tracker.update(report);
            }
        }
    }
}
