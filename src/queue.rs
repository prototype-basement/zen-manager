use crate::library::{self, LocalTrack};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    Ready,
    /// Transferable, but the Zen will list it under its filename.
    NoTags,
    UnsupportedFormat,
    /// A real audio file, but not a format this particular device accepts.
    UnsupportedByDevice,
    Unreadable,
    AlreadyOnDevice,
    NotEnoughSpace,
}

impl Issue {
    /// Whether this entry is excluded from the transfer. `NoTags` is advisory:
    /// the file still plays on the Zen, it just shows an ugly title.
    pub fn blocks_transfer(&self) -> bool {
        !matches!(self, Issue::Ready | Issue::NoTags)
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            Issue::Ready => "✓",
            Issue::NoTags => "⚠",
            _ => "✕",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Issue::Ready => "",
            Issue::NoTags => "No tags — will show as the filename",
            Issue::UnsupportedFormat => "Not an audio file",
            Issue::UnsupportedByDevice => "The ZEN cannot play this format",
            Issue::Unreadable => "Could not read this file",
            Issue::AlreadyOnDevice => "Already on the ZEN",
            Issue::NotEnoughSpace => "Not enough space on the ZEN",
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueuedFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    /// `None` when the file could not be parsed, in which case `issue` explains why.
    pub track: Option<LocalTrack>,
    pub issue: Issue,
}

/// What the device contributes to evaluation. `None` means no device is
/// connected, so the duplicate and space checks cannot be made yet.
pub struct DeviceState {
    pub free_space: u64,
    pub track_names: HashSet<String>,
    /// Lowercase extensions the device reported as supported. Empty means it
    /// told us nothing, in which case the check is skipped rather than
    /// rejecting everything.
    pub supported_extensions: HashSet<String>,
}

fn entry_for(path: &Path) -> QueuedFile {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    if !library::is_audio(path) {
        return QueuedFile {
            path: path.to_path_buf(),
            name,
            size,
            track: None,
            issue: Issue::UnsupportedFormat,
        };
    }

    match library::read_file(path) {
        Some(track) => QueuedFile {
            path: path.to_path_buf(),
            name,
            size: track.size,
            track: Some(track),
            issue: Issue::Ready,
        },
        None => QueuedFile {
            path: path.to_path_buf(),
            name,
            size,
            track: None,
            issue: Issue::Unreadable,
        },
    }
}

/// Expands dropped paths into queue entries. A dropped folder is scanned
/// recursively; a dropped non-audio file is kept in the queue so the person can
/// see why it was rejected rather than having it silently vanish.
pub fn entries_for_paths(paths: &[PathBuf]) -> Vec<QueuedFile> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            for track in library::scan(path) {
                out.push(QueuedFile {
                    path: track.path.clone(),
                    name: track.file_name.clone(),
                    size: track.size,
                    track: Some(track),
                    issue: Issue::Ready,
                });
            }
        } else {
            out.push(entry_for(path));
        }
    }
    out
}

/// Recomputes every entry's issue. Must run whenever the queue or the device
/// state changes, since space and duplicate checks depend on both.
pub fn evaluate(files: &mut [QueuedFile], device: Option<&DeviceState>) {
    let mut committed = 0u64;

    for file in files.iter_mut() {
        // A file that could not be parsed keeps its original reason.
        if file.track.is_none() {
            continue;
        }

        let untagged = file.track.as_ref().is_some_and(|t| !t.tagged);

        let extension = Path::new(&file.name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();

        file.issue = match device {
            Some(device)
                if !device.supported_extensions.is_empty()
                    && !device.supported_extensions.contains(&extension) =>
            {
                Issue::UnsupportedByDevice
            }
            Some(device) if device.track_names.contains(&file.name.to_lowercase()) => {
                Issue::AlreadyOnDevice
            }
            Some(device) if committed + file.size > device.free_space => Issue::NotEnoughSpace,
            _ => {
                committed += file.size;
                if untagged { Issue::NoTags } else { Issue::Ready }
            }
        };
    }
}

pub fn transferable(files: &[QueuedFile]) -> Vec<usize> {
    files
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.issue.blocks_transfer())
        .map(|(i, _)| i)
        .collect()
}

pub fn total_size(files: &[QueuedFile]) -> u64 {
    files.iter().map(|f| f.size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn track(name: &str, size: u64, tagged: bool) -> LocalTrack {
        LocalTrack {
            path: PathBuf::from(name),
            file_name: name.to_string(),
            title: "T".into(),
            artist: "A".into(),
            album: "B".into(),
            genre: String::new(),
            track_no: 1,
            duration: Duration::ZERO,
            size,
            tagged,
        }
    }

    fn queued(name: &str, size: u64, tagged: bool) -> QueuedFile {
        QueuedFile {
            path: PathBuf::from(name),
            name: name.to_string(),
            size,
            track: Some(track(name, size, tagged)),
            issue: Issue::Ready,
        }
    }

    fn device(free: u64, names: &[&str]) -> DeviceState {
        DeviceState {
            free_space: free,
            track_names: names.iter().map(|n| n.to_lowercase()).collect(),
            supported_extensions: ["mp3", "wma", "wav"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    #[test]
    fn without_a_device_nothing_is_blocked() {
        let mut files = vec![queued("a.mp3", 100, true), queued("b.mp3", 100, true)];
        evaluate(&mut files, None);
        assert_eq!(transferable(&files).len(), 2);
    }

    #[test]
    fn untagged_files_are_warned_but_still_transfer() {
        let mut files = vec![queued("a.mp3", 10, false)];
        evaluate(&mut files, Some(&device(1000, &[])));
        assert_eq!(files[0].issue, Issue::NoTags);
        assert_eq!(transferable(&files).len(), 1);
    }

    #[test]
    fn files_already_on_the_device_are_blocked() {
        let mut files = vec![queued("a.mp3", 10, true), queued("b.mp3", 10, true)];
        evaluate(&mut files, Some(&device(1000, &["A.MP3"])));
        assert_eq!(files[0].issue, Issue::AlreadyOnDevice);
        assert_eq!(files[1].issue, Issue::Ready);
        assert_eq!(transferable(&files).len(), 1);
    }

    #[test]
    fn space_is_counted_cumulatively_across_the_queue() {
        let mut files = vec![
            queued("a.mp3", 60, true),
            queued("b.mp3", 60, true),
            queued("c.mp3", 10, true),
        ];
        evaluate(&mut files, Some(&device(100, &[])));

        assert_eq!(files[0].issue, Issue::Ready);
        // 60 + 60 exceeds 100, so the second is blocked...
        assert_eq!(files[1].issue, Issue::NotEnoughSpace);
        // ...but a smaller file after it still fits in what remains.
        assert_eq!(files[2].issue, Issue::Ready);
    }

    #[test]
    fn a_blocked_file_does_not_consume_space() {
        let mut files = vec![queued("dup.mp3", 90, true), queued("b.mp3", 90, true)];
        evaluate(&mut files, Some(&device(100, &["dup.mp3"])));

        assert_eq!(files[0].issue, Issue::AlreadyOnDevice);
        assert_eq!(files[1].issue, Issue::Ready);
    }

    #[test]
    fn formats_the_device_cannot_play_are_blocked() {
        // The Zen Micro reports only nine filetypes; FLAC is not one of them.
        let mut files = vec![queued("track.flac", 10, true), queued("track.mp3", 10, true)];
        evaluate(&mut files, Some(&device(1000, &[])));

        assert_eq!(files[0].issue, Issue::UnsupportedByDevice);
        assert_eq!(files[1].issue, Issue::Ready);
        assert_eq!(transferable(&files).len(), 1);
    }

    #[test]
    fn a_device_reporting_no_formats_blocks_nothing() {
        // Never reject everything just because the device stayed quiet.
        let mut files = vec![queued("track.flac", 10, true)];
        let mut state = device(1000, &[]);
        state.supported_extensions.clear();
        evaluate(&mut files, Some(&state));

        assert_eq!(files[0].issue, Issue::Ready);
    }

    #[test]
    fn an_unplayable_file_does_not_consume_space() {
        let mut files = vec![queued("big.flac", 90, true), queued("ok.mp3", 90, true)];
        evaluate(&mut files, Some(&device(100, &[])));

        assert_eq!(files[0].issue, Issue::UnsupportedByDevice);
        assert_eq!(files[1].issue, Issue::Ready);
    }

    #[test]
    fn unparsed_files_keep_their_original_reason() {
        let mut files = vec![QueuedFile {
            path: PathBuf::from("notes.txt"),
            name: "notes.txt".into(),
            size: 5,
            track: None,
            issue: Issue::UnsupportedFormat,
        }];
        evaluate(&mut files, Some(&device(1000, &[])));

        assert_eq!(files[0].issue, Issue::UnsupportedFormat);
        assert!(transferable(&files).is_empty());
    }
}
