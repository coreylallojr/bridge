//! Bridge's pure logic. No Tauri, no Win32: everything here is unit-testable with `cargo test -p bridge-core`.
//! The JSON wire format mirrors `src/shared/types.ts` exactly (camelCase, `type`-tagged events).
pub mod types;

// Task 2
pub mod profile;
// Task 3a: input + launch planning
pub mod apps;
pub mod detect;
pub mod env;
pub mod launch_plan;
pub mod matcher;
pub mod meta_parse;
pub mod safe_fetch;
pub mod schemes;
// Task 3b: CLI + shell logic
pub mod accel;
pub mod command;
pub mod displays;
pub mod functions;
pub mod pipe_proto;
pub mod routine;
pub mod stats;
pub mod zones;

pub use types::*;
