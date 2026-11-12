//! Port of tests/unit/routine.test.ts. A panicking `open` stands in for the TS "throwing open".
use std::cell::RefCell;
use std::time::{Duration, Instant};

use bridge_core::routine::{run_routine, StepCtx};
use bridge_core::{Res, Routine, RoutineProgress, RoutineStatus, RoutineStep, Tile, TileKind};
use futures::executor::block_on;

fn tile(id: &str, name: &str) -> Tile {
    Tile {
        id: id.into(),
        group_id: "g".into(),
        name: name.into(),
        kind: TileKind::App,
        target: format!("C:\\{name}.exe"),
        args: None,
        keyword: None,
        subtitle: None,
        icon: None,
        created_at: String::new(),
        last_opened_at: None,
        open_count: 0,
    }
}

fn tiles() -> Vec<Tile> {
    vec![tile("t-a", "Alpha"), tile("t-b", "Bravo"), tile("t-c", "Charlie")]
}

fn routine(ids: &[&str]) -> Routine {
    Routine {
        id: "r-start".into(),
        name: "Start my day".into(),
        steps: ids.iter().map(|id| RoutineStep::tile(id.to_string())).collect(),
    }
}

fn status(s: RoutineStatus) -> &'static str {
    match s {
        RoutineStatus::Running => "running",
        RoutineStatus::Done => "done",
        RoutineStatus::Failed => "failed",
        RoutineStatus::Finished => "finished",
    }
}

#[test]
fn opens_steps_in_order_and_reports_progress() {
    let opened = RefCell::new(Vec::<String>::new());
    let mut events: Vec<RoutineProgress> = vec![];
    let res = block_on(run_routine(
        &routine(&["t-c", "t-a", "t-b"]),
        &tiles(),
        |c: StepCtx| {
            let t = c.tile.unwrap();
            opened.borrow_mut().push(t.id.clone());
            async move { Res::ok(format!("Opened {}", t.name)) }
        },
        |e| events.push(e),
        Duration::ZERO,
    ));
    assert_eq!(*opened.borrow(), vec!["t-c", "t-a", "t-b"]);
    assert!(res.ok);
    assert_eq!(res.message, "Start my day: opened all 3.");
    assert_eq!(
        events.iter().map(|e| format!("{}:{}", e.step, status(e.status))).collect::<Vec<_>>(),
        vec!["0:running", "0:done", "1:running", "1:done", "2:running", "2:done", "3:finished"]
    );
    assert!(events.iter().all(|e| e.routine_id == "r-start" && e.total == 3));
    assert_eq!(events[0].tile_id.as_deref(), Some("t-c"));
    assert_eq!(events[0].ok, None);
    let last = events.last().unwrap();
    assert_eq!((last.status, last.ok, last.tile_id.as_deref()), (RoutineStatus::Finished, Some(true), None));
}

#[test]
fn counts_a_missing_tile_as_failed_and_keeps_going() {
    let opened = RefCell::new(Vec::<String>::new());
    let mut events: Vec<RoutineProgress> = vec![];
    let res = block_on(run_routine(
        &routine(&["t-a", "t-gone", "t-b"]),
        &tiles(),
        |c: StepCtx| {
            let t = c.tile.unwrap();
            opened.borrow_mut().push(t.id.clone());
            async { Res::ok("") }
        },
        |e| events.push(e),
        Duration::ZERO,
    ));
    assert_eq!(*opened.borrow(), vec!["t-a", "t-b"]);
    let step1 = events.iter().find(|e| e.step == 1 && e.status != RoutineStatus::Running).unwrap();
    assert_eq!((step1.status, step1.ok, step1.tile_id.as_deref()), (RoutineStatus::Failed, Some(false), Some("t-gone")));
    assert!(!res.ok);
    assert_eq!(res.message, "Start my day: opened 2 of 3. Couldn't open a deleted tile.");
    assert_eq!(events.iter().filter(|e| e.status == RoutineStatus::Finished).count(), 1);
    let last = events.last().unwrap();
    assert_eq!((last.status, last.ok, last.step), (RoutineStatus::Finished, Some(false), 3));
}

#[test]
fn a_failed_or_throwing_open_fails_that_step_only_overall_ok_is_false() {
    let mut events: Vec<RoutineProgress> = vec![];
    let res = block_on(run_routine(
        &routine(&["t-a", "t-b", "t-c"]),
        &tiles(),
        |c: StepCtx| async move {
            let t = c.tile.unwrap();
            if t.id == "t-a" {
                return Res::fail("nope");
            }
            if t.id == "t-b" {
                panic!("boom");
            }
            Res::ok("")
        },
        |e| events.push(e),
        Duration::ZERO,
    ));
    let ids = |s: RoutineStatus| {
        events.iter().filter(|e| e.status == s).map(|e| e.tile_id.clone().unwrap()).collect::<Vec<_>>()
    };
    assert_eq!(ids(RoutineStatus::Failed), vec!["t-a", "t-b"]);
    assert_eq!(ids(RoutineStatus::Done), vec!["t-c"]);
    assert_eq!(events.iter().filter(|e| e.status == RoutineStatus::Finished).count(), 1);
    assert!(!res.ok);
    assert!(res.message.contains("Alpha"));
    assert!(res.message.contains("Bravo"));
    assert_eq!(res.message, "Start my day: opened 1 of 3. Couldn't open Alpha, Bravo.");
}

#[test]
fn an_empty_routine_finishes_once_and_is_not_ok() {
    let mut events: Vec<RoutineProgress> = vec![];
    let res = block_on(run_routine(&routine(&[]), &tiles(), |_c: StepCtx| async { Res::ok("") }, |e| events.push(e), Duration::ZERO));
    assert_eq!(
        events,
        vec![RoutineProgress {
            routine_id: "r-start".into(),
            step: 0,
            total: 0,
            status: RoutineStatus::Finished,
            tile_id: None,
            ok: Some(false)
        }]
    );
    assert!(!res.ok);
    assert_eq!(res.message, "Start my day has no steps yet. Right-click a tile \u{2192} Add to Start my day.");
}

#[test]
fn a_panicking_progress_listener_does_not_stop_the_routine() {
    let opened = RefCell::new(0);
    let res = block_on(run_routine(
        &routine(&["t-a", "t-b"]),
        &tiles(),
        |_c: StepCtx| {
            *opened.borrow_mut() += 1;
            async { Res::ok("") }
        },
        |_e| panic!("listener broke"),
        Duration::ZERO,
    ));
    assert_eq!(*opened.borrow(), 2);
    assert!(res.ok);
}

#[test]
fn waits_the_gap_between_steps_only() {
    let start = Instant::now();
    let res = block_on(run_routine(
        &routine(&["t-a", "t-b", "t-c"]),
        &tiles(),
        |_c: StepCtx| async { Res::ok("") },
        |_e| {},
        Duration::from_millis(40),
    ));
    let took = start.elapsed();
    assert!(res.ok);
    assert!(took >= Duration::from_millis(80), "{took:?}");
    assert!(took < Duration::from_millis(1000), "{took:?}");
}

#[test]
fn window_only_steps_run_and_are_named_by_their_app() {
    use bridge_core::WindowHint;
    let mut r = routine(&["t-a"]);
    r.steps.push(RoutineStep { hint: Some(WindowHint { exe: "ms-teams.exe".into(), ..Default::default() }), ..Default::default() });
    let res = block_on(run_routine(
        &r,
        &tiles(),
        |c: StepCtx| async move { if c.tile.is_some() { Res::ok("") } else { Res::fail("not open") } },
        |_e| {},
        Duration::ZERO,
    ));
    assert!(!res.ok);
    assert!(res.message.ends_with("Couldn't open ms-teams."), "{}", res.message);
}
