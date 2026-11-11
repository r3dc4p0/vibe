//! Live external-change detection.
//!
//! Vibe records the modification time and length of every file it opens and
//! compares them before it writes (see [`crate::filesystem::changed_on_disk`]).
//! That catches a foreign change at the one moment it matters least: the user
//! has already done the work they are about to overwrite, and the text on
//! screen has been wrong for however long they were typing.
//!
//! This module watches instead of waiting to be asked. It keeps a platform
//! watcher (FSEvents, inotify, kqueue — `notify` picks) on the *directories*
//! that hold open files, and reports the paths that changed. Directories rather
//! than files, because the ordinary way to write a file is to write a temporary
//! one and rename it over the original: the file the editor opened is then
//! gone, and a watch on that file would fall silent exactly when it matters. A
//! directory watch sees the rename land.
//!
//! Nothing here decides what a change *means*. It says which paths moved; the
//! application compares the metadata and either follows the file or flags the
//! buffer. Keeping the judgement out of this module is also what keeps it
//! testable without a platform watcher.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tracing::{debug, warn};

/// How long a path must stay quiet before a change to it is acted on.
///
/// One save arrives as a burst — truncate, write, write, close — and reading
/// the file after the first of those would show a half-written buffer. Waiting
/// for the burst to end costs a fraction of a second and makes the read land on
/// a whole file. It also collapses the several events a single save produces
/// into one reload.
pub const SETTLE: Duration = Duration::from_millis(200);

/// A filesystem watcher over the directories that hold open files.
pub struct DiskWatcher {
    inner: RecommendedWatcher,
    /// Watched directory → how many open files need it watched.
    ///
    /// Ref-counted because a project keeps most of its files in one directory:
    /// closing one of forty buffers must not stop the watch that the other
    /// thirty-nine still depend on.
    dirs: HashMap<PathBuf, usize>,
    events: Receiver<notify::Result<Event>>,
}

impl DiskWatcher {
    /// Start a watcher.
    ///
    /// Fails only when the platform has no usable backend or refuses the
    /// watcher itself; a refused *watch* is reported and skipped, since one
    /// unwatchable directory is not a reason to give up on the others.
    pub fn new() -> notify::Result<Self> {
        let (tx, events) = channel();
        let inner = notify::recommended_watcher(move |res: notify::Result<Event>| {
            // The receiver lives in the `App`; once that is gone there is
            // nothing to tell, and a failed send is how this thread finds out.
            let _ = tx.send(res);
        })?;
        Ok(Self {
            inner,
            dirs: HashMap::new(),
            events,
        })
    }

    /// Watch `dir`, or add one to the reasons it is already watched.
    pub fn watch_dir(&mut self, dir: &Path) {
        let count = self.dirs.entry(dir.to_path_buf()).or_insert(0);
        *count += 1;
        if *count > 1 {
            return;
        }
        match self.inner.watch(dir, RecursiveMode::NonRecursive) {
            Ok(()) => debug!("watching {}", dir.display()),
            Err(e) => {
                // Not fatal. The save-time check still stands, so the worst
                // case is the old behaviour: the change is found at save.
                warn!("cannot watch {}: {e}", dir.display());
                self.dirs.remove(dir);
            }
        }
    }

    /// Drop one reason to watch `dir`, unwatching it when it was the last.
    pub fn unwatch_dir(&mut self, dir: &Path) {
        let Some(count) = self.dirs.get_mut(dir) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count > 0 {
            return;
        }
        self.dirs.remove(dir);
        let _ = self.inner.unwatch(dir);
    }

    /// The next batch of changed paths, if any have arrived.
    ///
    /// Errors are logged and skipped rather than returned: a backend that
    /// reports a problem with one directory should not stop the editor.
    pub fn try_recv(&self) -> Option<Vec<PathBuf>> {
        loop {
            match self.events.try_recv() {
                Ok(Ok(event)) => {
                    if let Some(paths) = worth_reading(&event) {
                        return Some(paths);
                    }
                }
                Ok(Err(e)) => warn!("filesystem watch error: {e}"),
                Err(_) => return None,
            }
        }
    }
}

/// The paths of `event` that can mean the file's contents changed.
///
/// Opening and reading a file is not news, and on some backends every access
/// arrives here — a full drain of the channel for each read would cost more
/// than the watch saves. A close-after-write is news, because that is the last
/// thing a writer does.
fn worth_reading(event: &Event) -> Option<Vec<PathBuf>> {
    let interesting = matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Access(notify::event::AccessKind::Close(
                notify::event::AccessMode::Write
            ))
    );
    if !interesting || event.paths.is_empty() {
        return None;
    }
    Some(event.paths.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind};
    use notify::EventKind;

    fn event(kind: EventKind) -> Event {
        Event::new(kind).add_path(PathBuf::from("/tmp/x"))
    }

    #[test]
    fn writes_and_renames_are_news() {
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::To)),
            EventKind::Remove(notify::event::RemoveKind::File),
        ] {
            assert!(worth_reading(&event(kind)).is_some(), "{kind:?}");
        }
    }

    #[test]
    fn a_close_after_a_write_is_news_but_a_read_is_not() {
        let closed_for_write = event(EventKind::Access(AccessKind::Close(AccessMode::Write)));
        assert!(worth_reading(&closed_for_write).is_some());

        let read = event(EventKind::Access(AccessKind::Close(AccessMode::Read)));
        assert!(worth_reading(&read).is_none(), "reading is not a change");
        let opened = event(EventKind::Access(AccessKind::Open(AccessMode::Read)));
        assert!(worth_reading(&opened).is_none());
    }

    #[test]
    fn an_event_without_paths_is_not_news() {
        assert!(worth_reading(&Event::new(EventKind::Modify(ModifyKind::Any))).is_none());
    }
}
