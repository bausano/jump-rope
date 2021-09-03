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
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

fn main() {
    ffmpeg::init().unwrap();

    let frequency_tracker = start_video_analysis();

    // bevy must always run on main thread
    ui::start(frequency_tracker);
}

// Starts iterating the video frames with various window sizes and updates the
// tracker with latest values.
//
// Returns a shared state abstraction to read the latest frequency.
fn start_video_analysis() -> Arc<FrequencyTracker> {
    // creates new one shot channel to send shared state reference because:
    // 1. bevy must run on the main thread
    // 2. [`FrameIter`] cannot be shared between threads safely after
    //    initialization
    let (sender, receiver) = channel();
    thread::spawn(move || {
        // TODO: camera might not be on this device
        let file = "/dev/video0";
        let frames = FrameIter::from_file(file).expect("Cannot load video");
        let frame_rate = frames.frame_rate();
        println!("FPS: {}", frame_rate);

        // create shared state abstraction and send a clone reference to
        // the main thread
        let frequency_tracker = Arc::new(FrequencyTracker::new(frame_rate));
        sender.send(Arc::clone(&frequency_tracker)).unwrap();

        // The larger the multiplier, the more granular frequency intervals it
        // can find. However, it takes longer to start reporting and it takes
        // longer to adjust to rapid speed changes.
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

            // TODO: if no update for long time, clean the tracker
            //
            // check for frequency updates
            for (_, frequency_recv) in &channels {
                // we only care about the freshest value
                if let Some(report) = frequency_recv.try_iter().last() {
                    frequency_tracker.update(report);
                }
            }
        }
    });

    receiver.recv().unwrap()
}
