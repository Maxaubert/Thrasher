//! The clip folder: listing, naming, rename, delete with undo and crash recovery.
//!
//! A clip named `clip_007` is made of:
//! - `clip_007.wav`: the mic track (or the only track)
//! - `clip_007.system.wav`: system audio, when recorded
//! - `clip_007.json`: the sidecar

use crate::sidecar::{RecordingState, Sidecar, TrackKind};
use crate::wav;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;

const TRASH: &str = ".trash";
/// Files and folders that belong to a clip besides its mic track. `.voice` holds the
/// voice sample and generated words (see `voice`).
const EXTRA_SUFFIXES: [&str; 4] = [".system.wav", ".mix.wav", ".json", ".voice"];

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("a clip named '{0}' already exists")]
    Exists(String),
    #[error("no clip named '{0}'")]
    Missing(String),
    #[error("'{0}' is not a valid file name")]
    BadName(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub name: String,
    pub path: PathBuf,
    pub duration: f64,
    pub has_system: bool,
    pub created: SystemTime,
}

/// A deleted clip that can be put back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deleted {
    pub name: String,
    trash_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Library {
    dir: PathBuf,
}

pub fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n == name
        && !name.ends_with('.')
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| "<>:\"/\\|?*".contains(c) || c.is_control())
}

impl Library {
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn track_path(&self, name: &str, kind: TrackKind) -> PathBuf {
        self.dir.join(format!("{name}{}.wav", kind.file_suffix()))
    }

    pub fn sidecar_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    fn files_of(&self, name: &str) -> Vec<PathBuf> {
        std::iter::once(format!("{name}.wav"))
            .chain(EXTRA_SUFFIXES.iter().map(|s| format!("{name}{s}")))
            .map(|f| self.dir.join(f))
            .filter(|p| p.exists())
            .collect()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.track_path(name, TrackKind::Mic).exists()
    }

    /// All clip names, in no particular order. Cheap: no headers are read.
    pub fn names(&self) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let file = entry.file_name();
            let Some(file) = file.to_str() else { continue };
            let lower = file.to_ascii_lowercase();
            if !lower.ends_with(".wav")
                || lower.ends_with(".system.wav")
                || lower.ends_with(".mix.wav")
            {
                continue;
            }
            out.push(file[..file.len() - 4].to_string());
        }
        Ok(out)
    }

    /// All clips, oldest first (the newest is last, like the old recorder).
    pub fn list(&self) -> io::Result<Vec<Clip>> {
        let mut clips: Vec<Clip> = self
            .names()?
            .into_iter()
            .filter_map(|name| self.clip(&name).ok())
            .collect();
        clips.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.name.cmp(&b.name)));
        Ok(clips)
    }

    pub fn clip(&self, name: &str) -> io::Result<Clip> {
        let path = self.track_path(name, TrackKind::Mic);
        let meta = std::fs::metadata(&path)?;
        let duration = wav::read_info(&path)
            .map(|i| i.duration_secs())
            .unwrap_or(0.0);
        Ok(Clip {
            name: name.to_string(),
            duration,
            has_system: self.track_path(name, TrackKind::System).exists(),
            created: meta.created().or_else(|_| meta.modified())?,
            path,
        })
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), LibraryError> {
        if !valid_name(to) {
            return Err(LibraryError::BadName(to.into()));
        }
        if from == to {
            return Ok(());
        }
        if !self.exists(from) {
            return Err(LibraryError::Missing(from.into()));
        }
        // Case-only renames are allowed on Windows; anything else must be free.
        if self.exists(to) && !from.eq_ignore_ascii_case(to) {
            return Err(LibraryError::Exists(to.into()));
        }
        for path in self.files_of(from) {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let target = self.dir.join(format!("{to}{}", &file[from.len()..]));
            std::fs::rename(&path, &target)?;
        }
        let sc = self.sidecar_path(to);
        if let Ok(mut s) = Sidecar::load(&sc) {
            s.file = format!("{to}.wav");
            for t in &mut s.thrasher.tracks {
                t.file = format!("{to}{}.wav", t.kind.file_suffix());
            }
            s.save(&sc)?;
        }
        Ok(())
    }

    /// Moves the clip's files into the hidden trash folder.
    pub fn delete(&self, name: &str) -> Result<Deleted, LibraryError> {
        let files = self.files_of(name);
        if files.is_empty() {
            return Err(LibraryError::Missing(name.into()));
        }
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let trash_dir = self.dir.join(TRASH).join(format!("{stamp}"));
        std::fs::create_dir_all(&trash_dir)?;
        hide(&self.dir.join(TRASH));
        for f in files {
            std::fs::rename(&f, trash_dir.join(f.file_name().unwrap()))?;
        }
        Ok(Deleted {
            name: name.into(),
            trash_dir,
        })
    }

    /// Puts a deleted clip back. Fails if a clip with the same name was made since.
    pub fn undo_delete(&self, d: &Deleted) -> Result<(), LibraryError> {
        if self.exists(&d.name) {
            return Err(LibraryError::Exists(d.name.clone()));
        }
        for entry in std::fs::read_dir(&d.trash_dir)? {
            let entry = entry?;
            std::fs::rename(entry.path(), self.dir.join(entry.file_name()))?;
        }
        std::fs::remove_dir(&d.trash_dir)?;
        Ok(())
    }

    /// Finds clips whose sidecar says they were still recording (the app died) and
    /// makes their files readable again. Returns the names that were repaired.
    pub fn recover(&self) -> io::Result<Vec<String>> {
        let mut fixed = Vec::new();
        for name in self.names()? {
            let sc_path = self.sidecar_path(&name);
            let Ok(mut sc) = Sidecar::load(&sc_path) else {
                continue;
            };
            if sc.thrasher.state != RecordingState::Recording {
                continue;
            }
            for t in &sc.thrasher.tracks {
                let p = self.dir.join(&t.file);
                if p.exists() {
                    let info = wav::repair(&p)?;
                    if t.kind == TrackKind::Mic {
                        sc.duration = info.duration_secs();
                    }
                }
            }
            sc.thrasher.state = RecordingState::Complete;
            sc.save(&sc_path)?;
            fixed.push(name);
        }
        Ok(fixed)
    }
}

#[cfg(windows)]
fn hide(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // FILE_ATTRIBUTE_HIDDEN
    unsafe extern "system" {
        fn SetFileAttributesW(name: *const u16, attrs: u32) -> i32;
    }
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that lives for the call.
    unsafe {
        SetFileAttributesW(wide.as_ptr(), 0x2);
    }
}

#[cfg(not(windows))]
fn hide(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::{Meta, Track};
    use crate::wav::WavWriter;

    fn make(lib: &Library, name: &str, frames: usize, system: bool) {
        let mut w = WavWriter::create(&lib.track_path(name, TrackKind::Mic), 1000, 1).unwrap();
        w.write(&vec![0.1; frames]).unwrap();
        w.finalize().unwrap();
        if system {
            let p = lib.track_path(name, TrackKind::System);
            WavWriter::create(&p, 1000, 2).unwrap().finalize().unwrap();
        }
        Sidecar {
            file: format!("{name}.wav"),
            thrasher: Meta {
                tracks: vec![Track {
                    kind: TrackKind::Mic,
                    file: format!("{name}.wav"),
                    sample_rate: 1000,
                    channels: 1,
                    device: None,
                }],
                ..Default::default()
            },
            ..Default::default()
        }
        .save(&lib.sidecar_path(name))
        .unwrap();
    }

    #[test]
    fn list_rename_delete_undo() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        make(&lib, "clip_001", 500, false);
        std::thread::sleep(std::time::Duration::from_millis(20));
        make(&lib, "clip_002", 2000, true);

        let clips = lib.list().unwrap();
        assert_eq!(
            clips.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["clip_001", "clip_002"]
        );
        assert!((clips[1].duration - 2.0).abs() < 1e-9);
        assert!(clips[1].has_system);

        lib.rename("clip_002", "intro").unwrap();
        assert!(lib.track_path("intro", TrackKind::System).exists());
        let sc = Sidecar::load(&lib.sidecar_path("intro")).unwrap();
        assert_eq!(sc.file, "intro.wav");
        assert_eq!(sc.thrasher.tracks[0].file, "intro.wav");
        assert!(matches!(
            lib.rename("intro", "clip_001"),
            Err(LibraryError::Exists(_))
        ));
        assert!(matches!(
            lib.rename("intro", "a/b"),
            Err(LibraryError::BadName(_))
        ));

        let d = lib.delete("intro").unwrap();
        assert_eq!(lib.names().unwrap(), ["clip_001"]);
        lib.undo_delete(&d).unwrap();
        assert!(lib.exists("intro"));
        assert!(lib.sidecar_path("intro").exists());
    }

    #[test]
    fn recovers_unfinished_recording() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let mut w = WavWriter::create(&lib.track_path("c", TrackKind::Mic), 1000, 1).unwrap();
        w.write(&[0.2; 1500]).unwrap();
        w.sync().unwrap();
        std::mem::forget(w);
        let mut sc = Sidecar {
            file: "c.wav".into(),
            ..Default::default()
        };
        sc.thrasher.state = RecordingState::Recording;
        sc.thrasher.tracks.push(Track {
            kind: TrackKind::Mic,
            file: "c.wav".into(),
            sample_rate: 1000,
            channels: 1,
            device: None,
        });
        sc.save(&lib.sidecar_path("c")).unwrap();

        assert_eq!(lib.recover().unwrap(), ["c"]);
        let sc = Sidecar::load(&lib.sidecar_path("c")).unwrap();
        assert_eq!(sc.thrasher.state, RecordingState::Complete);
        assert!((sc.duration - 1.5).abs() < 1e-9);
        assert!(lib.recover().unwrap().is_empty());
    }

    #[test]
    fn names_ignore_extra_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        make(&lib, "a", 10, true);
        assert_eq!(lib.names().unwrap(), ["a"]);
    }
}
