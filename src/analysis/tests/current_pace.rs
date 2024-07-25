use crate::{
    analysis,
    util::tests_helper::{
        create_timer,
        run_with_splits_opt,
    },
    comparison::best_segments,
};

#[test]
fn predict_wall_clock_time() {
    let mut timer = create_timer(&["A", "B", "C"]);
    run_with_splits_opt(&mut timer, &[Some(5.0), Some(20.0), Some(60.0)]);

    timer.start().unwrap();
    let start = timer.get_start_time().unwrap();

    let snap = timer.snapshot();
    let cmp = &best_segments::NAME;
    let (current_pace, uf0) = analysis::current_pace::calculate(&snap, cmp);

    let (predicted_time, uf1) = analysis::current_pace::predict_wall_clock_time(&snap, cmp);

    let finish = start.time + current_pace.unwrap().to_duration();

    assert_eq!(uf0, uf1);
    assert_eq!(finish, predicted_time.unwrap().time);
}
