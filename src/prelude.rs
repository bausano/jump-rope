use std::{error::Error as StdError, result::Result as StdResult};

pub type Result<T> = StdResult<T, Box<dyn StdError>>;
