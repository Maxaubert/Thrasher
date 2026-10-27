//! Projects: folders of clips inside the clip folder, such as "School" or a work site.
//!
//! The clip folder itself is the first project, Inbox, so clips made before projects
//! existed never move. Every other project is a subfolder marked with a
//! `.thrasher-project` file, so other folders the user keeps there are not shown.

use crate::export::ExportOptions;
use crate::library::{Library, LibraryError, valid_name};
use std::io;
use std::path::{Path, PathBuf};

/// The name shown for the clip folder itself.
pub const INBOX: &str = "Inbox";
const MARKER: &str = ".thrasher-project";

/// The folder of project `name` under `root`; Inbox (or an empty name) is `root`.
pub fn dir(root: &Path, name: &str) -> PathBuf {
    if name.is_empty() || name == INBOX {
        root.to_path_buf()
    } else {
        root.join(name)
    }
}

/// The projects under `root` besides Inbox, sorted by name, ignoring case.
pub fn list(root: &Path) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || !entry.path().join(MARKER).exists() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            out.push(name.to_string());
        }
    }
    out.sort_by_key(|n| n.to_lowercase());
    Ok(out)
}

fn check_free(root: &Path, name: &str) -> Result<(), LibraryError> {
    if !valid_name(name) || name.eq_ignore_ascii_case(INBOX) {
        return Err(LibraryError::BadName(name.into()));
    }
    if root.join(name).exists() {
        return Err(LibraryError::Exists(name.into()));
    }
    Ok(())
}

/// Makes a new, empty project.
pub fn create(root: &Path, name: &str) -> Result<PathBuf, LibraryError> {
    check_free(root, name)?;
    let dir = root.join(name);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(MARKER), "")?;
    Ok(dir)
}

/// Renames a project's folder. Inbox cannot be renamed.
pub fn rename(root: &Path, from: &str, to: &str) -> Result<(), LibraryError> {
    if from == INBOX || !root.join(from).join(MARKER).exists() {
        return Err(LibraryError::Missing(from.into()));
    }
    if from == to {
        return Ok(());
    }
    // Case-only renames are allowed on Windows.
    if !from.eq_ignore_ascii_case(to) {
        check_free(root, to)?;
    } else if !valid_name(to) {
        return Err(LibraryError::BadName(to.into()));
    }
    std::fs::rename(root.join(from), root.join(to))?;
    Ok(())
}

/// Removes a project that has no clips left. Inbox cannot be removed.
pub fn remove(root: &Path, name: &str) -> Result<(), LibraryError> {
    let dir = root.join(name);
    if name == INBOX || !dir.join(MARKER).exists() {
        return Err(LibraryError::Missing(name.into()));
    }
    if !Library::open(&dir)?.names()?.is_empty() {
        return Err(LibraryError::Io(io::Error::other(
            "the project still has clips",
        )));
    }
    std::fs::remove_file(dir.join(MARKER))?;
    // Fails, leaving the folder, if the user keeps other files in it.
    std::fs::remove_dir(&dir).or_else(|_| std::fs::write(dir.join(MARKER), ""))?;
    Ok(())
}

/// Exports every clip of a project into a new folder named after it inside `parent`,
/// one folder per clip (see `export::export_project`). Returns the folder.
pub fn export_all(
    library: &Library,
    project: &str,
    options: ExportOptions,
    parent: &Path,
) -> io::Result<PathBuf> {
    export_all_progress(library, project, options, parent, &|_| {})
}

/// Like [`export_all`], reporting its progress from 0 to 1.
pub fn export_all_progress(
    library: &Library,
    project: &str,
    options: ExportOptions,
    parent: &Path,
    progress: crate::export::Progress,
) -> io::Result<PathBuf> {
    let mut out = parent.join(project);
    let mut n = 2;
    while out.exists() {
        out = parent.join(format!("{project} ({n})"));
        n += 1;
    }
    std::fs::create_dir_all(&out)?;
    let clips = library.list()?;
    let n = clips.len().max(1) as f32;
    for (i, clip) in clips.iter().enumerate() {
        crate::export::export_project_progress(library, &clip.name, options, &out, &|f| {
            progress((i as f32 + f) / n)
        })?;
    }
    progress(1.0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{Format, export_buffer};
    use crate::player::Buffer;

    fn clip(lib: &Library, name: &str) {
        let b = Buffer {
            samples: vec![0.1; 4800],
            sample_rate: 48_000,
            channels: 1,
        };
        let o = ExportOptions {
            format: Format::Wav,
            bits: 16,
            clean_voice: false,
        };
        export_buffer(&b, &lib.track_path(name, crate::sidecar::TrackKind::Mic), o).unwrap();
    }

    #[test]
    fn projects_are_marked_subfolders() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("exports")).unwrap();
        create(root.path(), "School").unwrap();
        create(root.path(), "at work").unwrap();
        assert_eq!(list(root.path()).unwrap(), ["at work", "School"]);
        assert!(create(root.path(), "School").is_err());
        assert!(create(root.path(), "Inbox").is_err());
        assert!(create(root.path(), "a/b").is_err());
        assert_eq!(dir(root.path(), INBOX), root.path());
        assert_eq!(dir(root.path(), "School"), root.path().join("School"));
    }

    #[test]
    fn rename_and_remove_only_empty_projects() {
        let root = tempfile::tempdir().unwrap();
        create(root.path(), "School").unwrap();
        rename(root.path(), "School", "Uni").unwrap();
        assert_eq!(list(root.path()).unwrap(), ["Uni"]);
        let lib = Library::open(dir(root.path(), "Uni")).unwrap();
        clip(&lib, "lecture");
        assert!(remove(root.path(), "Uni").is_err());
        std::fs::remove_file(lib.track_path("lecture", crate::sidecar::TrackKind::Mic)).unwrap();
        remove(root.path(), "Uni").unwrap();
        assert!(list(root.path()).unwrap().is_empty());
        assert!(remove(root.path(), INBOX).is_err());
    }

    #[test]
    fn clips_move_between_projects() {
        let root = tempfile::tempdir().unwrap();
        let inbox = Library::open(root.path()).unwrap();
        let school = Library::open(create(root.path(), "School").unwrap()).unwrap();
        clip(&inbox, "talk");
        clip(&school, "talk");
        let moved = inbox.move_to("talk", &school).unwrap();
        assert_eq!(moved, "talk (2)");
        assert!(!inbox.exists("talk"));
        assert!(school.exists("talk") && school.exists("talk (2)"));
    }
}
