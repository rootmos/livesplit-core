use crate::{
    analysis,
    util::tests_helper::{
        create_timer,
        run_with_splits,
    },
    comparison::best_segments,
};

use std::{
    thread,
    time::Duration,
};

#[test]
fn predict_wall_clock_time() {
    let mut timer = create_timer(&["A"]);
    run_with_splits(&mut timer, &[60.0]);

    timer.start().unwrap();
    let start = timer.get_start_time().unwrap();

    let snap = timer.snapshot();
    let cmp = &best_segments::NAME;
    let (current_pace, _) = analysis::current_pace::calculate(&snap, cmp);

    let (predicted_time, uf1) = analysis::current_pace::predict_wall_clock_time(&snap, cmp);

    let finish = start.time + current_pace.unwrap().to_duration();

    assert_eq!(uf1, true);
    assert_eq!(finish, predicted_time.unwrap().time);
}

#[test]
fn predicted_time_doesnt_change_while_running() {
    let mut timer = create_timer(&["A"]);
    run_with_splits(&mut timer, &[60.0]);

    timer.start().unwrap();
    let start = timer.get_start_time().unwrap().time;

    let cmp = &best_segments::NAME;

    let snap0 = timer.snapshot();
    let (predicted_time0, _) = analysis::current_pace::predict_wall_clock_time(&snap0, cmp);

    let d0 = predicted_time0.unwrap().time - start;
    assert_eq!(d0, Duration::from_secs(60));

    thread::sleep(Duration::from_secs(5));

    let snap1 = timer.snapshot();
    let (predicted_time1, _) = analysis::current_pace::predict_wall_clock_time(&snap1, cmp);

    assert_eq!(predicted_time0.unwrap(), predicted_time1.unwrap());

    let d1 = predicted_time1.unwrap().time - start;
    assert_eq!(d1, Duration::from_secs(60));
}
