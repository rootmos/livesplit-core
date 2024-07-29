//! Calculates the current pace of the active attempt based on the comparison
//! provided. If there's no active attempt, the final time of the comparison is
//! returned instead.

use crate::{analysis, timing::Snapshot, TimeSpan, TimerPhase, AtomicDateTime};

/// Calculates the current pace of the active attempt based on the comparison
/// provided. If there's no active attempt, the final time of the comparison is
/// returned instead.
pub fn calculate(timer: &Snapshot<'_>, comparison: &str) -> (Option<TimeSpan>, bool) {
    let timing_method = timer.current_timing_method();
    let last_segment = timer.run().segments().last().unwrap();
    let phase = timer.current_phase();

    match phase {
        TimerPhase::Running | TimerPhase::Paused => {
            let mut delta = analysis::last_delta(
                timer.run(),
                timer.current_split_index().unwrap(),
                comparison,
                timing_method,
            )
            .unwrap_or_default();

            let mut is_live = false;

            catch! {
                let live_delta = timer.current_time()[timing_method]?
                    - timer.current_split().unwrap().comparison(comparison)[timing_method]?;

                if live_delta > delta {
                    delta = live_delta;
                    is_live = true;
                }
            };

            let value = catch! {
                last_segment.comparison(comparison)[timing_method]? + delta
            };

            (
                value,
                is_live && phase.updates_frequently(timing_method) && value.is_some(),
            )
        }
        TimerPhase::Ended => (last_segment.split_time()[timing_method], false),
        TimerPhase::NotRunning => (last_segment.comparison(comparison)[timing_method], false),
    }
}

pub fn predict_wall_clock_time(timer: &Snapshot<'_>, comparison: &str) -> (Option<AtomicDateTime>, bool) {
    if let (Some(cp), _) = calculate(timer, comparison) {
        let start = timer.get_start_time().unwrap_or_else(|| AtomicDateTime::now());
        let pause_time = timer.get_pause_time().unwrap_or_else(|| TimeSpan::zero()).to_duration();
        let finish = AtomicDateTime {
            time: start.time + cp.to_duration() + pause_time,
            synced_with_atomic_clock: start.synced_with_atomic_clock,
        };
        return (Some(finish), true); // TODO: is it correct to claim that it updates frequently?
    } else {
        return (None, false);
    }
}
