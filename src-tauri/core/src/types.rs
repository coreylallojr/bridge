//! Shared data contracts. Mirror of `src/shared/types.ts`; change both together.
use serde::{Deserialize, Serialize};
pub use crate::displays::Rect;

/// The built-in routine behind the big button. Always present in a valid profile.
pub const START_MY_DAY_ID: &str = "r-start";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileKind {
    Url,
    App,
    Folder,
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub kind: TileKind,
    /// URL, absolute path, `shell:AppsFolder\<AppID>`, or protocol URI (e.g. `ms-teams:`).
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// File name inside `<BRIDGE_HOME>/icons`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<String>,
    pub open_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
}

/// Where a step's window goes: a monitor (stable display id) and a zone name (see `zones::ZONES`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub display_id: i64,
    pub zone: String,
}

/// How to recognise a step's window among the open ones: the process file name (lowercase, e.g. `ms-teams.exe`),
/// optionally a title fragment, and which one if that app has several windows (0-based, in z-order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WindowHint {
    pub exe: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nth: Option<u32>,
}

/// One step of a routine/workspace. `tile_id` empty = a window-only step (arrange an app that's already open).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutineStep {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Placement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<WindowHint>,
}

impl RoutineStep {
    pub fn tile(tile_id: impl Into<String>) -> Self {
        RoutineStep { tile_id: tile_id.into(), ..Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub steps: Vec<RoutineStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    Auto,
    Pinned,
    Summon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hotkey: String,
    pub mode: DisplayMode,
    /// Stable display id (FNV-1a of the device name); null = choose automatically.
    pub pinned_display_id: Option<i64>,
    pub calm_motion: bool,
    /// Theme preset: `command` (default), `ember`, `mono`, `contrast`.
    pub theme: String,
    /// Optional accent override, `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Glass strength 0–100 (panel opacity + blur).
    pub glass: u8,
    /// `comfortable` or `compact`.
    pub density: String,
    /// UI text size, percent (85–130).
    pub font_scale: u8,
    /// Reactor frame cap while focused: 15, 24, 30 or 60.
    pub fps: u8,
    /// Reactor render quality: `low`, `medium`, `high`.
    pub quality: String,
    /// Side widgets, in order. Known: routine, recent, pc, focus, notes, clock.
    pub widgets: Vec<String>,
}

pub const DEFAULT_WIDGETS: [&str; 3] = ["routine", "recent", "pc"];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey: "Alt+Space".into(),
            mode: DisplayMode::Auto,
            pinned_display_id: None,
            calm_motion: false,
            theme: "command".into(),
            accent: None,
            glass: 60,
            density: "comfortable".into(),
            font_scale: 100,
            fps: 30,
            quality: "medium".into(),
            widgets: DEFAULT_WIDGETS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub groups: Vec<Group>,
    pub tiles: Vec<Tile>,
    pub routines: Vec<Routine>,
    pub settings: Settings,
}

/// `Result<T>` in TS: `{ ok, message, data? }`. `message` is plain language, safe to show the user as-is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Res<T = ()> {
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> Res<T> {
    pub fn ok(message: impl Into<String>) -> Self {
        Res { ok: true, message: message.into(), data: None }
    }
    pub fn ok_with(message: impl Into<String>, data: T) -> Self {
        Res { ok: true, message: message.into(), data: Some(data) }
    }
    pub fn fail(message: impl Into<String>) -> Self {
        Res { ok: false, message: message.into(), data: None }
    }
    /// Same ok/message, data dropped (e.g. a `Res<Tile>` answered over the CLI pipe).
    pub fn plain(self) -> Res<()> {
        Res { ok: self.ok, message: self.message, data: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub name: String,
    pub app_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: i64,
    /// Physical-pixel bounds and work area (for drawing the monitors to scale).
    #[serde(default)]
    pub bounds: Rect,
    #[serde(default)]
    pub work: Rect,
    /// "Screen 2 · 1920×1080"
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    pub portrait: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellMode {
    Pinned,
    Summon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellState {
    pub mode: ShellMode,
    pub display_id: i64,
    /// 1-based screen number for labels ("Pinned to screen 2").
    pub screen_number: u32,
    pub hotkey: String,
    pub hotkey_ok: bool,
    /// The exact PowerShell profile line the user can add (shown, never auto-installed).
    pub profile_line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PcStats {
    /// 0–100
    pub cpu: u8,
    /// 0–100
    pub mem: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineStatus {
    Running,
    Done,
    Failed,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineProgress {
    pub routine_id: String,
    /// 0-based step index; for `finished` equals total.
    pub step: u32,
    pub total: u32,
    pub status: RoutineStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToastTone {
    Info,
    Ok,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiCommand {
    FocusPalette,
    StartMyDay,
    OpenSettings,
}

/// Main → renderer events on the `bridge:event` channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum BridgeEvent {
    Profile { profile: Profile },
    Shell { state: ShellState },
    Routine { progress: RoutineProgress },
    Launched { tile_id: String, ok: bool, message: String },
    Toast { message: String, tone: ToastTone },
    Command { command: UiCommand },
}

/// `TilePatch` in `src/shared/ipc.ts`: the editable subset of a tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TilePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}

/// `Partial<Settings>` from the renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<DisplayMode>,
    /// `Some(None)` = explicitly set to null ("choose automatically").
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    pub pinned_display_id: Option<Option<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calm_motion: Option<bool>,
}

fn double_option<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(d).map(Some)
}
