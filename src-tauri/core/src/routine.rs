//! Runs a routine's steps in order. Port of `src/main/launch/routine.ts`; the opener is injected, so it is
//! unit-tested without a window or a runtime.
use std::collections::HashMap;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use futures::FutureExt;

use crate::types::{Res, Routine, RoutineProgress, RoutineStatus, RoutineStep, Tile};

/// Runtime-agnostic async sleep: a short-lived thread completes a oneshot.
async fn sleep(d: Duration) {
    let (tx, rx) = futures::channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = tx.send(());
    });
    let _ = rx.await;
}

/// What the routine runner hands the `open` callback for one step.
#[derive(Debug, Clone)]
pub struct StepCtx {
    pub index: u32,
    pub step: RoutineStep,
    /// The step's tile; `None` for a window-only step.
    pub tile: Option<Tile>,
}

/// Opens each step's tile in order, `gap` apart (700 ms in production), reporting progress.
/// A missing tile, a failed open, or a panicking open fails that step only. A panicking `progress` is ignored.
pub async fn run_routine<O, OF, P>(r: &Routine, tiles: &[Tile], mut open: O, mut progress: P, gap: Duration) -> Res<()>
where
    O: FnMut(StepCtx) -> OF,
    OF: Future<Output = Res<()>>,
    P: FnMut(RoutineProgress),
{
    let total = r.steps.len() as u32;
    let mut emit = |step: u32, status: RoutineStatus, tile_id: Option<String>, ok: Option<bool>| {
        let e = RoutineProgress { routine_id: r.id.clone(), step, total, status, tile_id, ok };
        // A broken listener must not stop the routine.
        let _ = catch_unwind(AssertUnwindSafe(|| progress(e)));
    };

    if total == 0 {
        emit(0, RoutineStatus::Finished, None, Some(false));
        return Res::fail(format!("{0} has no steps yet. Right-click a tile \u{2192} Add to {0}.", r.name));
    }

    let by_id: HashMap<&str, &Tile> = tiles.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut failed: Vec<String> = vec![];
    for (i, s) in r.steps.iter().enumerate() {
        let i = i as u32;
        if i > 0 && !gap.is_zero() {
            sleep(gap).await;
        }
        let tile_id = s.tile_id.clone();
        emit(i, RoutineStatus::Running, Some(tile_id.clone()), None);
        let tile = by_id.get(tile_id.as_str()).copied();
        let mut ok = false;
        // A step runs if it has a tile, or if it is a window-only step (a hint, no tile).
        if tile.is_some() || (tile_id.is_empty() && s.hint.is_some()) {
            let ctx = StepCtx { index: i, step: s.clone(), tile: tile.cloned() };
            ok = match catch_unwind(AssertUnwindSafe(|| open(ctx))) {
                Ok(fut) => AssertUnwindSafe(fut).catch_unwind().await.map(|r| r.ok).unwrap_or(false),
                Err(_) => false,
            };
        }
        if !ok {
            failed.push(match (tile, &s.hint) {
                (Some(t), _) => t.name.clone(),
                (None, Some(h)) if tile_id.is_empty() => h.exe.trim_end_matches(".exe").to_string(),
                _ => "a deleted tile".to_string(),
            });
        }
        let status = if ok { RoutineStatus::Done } else { RoutineStatus::Failed };
        emit(i, status, Some(tile_id), Some(ok));
    }

    let ok = failed.is_empty();
    emit(total, RoutineStatus::Finished, None, Some(ok));
    if ok {
        return Res::ok(format!("{}: opened all {total}.", r.name));
    }
    Res::fail(format!(
        "{}: opened {} of {total}. Couldn't open {}.",
        r.name,
        total - failed.len() as u32,
        failed.join(", ")
    ))
}
