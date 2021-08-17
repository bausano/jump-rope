use rustfft::{num_complex::Complex, Fft};
use std::cmp::Ordering;
use std::f32::consts::PI;
use std::ops::RangeInclusive;
use std::sync::Arc;

pub struct Oscillator {
    // Initiated object which can run FFT.
    fft: Arc<dyn Fft<f32>>,
    // Determines how are values prepared before FFT is ran.
    window_fn: WindowFn,
    // Holds past samples, that is pixel grayscale values.
    state: Vec<u8>,
}

impl Oscillator {
    pub fn new(fft: Arc<dyn Fft<f32>>, window_fn: WindowFn) -> Self {
        Self {
            fft,
            state: Vec::new(),
            window_fn,
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

    pub fn frequency_bin(
        &self,
        window: usize,
        relevant_bins: RangeInclusive<usize>,
        scratch_a: &mut [Complex<f32>],
        scratch_b: &mut [Complex<f32>],
    ) -> Option<usize> {
        debug_assert_eq!(scratch_a.len(), window);
        debug_assert_eq!(scratch_b.len(), window);

        // not enough data yet to find necessary range of frequencies
        if window > self.state.len() {
            return None;
        }

        let get_state_in_window =
            || self.state.iter().skip(self.state.len() - window);

        let average = (get_state_in_window()
            .fold(0, |acc, p| acc + *p as usize)
            / self.state.len()) as u8;
        let error = (get_state_in_window()
            .map(|v| v.max(&average) - v.min(&average))
            .map(|v| v as usize)
            .sum::<usize>()
            / self.state.len()) as u8;

        if error > 10 {
            /*
            let xd = self.state.iter().enumerate().fold(
                String::new(),
                |mut acc, (k, g)| {
                    acc.push_str(&format!("({},{}),", k, g));
                    acc
                },
            );
            println!("{}", xd);
            */
        } else {
            return None;
        }

        // inserts the state of the oscillator into given buffer after applying
        // window function and alike
        self.populate_buffer_with_state(window, scratch_a);

        // stores fft bins into first buffer
        self.fft.process_with_scratch(scratch_a, scratch_b);

        // looks at the greatest peak in the output and returns the index
        // (frequency bin) and magnitude (converted to grayscale)
        largest_bin(window, relevant_bins, scratch_a.iter())
    }

    // Set the buffer to the tail of the state where the len of the tail is
    // given by window size.
    fn populate_buffer_with_state(
        &self,
        window: usize,
        scratch_a: &mut [Complex<f32>],
    ) {
        for (index, grayness_byte) in self
            .state
            .iter()
            .skip(self.state.len() - window)
            .enumerate()
        {
            let real = *grayness_byte as f32 * self.window_fn.apply(index);
            scratch_a[index] = Complex::new(real, 0.0);
        }
    }
}

// Finds the frequency bin with the highest magnitude and returns its index.
fn largest_bin<'a>(
    window: usize,
    relevant_bins: RangeInclusive<usize>,
    mut bins: impl Iterator<Item = &'a Complex<f32>>,
) -> Option<usize> {
    // the average grayscale pixel value is not used
    let _dc = bins.next();

    bins.map(|c| c.norm())
        // because we only use real values for inputs, the FFT duplicates the
        // bands into second half, therefore we cut it off
        .take(window / 2)
        .map(|mag| mag / window as f32)
        .enumerate()
        .skip(*relevant_bins.start())
        .take(relevant_bins.count())
        .max_by(|(_, a), (_, b)| {
            if a < b {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        })
        .filter(|(_, mag)| *mag > 5.0)
        // we've skipped the dc on zeroth index
        .map(|(k, _)| k + 1)
}

/// Precomputed values by which relevant time value is multiplied to avoid
/// leakage.
///
/// https://www.edn.com/windowing-functions-improve-fft-results-part-i
#[derive(Clone)]
pub struct WindowFn(Vec<f32>);

impl WindowFn {
    #[allow(dead_code)]
    pub fn blackman(window: usize) -> Self {
        let precomputed = (0..window)
            .map(|n| {
                let n = n as f32;
                let window = window as f32;

                0.42 - 0.5 * ((2.0 * PI * n) / window).cos()
                    + 0.08 * ((4.0 * PI * n) / window).cos()
            })
            .map(|scalar| scalar.clamp(0.0, 1.0))
            .collect();

        Self(precomputed)
    }

    #[allow(dead_code)]
    pub fn sine_lobe(window: usize) -> Self {
        let precomputed = (0..window)
            .map(|n| {
                let n = n as f32;
                let window = window as f32;

                (PI * n / window).sin()
            })
            .collect();

        Self(precomputed)
    }

    #[allow(dead_code)]
    pub fn rectangular(window: usize) -> Self {
        let precomputed = (0..window).map(|_| 1.0).collect();

        Self(precomputed)
    }

    fn apply(&self, n: usize) -> f32 {
        self.0[n]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::FftPlanner;

    #[test]
    fn it_finds_frequency_bin() {
        let window = 128;
        let relevant_bins = 0..=window;

        let state = (0..window)
            .map(|n| {
                let n = n as f32;
                let real = 255.0 / 8.0 * ((n - 32.0) / 2.5).cos() + 64.0;

                real.round() as u8
            })
            .collect();

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(window);
        let window_fn = WindowFn::rectangular(window);

        let oscillator = Oscillator {
            fft,
            window_fn,
            state,
        };

        let mut scratch_a = Vec::with_capacity(window);
        scratch_a.resize(window, Complex::default());

        let mut scratch_b = Vec::with_capacity(window);
        scratch_b.resize(window, Complex::default());

        assert_eq!(
            oscillator.frequency_bin(
                window,
                relevant_bins.clone(),
                &mut scratch_a,
                &mut scratch_b
            ),
            Some(7)
        );
    }
}
