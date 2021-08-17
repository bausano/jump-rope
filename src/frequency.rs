use crate::oscillator::{Oscillator, WindowFn};
use crate::prelude::*;
use image::GrayImage;
use rand::{thread_rng, Rng};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

/// This value is streamed from the spawned analyzer thread to update on what
/// frequency has been identified.
#[derive(Debug)]
pub struct Report {
    pub window: usize,
    pub frame_index: usize,
    pub frequency: f32,
}

pub struct AnalyzerBuilder {
    pub frame_rate: usize,
    /// How many past values to analyze in FFT.
    pub window: usize,
    pub frame_width: u32,
    pub frame_height: u32,
}

/// Spawns a new thread based on the settings given. The returned sender updates
/// the spawned analyzer thread on new frames. In consistent intervals, the
/// thread updates the receiver on what frequency it thinks is most prevalent
/// in the video.
pub fn analyzer_channel(
    builder: AnalyzerBuilder,
) -> (Sender<Arc<GrayImage>>, Receiver<Report>) {
    let AnalyzerBuilder {
        frame_rate,
        window,
        frame_width,
        frame_height,
    } = builder;

    let mut rng = thread_rng();
    let mut analyzer = Analyzer::new(frame_rate, window);

    let oscillators_count = frame_width as usize * frame_height as usize / 25;
    analyzer.init_oscillators(
        &mut rng,
        oscillators_count,
        frame_width,
        frame_height,
    );

    let (frame_sender, frame_recv) = channel::<Arc<_>>();
    let (frequency_sender, frequency_recv) = channel();

    thread::spawn(move || {
        let frames_per_ms = analyzer.frame_rate as f32 / 1000.0;

        let update_frequency_every_nth_frame =
            (REPORT_FREQUENCY_AFTER_MS as f32 * frames_per_ms) as usize;
        let truncate_state_every_nth_frame =
            (TRUNCATE_STATE_AFTER_MS as f32 * frames_per_ms) as usize;

        // with these iterator we make a fundamental but justified assumption
        // that it on average takes longer time to deliver new messages than
        // to process them
        //
        // if new frames are produced faster than this loop can process them,
        // then delay between real time and output keeps widening
        //
        // however most cameras have pretty low FPS and the computation we do
        // on average is super cheap
        let mut frames = frame_recv.iter().enumerate();
        while let Some((frame_index, frame)) = frames.next() {
            // pushes pixel values to relevant oscillators
            analyzer.push_pixel_values_to_oscillators(&frame);

            if frame_index % update_frequency_every_nth_frame == 0 {
                if let Some(f) = analyzer.frequency() {
                    frequency_sender
                        .send(Report {
                            frame_index,
                            frequency: f,
                            window,
                        })
                        .expect("Channel died");
                }
            }

            if frame_index % truncate_state_every_nth_frame == 0 {
                analyzer.truncate_state();
            }
        }
    });

    (frame_sender, frequency_recv)
}

// Keeps bunch of oscillators that keep track of video state history and return
// frequencies in that state (each oscillator sees [`VIEW_SIZE`] pixels).
//
// The [`Analyzer`] can then put together estimates from each oscillator and
// average it to get the final frequency.
struct Analyzer {
    // Initiated object which can run FFT.
    fft: Arc<dyn Fft<f32>>,
    // Map of pixel indices to objects which track them.
    oscillators: HashMap<(u32, u32), Oscillator>,
    // FPS of the video.
    frame_rate: usize,
    // How many samples to use for FFT.
    window: usize,
    // Precomputed values of function which scales oscillator's state.
    window_fn: WindowFn,
    // Allocated buffers for the FFT algorithm. They contain opaque data.
    scratch_buffers: (Vec<Complex<f32>>, Vec<Complex<f32>>),
}

impl Analyzer {
    pub fn new(frame_rate: usize, window: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(window);
        let window_fn = WindowFn::blackman(window);

        let create_buf = || {
            let mut buf = Vec::with_capacity(window);
            buf.resize(window, Complex::default());
            buf
        };

        Self {
            fft,
            frame_rate,
            window,
            window_fn,
            oscillators: HashMap::new(),
            scratch_buffers: (create_buf(), create_buf()),
        }
    }

    // Creates `oscillators_count` randomly placed (on a frame) oscillators
    // which will track average values of some small frame square.
    fn init_oscillators(
        &mut self,
        rng: &mut impl Rng,
        oscillators_count: usize,
        width: u32,
        height: u32,
    ) {
        for _ in 0..oscillators_count {
            let x = rng.gen_range(0..(width - VIEW_SIZE));
            let y = rng.gen_range(0..(height - VIEW_SIZE));
            self.oscillators.insert(
                (x, y),
                Oscillator::new(Arc::clone(&self.fft), self.window_fn.clone()),
            );
        }
    }

    fn push_pixel_values_to_oscillators(&mut self, frame: &GrayImage) {
        let p = |x, y| frame[(x, y)].0[0] as u32;

        for ((x, y), oscillator) in &mut self.oscillators {
            let (x, y) = (*x, *y);
            // IMPORTANT: keep VIEW_SIZE the same as additions count here
            // TODO: change the square size with change in VIEW_SIZE
            let val = ((p(x, y) + p(x, y + 1) + p(x + 1, y) + p(x + 1, y + 1))
                / VIEW_SIZE) as u8;
            oscillator.push_pixel_value(val);
        }
    }

    fn frequency(&mut self) -> Option<f32> {
        // Allows us to focus on frequencies in which people usually jump (not
        // too slow, not too fast).
        //
        // If this software were to extend to other domains, the frequencies of
        // interest would have to be adjusted.
        let relevant_bins = self.frequency_to_bin(LOWEST_FREQUENCY_OF_INTEREST)
            ..=self.frequency_to_bin(HIGHEST_FREQUENCY_OF_INTEREST);

        // to prevent reinitializing memory, we keep these buffers
        let (ref mut a, ref mut b) = &mut self.scratch_buffers;

        // index = bin
        // value = how many oscillators resonate in the bin frequency interval
        let mut bins_count: Vec<usize> = vec![];
        bins_count.resize(self.window / 2, 0);

        for oscillator in self.oscillators.values() {
            if let Some(bin) =
                oscillator.frequency_bin(relevant_bins.clone(), a, b)
            {
                bins_count[bin] += 1;
            }
        }

        // find the couple of adjacent frequencies which together have the
        // highest resonating oscillators
        let (bin1, largest_couple) = bins_count
            .windows(2)
            .enumerate()
            .max_by_key(|(_, b)| b[0] + b[1])
            .unwrap();
        let largest_couple_oscillators_count =
            (largest_couple[0] + largest_couple[1]) as f32;
        let oscillator_count: usize = bins_count.iter().sum();

        if largest_couple_oscillators_count / oscillator_count as f32
            > MIN_OSCILLATORS_AGREEMENT_RATIO
        {
            let f1 = self.bin_to_frequency(bin1);
            let f1_share =
                largest_couple[0] as f32 / largest_couple_oscillators_count;

            let f2 = self.bin_to_frequency(bin1 + 1);
            let f2_share =
                largest_couple[1] as f32 / largest_couple_oscillators_count;

            Some(f1 * f1_share + f2 * f2_share)
        } else {
            None
        }
    }

    fn truncate_state(&mut self) {
        for oscillator in self.oscillators.values_mut() {
            oscillator.truncate_state();
        }
    }

    fn frequency_to_bin(&self, f: f32) -> usize {
        (f * self.window as f32 / self.frame_rate as f32).floor() as usize
    }

    fn bin_to_frequency(&self, bin: usize) -> f32 {
        (bin * self.frame_rate) as f32 / self.window as f32
    }
}
