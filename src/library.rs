use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct LocalTrack {
    pub path: PathBuf,
    pub file_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub track_no: u32,
    pub duration: Duration,
    pub size: u64,
    /// False when the file carried no usable title tag, so `title` is really the
    /// filename. Such a track still transfers, but shows up on the Zen under its
    /// filename, which is worth warning about before sending.
    pub tagged: bool,
}

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wma", "wav", "ogg", "flac", "m4a", "aac", "mp4", "mp2",
];

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Reads tags from `path`, falling back to the file stem for the title when a
/// file has no usable tags — the Zen shows the title field, so an empty one
/// would leave the track unidentifiable on the device.
///
/// Returns `None` only when the file cannot be read at all; an audio file with
/// no tags still yields a track, flagged with `tagged: false`.
pub fn read_file(path: &Path) -> Option<LocalTrack> {
    let size = std::fs::metadata(path).ok()?.len();
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let file_name = path.file_name()?.to_string_lossy().to_string();

    let untagged = |duration| {
        (
            stem.clone(),
            String::new(),
            String::new(),
            String::new(),
            0,
            duration,
            false,
        )
    };

    let (title, artist, album, genre, track_no, duration, tagged) =
        match lofty::read_from_path(path) {
            Ok(file) => {
                let duration = file.properties().duration();
                match file.primary_tag().or_else(|| file.first_tag()) {
                    Some(tag) => match tag.title() {
                        Some(title) => (
                            title.to_string(),
                            tag.artist().map(|t| t.to_string()).unwrap_or_default(),
                            tag.album().map(|t| t.to_string()).unwrap_or_default(),
                            tag.genre().map(|t| t.to_string()).unwrap_or_default(),
                            tag.track().unwrap_or(0),
                            duration,
                            true,
                        ),
                        None => untagged(duration),
                    },
                    None => untagged(duration),
                }
            }
            Err(_) => untagged(Duration::ZERO),
        };

    Some(LocalTrack {
        path: path.to_path_buf(),
        file_name,
        title,
        artist,
        album,
        genre,
        track_no,
        duration,
        size,
        tagged,
    })
}

pub fn scan(root: &Path) -> Vec<LocalTrack> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ftype) = entry.file_type() else {
                continue;
            };
            if ftype.is_dir() {
                stack.push(path);
            } else if ftype.is_file()
                && is_audio(&path)
                && let Some(track) = read_file(&path)
            {
                out.push(track);
            }
        }
    }

    sort_tracks(&mut out);
    out
}

pub fn sort_tracks(tracks: &mut [LocalTrack]) {
    tracks.sort_by(|a, b| {
        a.artist
            .to_lowercase()
            .cmp(&b.artist.to_lowercase())
            .then(a.album.to_lowercase().cmp(&b.album.to_lowercase()))
            .then(a.track_no.cmp(&b.track_no))
            .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("zenmanager-test-{name}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, rel: &str, contents: &[u8]) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scan_picks_up_audio_recursively_and_ignores_other_files() {
        let dir = TempDir::new("scan-filter");
        dir.write("a.mp3", b"not really audio");
        dir.write("notes.txt", b"hello");
        dir.write("cover.jpg", b"jpeg");
        dir.write("nested/deep/c.flac", b"not really audio");

        let found = scan(&dir.0);
        let names: Vec<_> = found.iter().map(|t| t.file_name.as_str()).collect();

        assert_eq!(names.len(), 2, "got {names:?}");
        assert!(names.contains(&"a.mp3"));
        assert!(names.contains(&"c.flac"));
    }

    #[test]
    fn unreadable_tags_fall_back_to_the_file_stem() {
        let dir = TempDir::new("scan-fallback");
        dir.write("My Song.mp3", b"not really audio");

        let found = scan(&dir.0);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "My Song");
        assert_eq!(found[0].artist, "");
        assert_eq!(found[0].track_no, 0);
    }

    #[test]
    fn extension_matching_is_case_insensitive() {
        let dir = TempDir::new("scan-case");
        dir.write("loud.MP3", b"not really audio");

        assert_eq!(scan(&dir.0).len(), 1);
    }

    #[test]
    fn tracks_are_ordered_by_artist_then_album_then_track_number() {
        let mut tracks = vec![
            LocalTrack {
                path: PathBuf::new(),
                file_name: "c".into(),
                title: "C".into(),
                artist: "Beta".into(),
                album: "X".into(),
                genre: String::new(),
                track_no: 1,
                duration: Duration::ZERO,
                size: 0,
                tagged: true,
            },
            LocalTrack {
                path: PathBuf::new(),
                file_name: "b".into(),
                title: "B".into(),
                artist: "Alpha".into(),
                album: "X".into(),
                genre: String::new(),
                track_no: 2,
                duration: Duration::ZERO,
                size: 0,
                tagged: true,
            },
            LocalTrack {
                path: PathBuf::new(),
                file_name: "a".into(),
                title: "A".into(),
                artist: "Alpha".into(),
                album: "X".into(),
                genre: String::new(),
                track_no: 1,
                duration: Duration::ZERO,
                size: 0,
                tagged: true,
            },
        ];

        sort_tracks(&mut tracks);

        let order: Vec<_> = tracks.iter().map(|t| t.file_name.as_str()).collect();
        assert_eq!(order, vec!["a", "b", "c"]);
    }
}
