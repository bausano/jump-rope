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
use std::thread;

fn main() {
    ffmpeg::init().unwrap();

    // TODO: don't hard code FPS but get it from camera settings
    let frequency_tracker = Arc::new(FrequencyTracker::new(10));

    let frequency_tracker_clone = Arc::clone(&frequency_tracker);
    thread::spawn(move || start_video_analysis(frequency_tracker_clone));

    ui::start(frequency_tracker);
}

// Starts iterating the video frames with various window sizes and updates the
// tracker with latest values.
//
// # Important
// This method blocks until video stops.
fn start_video_analysis(frequency_tracker: Arc<FrequencyTracker>) {
    // TODO: camera might not be on this device
    let file = "/dev/video0";
    let frames = FrameIter::from_file(file).expect("Cannot load video");
    let frame_rate = frames.frame_rate();
    println!("FPS: {}", frame_rate);

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

        // TODO: if no update for long time, clean tracker
        // check for frequency updates
        for (_, frequency_recv) in &channels {
            // we only care about the freshest value
            if let Some(report) = frequency_recv.try_iter().last() {
                frequency_tracker.update(report);
            }
        }
    }
}
