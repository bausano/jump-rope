use crate::analyzer;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Keeps track of latest frequencies for all window sizes and exports logic
/// to calculate the consensus.
pub struct FrequencyTracker {
    frame_rate: usize,
    inner: Mutex<BTreeMap<usize, analyzer::Report>>,
}

impl FrequencyTracker {
    pub fn new(frame_rate: usize) -> Self {
        Self {
            frame_rate,
            inner: Default::default(),
        }
    }

    pub fn update(&self, report: analyzer::Report) {
        let mut guard = self.inner.lock().unwrap();
        guard.insert(report.window, report);
    }

    pub fn calculate_latest(&self) -> Option<f32> {
        let guard = self.inner.lock().unwrap();
        let frequencies_ordered_by_window_size: Vec<_> =
            (*guard).values().cloned().collect();
        drop(guard);

        // we address the compromise where higher window size reports more
        // granular frequencies but takes longer to adjust to tempo changes:
        // - start with the roughest but most present estimate (i.e. lowest
        //  window size)
        // - in loop take more granular estimates as long as within range of
        //  the previous estimate (go up in window size)
        //
        //  TODO: should we care about frame index being up to date?
        frequencies_ordered_by_window_size
            .windows(2)
            .take_while(|pair| {
                let prev = &pair[0];
                let curr = &pair[1];

                // of sensitivity of a single bin in given window size
                let s = self.frame_rate as f32 / prev.window as f32;

                // all frequencies in this interval are sort of equivalent for
                // the sensitivity under given window size
                //
                // we don't use half bin width to one side and half to the
                // other purely to give more leeway to the output
                let interval = (prev.frequency - s)..(prev.frequency + s);

                // if the current report frequency is within the interval,
                // use the frequency from the current report
                interval.contains(&curr.frequency)
            })
            .last()
            .map(|report| dbg!(&report[1]).frequency)
    }
}
