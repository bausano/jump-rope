use std::{error::Error as StdError, result::Result as StdResult};

pub type Result<T> = StdResult<T, Box<dyn StdError>>;

/// Ratio between the number of oscillators who agree on a frequency, and the
/// total oscillators who identified any frequency.
pub const MIN_OSCILLATORS_AGREEMENT_RATIO: f32 = 1.0 / 2.0;

/// Size of the pixel square whose average value a single [`Oscillator`] tracks.
pub const VIEW_SIZE: u32 = 2;

/// Every n ms, frequency [`Analyzer`] reports current estimated frequency.
pub const REPORT_FREQUENCY_AFTER_MS: usize = 250;

/// Every n ms clean up work is done to avoid growing state buffers
/// indefinitely.
pub const TRUNCATE_STATE_AFTER_MS: usize = 2000;

/// The minimal magnitude of the aligned data (output of FFT) to consider the
/// frequency bin as relevant.
///
/// This has been experimentaly adjusted to be a good value for grayscale
/// to filter out noise.
pub const MAGNITUDE_THRESHOLD: f32 = 5.0;

/// For the use case of tracking jump roping frequencies, there's no point in
/// tracking anything slower than this.
pub const LOWEST_FREQUENCY_OF_INTEREST: f32 = 0.8;

/// Similar as [`LOWEST_FREQUENCY_OF_INTEREST`].
pub const HIGHEST_FREQUENCY_OF_INTEREST: f32 = 4.0;
