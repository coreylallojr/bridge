//! Bridge shell (Task 5): the main window and its display modes, z-order, displays, display events, tray, global
//! hotkey, single instance, and the CLI pipe server. Pure decisions live in `bridge-core`. Windows only.

pub mod display_events;
pub mod displays;
pub mod hotkey;
pub mod instance;
pub mod pipe_server;
pub mod tray;
pub mod window;
pub mod zorder;

pub use window::WindowManager;
