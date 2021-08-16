use rustfft::{num_complex::Complex, Fft};
use std::{cmp::Ordering, sync::Arc};

pub struct Oscillator {
    // Initiated object which can run FFT.
    fft: Arc<dyn Fft<f32>>,
    // Holds past samples, that is pixel grayscale values.
    state: Vec<u8>,
}

impl Oscillator {
    pub fn new(fft: Arc<dyn Fft<f32>>) -> Self {
        Self {
            fft,
            state: Vec::new(),
        }
    }

    pub fn push_pixel_value(&mut self, value: u8) {
        self.state.push(value);
    }

    pub fn truncate_state(&mut self, window: usize) {
        let len = self.state.len();
        if len > window {
            self.state.copy_within((len - window - 1).., 0);
            self.state.truncate(window);
        }
    }

    pub fn frequency(
        &self,
        sample_rate: usize,
        window: usize,
        scratch_a: &mut [Complex<f32>],
        scratch_b: &mut [Complex<f32>],
    ) -> Option<f32> {
        let bin = self.frequency_bin(window, scratch_a, scratch_b)?;

        Some((bin * sample_rate) as f32 / window as f32)
    }

    fn frequency_bin(
        &self,
        window: usize,
        scratch_a: &mut [Complex<f32>],
        scratch_b: &mut [Complex<f32>],
    ) -> Option<usize> {
        debug_assert_eq!(scratch_a.len(), window);
        debug_assert_eq!(scratch_b.len(), window);

        // not enough data yet to find necessary range of frequencies
        if window > self.state.len() {
            return None;
        }

        // set the buffer to the tail of the state with the len of the tail
        // given by window size
        for (index, val) in self
            .state
            .iter()
            .skip(self.state.len() - window)
            .enumerate()
        {
            let val = *val as f32; // todo: apply window function
            scratch_a[index] = Complex::new(val, 0.0);
        }

        self.fft.process_with_scratch(scratch_a, scratch_b);

        let mut iter = scratch_a.iter();
        let dc = (iter.next().unwrap().norm() / window as f32).floor() as u8;
        let (k, mag) =
            iter.map(|c| c.norm()).take(window / 2).enumerate().max_by(
                |(_, a), (_, b)| {
                    if a < b {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                },
            )?;
        let mag = (mag / (window as f32)).floor() as u8;
        if mag > 10 {
            //println!("dc {} mag  {}", dc, mag);
            Some(k)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::FftPlanner;

    #[test]
    fn it_finds_frequency() {
        let window = 128;
        let sample_rate = 30;

        let state = (0..window)
            .map(|n| {
                let n = n as f32;
                let real = 255.0 / 8.0 * ((n - 32.0) / 2.5).cos() + 64.0;

                real.round() as u8
            })
            .collect();

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(window);

        let oscillator = Oscillator { fft, state };

        let mut scratch_a = Vec::with_capacity(window);
        scratch_a.resize(window, Complex::default());

        let mut scratch_b = Vec::with_capacity(window);
        scratch_b.resize(window, Complex::default());

        assert_eq!(
            oscillator.frequency_bin(window, &mut scratch_a, &mut scratch_b),
            Some(7)
        );

        assert_eq!(
            oscillator.frequency(
                sample_rate,
                window,
                &mut scratch_a,
                &mut scratch_b
            ),
            Some(1.640625)
        );
    }
}
