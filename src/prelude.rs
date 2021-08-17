use std::{error::Error as StdError, result::Result as StdResult};

pub type Result<T> = StdResult<T, Box<dyn StdError>>;

/// Ratio between the number of oscillators who agree on a frequency, and the
/// total oscillators who identified any frequency.
pub const MIN_OSCILLATORS_AGREEMENT_RATIO: f32 = 1.0 / 2.0;

pub const VIEW_SIZE: u32 = 2;

pub const UPDATE_FREQUENCY_AFTER_MS: usize = 250;

pub const TRUNCATE_STATE_AFTER_MS: usize = 2000;
