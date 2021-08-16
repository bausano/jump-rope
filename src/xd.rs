use crate::oscillator::Oscillator;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::{collections::HashMap, sync::Arc};

pub struct Xd {
    // Initiated object which can run FFT.
    fft: Arc<dyn Fft<f32>>,
    // Map of pixel indices to objects which track them.
    oscillators: HashMap<usize, Oscillator>,
    // FPS of the video.
    frame_rate: usize,
    // How many samples to use for FFT.
    window: usize,
    // Allocated buffers for the FFT algorithm. They contain opaque data.
    scratch_buffers: (Vec<Complex<f32>>, Vec<Complex<f32>>),
}

impl Xd {
    pub fn new(frame_rate: usize, window: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(window);

        let create_buf = || {
            let mut buf = Vec::with_capacity(window);
            buf.resize(window, Complex::default());
            buf
        };

        Self {
            fft,
            frame_rate,
            window,
            oscillators: HashMap::new(),
            scratch_buffers: (create_buf(), create_buf()),
        }
    }

    pub fn frequency(&mut self) {
        let (ref mut a, ref mut b) = &mut self.scratch_buffers;

        let mut freqs = HashMap::new();
        for oscillator in self.oscillators.values() {
            let f = oscillator.frequency(self.frame_rate, self.window, a, b);
            if let Some(f) = f {
                if (0.5..10.0).contains(&f) {
                    let freq_n =
                        freqs.entry((1000.0 * f) as usize).or_insert(0);
                    *freq_n = *freq_n + 1;
                }
            }
        }

        // average
        // window function
        // find edges and focus on pixels on edges only
        // track pixel activity and evict inactive pixels
        // ignore pixel values which don't have large peaks
        if let Some(max) = freqs.values().max().copied() {
            let mut freqs: Vec<_> = freqs
                .into_iter()
                .filter(|(_, mag)| max / mag <= 100)
                .collect();
            freqs.sort_by_key(|(f, _)| *f);
            println!("{:?}", freqs);
        }
    }

    pub fn tracked_pixels(&self) -> impl Iterator<Item = &usize> {
        self.oscillators.keys()
    }

    pub fn track_pixel(&mut self, pixel_index: usize) {
        self.oscillators
            .insert(pixel_index, Oscillator::new(Arc::clone(&self.fft)));
    }

    pub fn update_pixel(&mut self, pixel_index: usize, pixel_value: u8) {
        self.oscillators
            .get_mut(&pixel_index)
            .expect("This pixel is not tracked")
            .push_pixel_value(pixel_value);
    }
}
