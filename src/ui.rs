use crate::frequency_tracker::FrequencyTracker;
use std::sync::Arc;

pub fn start_on_another_thread(tracker: Arc<FrequencyTracker>) {
    println!("{:?}", tracker.calculate_latest());
}
