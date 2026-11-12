//! the user's profile on disk: `<home>/profile.json`. Port of `src/main/profile/store.ts`.
//!
//! Writes are synchronous and atomic (tmp + rename), with the previous file kept as `profile.json.bak`. A damaged file
//! is set aside, never deleted. A hand edit made while Bridge runs is adopted on the next update.
use std::ffi::OsString;
use std::fs;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::schema::parse_profile_report;
use crate::types::Profile;

pub const NOTICE_FROM_BACKUP: &str = "Your profile file was damaged, so Bridge loaded the last good copy.";
pub const NOTICE_DROPPED: &str =
    "Some items in your profile couldn't be read, so Bridge left them out. The original file was kept in the Bridge folder.";
pub const NOTICE_EXTERNAL_DAMAGED: &str =
    "Your edited profile file couldn't be read, so Bridge kept its own copy. Your edited file was kept in the Bridge folder.";
pub const NOTICE_FROM_STARTER: &str =
    "Your profile file was damaged, so Bridge started fresh. The damaged file was kept in the Bridge folder.";

/// Local time, `YYYYMMDD-HHMMSS`.
fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// `<file><suffix>`, e.g. `profile.json.bak`.
fn suffixed(file: &Path, suffix: &str) -> PathBuf {
    let mut s: OsString = file.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// UTF-8 text (lossy, like Node's `readFileSync(f, 'utf8')`) without a leading BOM.
fn read_text(file: &Path) -> io::Result<String> {
    let text = String::from_utf8_lossy(&fs::read(file)?).into_owned();
    Ok(match text.strip_prefix('\u{FEFF}') {
        Some(rest) => rest.to_owned(),
        None => text,
    })
}

/// Read + parse + validate. `Ok(None)` when the content is damaged; I/O errors (e.g. file locked) are returned.
fn try_load(file: &Path) -> io::Result<Option<(Profile, usize)>> {
    let text = read_text(file)?;
    Ok(serde_json::from_str::<Value>(&text).ok().and_then(|v| parse_profile_report(v).ok()))
}

fn try_load_quiet(file: &Path) -> bool {
    matches!(try_load(file), Ok(Some(_)))
}

/// Size + mtime, to notice when someone (the user in Notepad) edited the file behind Bridge's back. None = missing.
type Sig = Option<(u64, SystemTime)>;

fn signature(file: &Path) -> Sig {
    let m = fs::metadata(file).ok()?;
    Some((m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
}

fn load_starter(starter: &Path) -> Profile {
    read_text(starter)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| parse_profile_report(v).ok())
        .map(|(p, _)| p)
        .unwrap_or_else(|| parse_profile_report(Value::Object(Default::default())).expect("{} is a valid profile").0)
}

/// EPERM / EBUSY / EACCES in Node terms.
fn is_transient(e: &io::Error) -> bool {
    if matches!(e.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy) {
        return true;
    }
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Windows can briefly lock a file (antivirus, indexer). Retry a rename 5 times (25, 50, … 125 ms) before giving up.
fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt: u64 = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if attempt >= 5 || !is_transient(&e) => return Err(e),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(25 * (attempt + 1)));
                attempt += 1;
            }
        }
    }
}

/// Copy (or move) `file` to `<home>/profile.<label>-<stamp>[-N].json`, never overwriting an earlier copy.
fn keep_copy(file: &Path, home: &Path, label: &str, mv: bool) -> io::Result<()> {
    let mut dest = home.join(format!("profile.{label}-{}.json", stamp()));
    let mut i = 2;
    while dest.exists() {
        dest = home.join(format!("profile.{label}-{}-{i}.json", stamp()));
        i += 1;
    }
    if mv {
        rename_with_retry(file, &dest)
    } else {
        fs::copy(file, &dest).map(|_| ())
    }
}

/// tmp → (current file → .bak, only if it parses) → rename. Returns the new signature.
fn write(file: &Path, p: &Profile) -> io::Result<Sig> {
    let tmp = suffixed(file, ".tmp");
    let mut text = serde_json::to_string_pretty(p).map_err(io::Error::other)?;
    text.push('\n');
    fs::write(&tmp, text)?;
    // Only a readable file becomes the backup, so a broken hand edit can never replace the last good copy.
    if file.exists() && try_load_quiet(file) {
        fs::copy(file, suffixed(file, ".bak"))?;
    }
    rename_with_retry(&tmp, file)?;
    Ok(signature(file))
}

/// Recover from a poisoned lock: a panicking update closure only ever touched a draft copy.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Returned by `on_change` / `on_notice`; pass it to `off` to unsubscribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

type ChangeFn = Arc<dyn Fn(&Profile) + Send + Sync>;
type NoticeFn = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Default)]
struct Listeners {
    next: u64,
    change: Vec<(u64, ChangeFn)>,
    notice: Vec<(u64, NoticeFn)>,
}

struct State {
    profile: Profile,
    disk_sig: Sig,
}

pub struct ProfileStore {
    file: PathBuf,
    home: PathBuf,
    load_notice: Option<String>,
    state: Mutex<State>,
    listeners: Mutex<Listeners>,
}

impl ProfileStore {
    fn new(file: PathBuf, home: &Path, profile: Profile, load_notice: Option<&str>, disk_sig: Sig) -> Self {
        ProfileStore {
            file,
            home: home.to_path_buf(),
            load_notice: load_notice.map(str::to_owned),
            state: Mutex::new(State { profile, disk_sig }),
            listeners: Mutex::new(Listeners::default()),
        }
    }

    /// Open (or create) `<home>/profile.json`. A damaged file is not an error (see `load_notice`); only I/O
    /// failures are (home not creatable, file locked, copy/write failed).
    pub fn open(home: &Path, starter: &Path) -> io::Result<ProfileStore> {
        fs::create_dir_all(home)?;
        let file = home.join("profile.json");

        if !file.exists() {
            let profile = load_starter(starter);
            let sig = write(&file, &profile)?;
            return Ok(Self::new(file, home, profile, None, sig));
        }

        match try_load(&file)? {
            Some((profile, 0)) => {
                let sig = signature(&file);
                return Ok(Self::new(file, home, profile, None, sig));
            }
            Some((profile, _)) => {
                // Readable, but some items were unreadable: keep the untouched original before anything rewrites it.
                keep_copy(&file, home, "dropped", false)?;
                let sig = signature(&file);
                return Ok(Self::new(file, home, profile, Some(NOTICE_DROPPED), sig));
            }
            None => {}
        }

        // Damaged: keep it aside, then fall back to the backup, then the starter.
        keep_copy(&file, home, "corrupt", true)?;
        let bak = suffixed(&file, ".bak");
        let from_bak = if bak.exists() { try_load(&bak).ok().flatten().map(|(p, _)| p) } else { None };
        let (profile, notice) = match from_bak {
            Some(p) => (p, NOTICE_FROM_BACKUP),
            None => (load_starter(starter), NOTICE_FROM_STARTER),
        };
        let sig = write(&file, &profile)?;
        Ok(Self::new(file, home, profile, Some(notice), sig))
    }

    /// Plain-language message when opening had to repair or replace the file; None when it loaded cleanly.
    pub fn load_notice(&self) -> Option<String> {
        self.load_notice.clone()
    }

    /// A copy of the current profile. Mutating it changes nothing; use `update`.
    pub fn get(&self) -> Profile {
        lock(&self.state).profile.clone()
    }

    /// Apply `f` to a copy, normalize, save, then notify listeners. Returns a copy of the saved profile.
    /// If the save fails, nothing changes. If `f` panics, nothing changes and the panic propagates.
    /// `f` runs under the store lock: it must not call back into the store.
    pub fn update(&self, f: impl FnOnce(&mut Profile)) -> Result<Profile, String> {
        self.try_update(|p| {
            f(p);
            Ok(())
        })
    }

    /// Like `update`, but `f` can refuse: an `Err` from `f` is returned as-is and nothing changes.
    pub fn try_update(&self, f: impl FnOnce(&mut Profile) -> Result<(), String>) -> Result<Profile, String> {
        let mut notices: Vec<&'static str> = Vec::new();
        let result = {
            let mut st = lock(&self.state);
            self.update_locked(&mut st, f, &mut notices)
        };
        // Listeners run outside the lock, so they may call `get()`.
        for n in notices {
            self.notify(n);
        }
        let next = result?;
        let listeners: Vec<ChangeFn> = lock(&self.listeners).change.iter().map(|(_, cb)| cb.clone()).collect();
        for cb in listeners {
            if catch_unwind(AssertUnwindSafe(|| cb(&next))).is_err() {
                eprintln!("[profile] onChange listener failed");
            }
        }
        Ok(next)
    }

    fn update_locked(
        &self,
        st: &mut State,
        f: impl FnOnce(&mut Profile) -> Result<(), String>,
        notices: &mut Vec<&'static str>,
    ) -> Result<Profile, String> {
        self.sync_from_disk(st, notices).map_err(|e| e.to_string())?;
        let mut draft = st.profile.clone();
        f(&mut draft)?;
        let value = serde_json::to_value(&draft).map_err(|e| e.to_string())?;
        let (next, _) = parse_profile_report(value)?;
        st.disk_sig = write(&self.file, &next).map_err(|e| e.to_string())?;
        st.profile = next.clone();
        Ok(next)
    }

    /// Called with the new profile after every successful save.
    pub fn on_change(&self, cb: Box<dyn Fn(&Profile) + Send + Sync>) -> ListenerId {
        let mut l = lock(&self.listeners);
        l.next += 1;
        let id = l.next;
        l.change.push((id, Arc::from(cb)));
        ListenerId(id)
    }

    /// Called with a plain-language message when Bridge had to set an outside edit aside.
    pub fn on_notice(&self, cb: Box<dyn Fn(&str) + Send + Sync>) -> ListenerId {
        let mut l = lock(&self.listeners);
        l.next += 1;
        let id = l.next;
        l.notice.push((id, Arc::from(cb)));
        ListenerId(id)
    }

    /// Unsubscribe a change or notice listener. Returns false when it was already gone.
    pub fn off(&self, id: ListenerId) -> bool {
        let mut l = lock(&self.listeners);
        let before = l.change.len() + l.notice.len();
        l.change.retain(|(i, _)| *i != id.0);
        l.notice.retain(|(i, _)| *i != id.0);
        before != l.change.len() + l.notice.len()
    }

    /// If profile.json changed on disk since Bridge last read or wrote it (a hand edit), adopt the edit instead of
    /// overwriting it. An unreadable edit is copied aside first, and Bridge keeps its in-memory copy.
    fn sync_from_disk(&self, st: &mut State, notices: &mut Vec<&'static str>) -> io::Result<()> {
        if !self.file.exists() || signature(&self.file) == st.disk_sig {
            return Ok(());
        }
        let Ok(loaded) = try_load(&self.file) else {
            return Ok(()); // locked right now; the write will retry or fail loudly
        };
        match loaded {
            Some((profile, dropped)) => {
                if dropped > 0 {
                    keep_copy(&self.file, &self.home, "dropped", false)?;
                    notices.push(NOTICE_DROPPED);
                }
                st.profile = profile;
            }
            None => {
                keep_copy(&self.file, &self.home, "edited", false)?;
                notices.push(NOTICE_EXTERNAL_DAMAGED);
            }
        }
        st.disk_sig = signature(&self.file);
        Ok(())
    }

    fn notify(&self, message: &str) {
        let listeners: Vec<NoticeFn> = lock(&self.listeners).notice.iter().map(|(_, cb)| cb.clone()).collect();
        for cb in listeners {
            if catch_unwind(AssertUnwindSafe(|| cb(message))).is_err() {
                eprintln!("[profile] notice listener failed");
            }
        }
    }
}
