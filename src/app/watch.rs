//! The application's half of external-change detection.
//!
//! [`DiskWatcher`] reports paths that moved; this module decides what those
//! moves mean for the buffers on screen. The split matters because the
//! decisions are the part worth testing, and they can be tested by calling them
//! directly, with no watcher, no threads and no dependence on how the host
//! filesystem reports a write.
//!
//! Two outcomes, chosen by whether the buffer has anything of its own to lose:
//!
//! * a clean buffer follows the file. Somebody else's write is the new truth,
//!   and reloading costs the user nothing — that is what makes the detection
//!   *live* rather than a question asked at save time;
//! * a modified buffer is flagged, never reloaded. The edits on screen are the
//!   only copy of themselves, and a reload would delete them without a
//!   keystroke. The status bar carries `⚠ changed on disk` from then on, and
//!   the existing save-time confirmation is what offers the choice.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{App, ASYNC_OPEN_THRESHOLD};
use crate::filesystem;
use crate::watch::{DiskWatcher, SETTLE};

/// Batches of events read from the watcher per frame.
///
/// A directory-wide operation (`git checkout`, a build writing into the tree)
/// can produce thousands of events. Reading them all in one frame would stall
/// the paint for as long as it takes to stat each path; the remainder is still
/// queued and is picked up on the next frame.
const MAX_BATCHES_PER_FRAME: usize = 64;

impl App {
    /// Start watching every open file for changes made by other programs.
    ///
    /// Called by [`App::new`]. The headless constructor leaves the watcher off
    /// so the integration tests start no threads and do not depend on the
    /// platform's event backend; a test that wants the real thing calls this.
    pub fn enable_disk_watch(&mut self) {
        if self.watcher.is_some() || !self.config.files.watch_external_changes {
            return;
        }
        match DiskWatcher::new() {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(e) => {
                // Degrade to the behaviour that existed before the watcher:
                // a change is found when the user tries to save over it.
                self.warn(format!("cannot watch files for changes: {e}"));
                return;
            }
        }
        let open: Vec<(usize, PathBuf)> = self
            .buffers
            .iter()
            .filter_map(|doc| doc.path().map(|path| (doc.id(), path.to_path_buf())))
            .collect();
        for (id, path) in open {
            self.track_path(id, &path);
        }
    }

    /// Watch the file behind document `id`.
    ///
    /// Called wherever a document acquires a path: on open, on save under a new
    /// name, and once for everything already open when the watcher starts.
    pub(crate) fn track_path(&mut self, id: usize, path: &Path) {
        if self.watcher.is_none() {
            return;
        }
        let Some(file) = canonical(path) else {
            return;
        };
        // One document per file: `open_path` refuses to open a file twice, so
        // the first claim is the only one.
        if self.watched.contains_key(&file) {
            return;
        }
        let Some(dir) = file.parent().map(Path::to_path_buf) else {
            return;
        };
        if let Some(watcher) = self.watcher.as_mut() {
            watcher.watch_dir(&dir);
        }
        self.watched.insert(file, id);
    }

    /// Stop watching whatever document `id` had open.
    pub(crate) fn untrack_path(&mut self, id: usize) {
        let file = self
            .watched
            .iter()
            .find(|(_, owner)| **owner == id)
            .map(|(file, _)| file.clone());
        let Some(file) = file else {
            return;
        };
        self.watched.remove(&file);
        self.disk_pending.remove(&id);
        if let (Some(dir), Some(watcher)) = (file.parent(), self.watcher.as_mut()) {
            watcher.unwatch_dir(dir);
        }
    }

    /// Drain the watcher and act on the files that have stopped changing.
    ///
    /// Run once per frame from [`App::poll_jobs`], so a change is on screen
    /// within a frame or two of the write that made it.
    pub(crate) fn poll_disk_events(&mut self) {
        if self.watcher.is_none() {
            return;
        }
        // The paths are collected before any of the bookkeeping: the watcher
        // borrows `self`, and recording a pending change needs `&mut self`.
        let mut paths = Vec::new();
        if let Some(watcher) = self.watcher.as_ref() {
            for _ in 0..MAX_BATCHES_PER_FRAME {
                match watcher.try_recv() {
                    Some(batch) => paths.extend(batch),
                    None => break,
                }
            }
        }
        let now = Instant::now();
        for path in paths {
            let Some(key) = canonical(&path) else {
                continue;
            };
            if let Some(&id) = self.watched.get(&key) {
                // The stamp is replaced, not kept: the settle window measures
                // quiet, so every new event restarts it.
                self.disk_pending.insert(id, now);
            }
        }
        let ready: Vec<usize> = self
            .disk_pending
            .iter()
            .filter(|(_, at)| now.duration_since(**at) >= SETTLE)
            .map(|(id, _)| *id)
            .collect();
        for id in ready {
            self.disk_pending.remove(&id);
            self.apply_disk_change(id);
        }
    }

    /// Reconcile every open buffer with its file, watcher or no watcher.
    ///
    /// The watcher normally drives this, one document at a time and only when
    /// the filesystem says something moved. This is the blunt version — stat
    /// every open file and act on the ones that differ — which is what a
    /// filesystem that cannot be watched (a network mount, a container with no
    /// inotify) still gets, and what the tests drive so they do not depend on
    /// the timing of a platform event.
    pub fn check_files_on_disk(&mut self) {
        let ids: Vec<usize> = self
            .buffers
            .iter()
            .filter(|doc| doc.path().is_some())
            .map(|doc| doc.id())
            .collect();
        for id in ids {
            self.apply_disk_change(id);
        }
    }

    /// Reconcile one document with the file on disk.
    fn apply_disk_change(&mut self, id: usize) {
        let Some(idx) = self.buffers.iter().position(|doc| doc.id() == id) else {
            return;
        };
        let Some(path) = self.buffers[idx].path().map(Path::to_path_buf) else {
            return;
        };
        let name = self.buffers[idx].display_name();

        let Some((_mtime, len)) = filesystem::metadata_of(&path) else {
            // The file is gone. The text is not: a buffer keeps what it has
            // until the user decides, and the flag explains why saving will
            // recreate the file.
            self.flag_external_change(
                idx,
                format!("{name} was deleted on disk; the buffer is kept"),
            );
            return;
        };

        let (mtime, disk_len) = (self.buffers[idx].disk_mtime(), self.buffers[idx].disk_len());
        if !filesystem::changed_on_disk(&path, mtime, disk_len) {
            // Our own save, or a write that left the bytes as they were.
            return;
        }

        if self.buffers[idx].is_modified() {
            self.flag_external_change(idx, format!("{name} changed on disk; your edits are kept"));
            return;
        }

        if len > ASYNC_OPEN_THRESHOLD {
            // Reading this on the UI thread would freeze the editor for the
            // same reason opening it does. Flag it and let the user choose the
            // moment.
            self.flag_external_change(
                idx,
                format!("{name} changed on disk; Ctrl+Shift+R reloads it"),
            );
            return;
        }

        match self.load_into(idx, &path) {
            Ok(()) => self.success(format!("Reloaded {name} (changed on disk)")),
            Err(e) => self.error(e.user_message()),
        }
    }

    /// Mark a buffer as changed underneath the user, and say so once.
    ///
    /// The status bar carries the flag from then on; repeating the message on
    /// every event would bury the one the user has to read.
    fn flag_external_change(&mut self, idx: usize, message: String) {
        let already = self.buffers[idx].externally_modified();
        self.buffers[idx].set_externally_modified(true);
        if !already {
            self.warn(message);
        }
    }
}

/// Canonical form of `path`, with a name for a file that is not there yet.
///
/// Both sides of the comparison have to agree: the watcher reports resolved
/// paths, and a buffer's path may be relative, contain `..`, or arrive through
/// a symlink. When the file itself cannot be resolved — it was deleted, or it
/// has not been written yet — the parent directory can, and the name is joined
/// to it, which is the same path the watch will report if the file appears.
fn canonical(path: &Path) -> Option<PathBuf> {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return Some(resolved);
    }
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let dir = std::fs::canonicalize(parent).ok()?;
    path.file_name().map(|name| dir.join(name))
}
