//! Bridge OS services (Task 4): launch, clipboard, stats, icons, meta, apps, dragdrop.
//! Everything Win32 / network lives here; the pure decisions live in `bridge-core`.
pub mod apps;
pub mod clipboard;
pub mod dragdrop;
pub mod icons;
pub mod launch;
pub mod meta;
pub mod stats;
pub mod winmgr;

mod win;
