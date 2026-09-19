use crate::mtp::{DeviceTrack, UNKNOWN_ALBUM};

pub const UNKNOWN_ARTIST: &str = "unknown artist";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Artist,
    Album,
    Track,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheckState {
    Unchecked,
    Partial,
    Checked,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodePath {
    Artist(usize),
    Album(usize, usize),
    Track(usize, usize, usize),
}

#[derive(Debug)]
pub struct TrackNode {
    pub id: u32,
    pub title: String,
    pub size: u64,
    pub checked: bool,
}

#[derive(Debug)]
pub struct AlbumNode {
    pub name: String,
    pub expanded: bool,
    pub tracks: Vec<TrackNode>,
}

#[derive(Debug)]
pub struct ArtistNode {
    pub name: String,
    pub expanded: bool,
    pub albums: Vec<AlbumNode>,
}

#[derive(Debug, Default)]
pub struct MusicTree {
    pub artists: Vec<ArtistNode>,
}

/// One visible line of the flattened tree.
pub struct Row {
    pub kind: NodeKind,
    pub depth: i32,
    pub label: String,
    /// Zero for a track row, which shows only its size.
    pub track_count: i32,
    pub size: u64,
    pub check: CheckState,
    pub expanded: bool,
}

fn combine(states: impl Iterator<Item = CheckState>) -> CheckState {
    let mut any_checked = false;
    let mut any_unchecked = false;

    for state in states {
        match state {
            CheckState::Partial => return CheckState::Partial,
            CheckState::Checked => any_checked = true,
            CheckState::Unchecked => any_unchecked = true,
        }
        if any_checked && any_unchecked {
            return CheckState::Partial;
        }
    }

    if any_checked {
        CheckState::Checked
    } else {
        CheckState::Unchecked
    }
}

impl AlbumNode {
    fn check_state(&self) -> CheckState {
        combine(self.tracks.iter().map(|t| {
            if t.checked {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            }
        }))
    }

    fn size(&self) -> u64 {
        self.tracks.iter().map(|t| t.size).sum()
    }
}

impl ArtistNode {
    fn check_state(&self) -> CheckState {
        combine(self.albums.iter().map(|a| a.check_state()))
    }

    fn track_count(&self) -> usize {
        self.albums.iter().map(|a| a.tracks.len()).sum()
    }

    fn size(&self) -> u64 {
        self.albums.iter().map(|a| a.size()).sum()
    }
}

impl MusicTree {
    /// Groups tracks by artist then album. Tracks whose tags are missing are
    /// filed under explicit placeholder names rather than dropped, so nothing
    /// on the device is invisible in the tree.
    pub fn build(tracks: &[DeviceTrack]) -> Self {
        let mut artists: Vec<ArtistNode> = Vec::new();

        for track in tracks {
            let artist_name = if track.artist.trim().is_empty() {
                UNKNOWN_ARTIST
            } else {
                track.artist.trim()
            };
            let album_name = if track.album.trim().is_empty() {
                UNKNOWN_ALBUM
            } else {
                track.album.trim()
            };
            let title = if track.title.trim().is_empty() {
                track.name.clone()
            } else {
                track.title.trim().to_string()
            };

            let artist = match artists
                .iter_mut()
                .position(|a| a.name.eq_ignore_ascii_case(artist_name))
            {
                Some(index) => &mut artists[index],
                None => {
                    artists.push(ArtistNode {
                        name: artist_name.to_string(),
                        expanded: false,
                        albums: Vec::new(),
                    });
                    artists.last_mut().expect("just pushed")
                }
            };

            let album = match artist
                .albums
                .iter_mut()
                .position(|a| a.name.eq_ignore_ascii_case(album_name))
            {
                Some(index) => &mut artist.albums[index],
                None => {
                    artist.albums.push(AlbumNode {
                        name: album_name.to_string(),
                        expanded: false,
                        tracks: Vec::new(),
                    });
                    artist.albums.last_mut().expect("just pushed")
                }
            };

            album.tracks.push(TrackNode {
                id: track.id,
                title,
                size: track.size,
                checked: false,
            });
        }

        artists.sort_by_key(|a| a.name.to_lowercase());
        for artist in &mut artists {
            artist.albums.sort_by_key(|a| a.name.to_lowercase());
            for album in &mut artist.albums {
                album.tracks.sort_by_key(|t| t.title.to_lowercase());
            }
        }

        Self { artists }
    }

    /// Flattens the visible rows, returning the path each row maps back to so
    /// the UI can address nodes by row index.
    pub fn rows(&self) -> (Vec<Row>, Vec<NodePath>) {
        let mut rows = Vec::new();
        let mut paths = Vec::new();

        for (ai, artist) in self.artists.iter().enumerate() {
            let count = artist.track_count();
            rows.push(Row {
                kind: NodeKind::Artist,
                depth: 0,
                label: artist.name.clone(),
                track_count: count as i32,
                size: artist.size(),
                check: artist.check_state(),
                expanded: artist.expanded,
            });
            paths.push(NodePath::Artist(ai));

            if !artist.expanded {
                continue;
            }

            for (bi, album) in artist.albums.iter().enumerate() {
                let count = album.tracks.len();
                rows.push(Row {
                    kind: NodeKind::Album,
                    depth: 1,
                    label: album.name.clone(),
                    track_count: count as i32,
                    size: album.size(),
                    check: album.check_state(),
                    expanded: album.expanded,
                });
                paths.push(NodePath::Album(ai, bi));

                if !album.expanded {
                    continue;
                }

                for (ti, track) in album.tracks.iter().enumerate() {
                    rows.push(Row {
                        kind: NodeKind::Track,
                        depth: 2,
                        label: track.title.clone(),
                        track_count: 0,
                        size: track.size,
                        check: if track.checked {
                            CheckState::Checked
                        } else {
                            CheckState::Unchecked
                        },
                        expanded: false,
                    });
                    paths.push(NodePath::Track(ai, bi, ti));
                }
            }
        }

        (rows, paths)
    }

    pub fn toggle_expanded(&mut self, path: NodePath) {
        match path {
            NodePath::Artist(ai) => {
                if let Some(artist) = self.artists.get_mut(ai) {
                    artist.expanded = !artist.expanded;
                }
            }
            NodePath::Album(ai, bi) => {
                if let Some(album) = self
                    .artists
                    .get_mut(ai)
                    .and_then(|a| a.albums.get_mut(bi))
                {
                    album.expanded = !album.expanded;
                }
            }
            NodePath::Track(..) => {}
        }
    }

    /// Checking a node applies to everything beneath it, which is the whole
    /// point of the tree: select an artist, delete the lot.
    pub fn set_checked(&mut self, path: NodePath, checked: bool) {
        match path {
            NodePath::Artist(ai) => {
                if let Some(artist) = self.artists.get_mut(ai) {
                    for album in &mut artist.albums {
                        for track in &mut album.tracks {
                            track.checked = checked;
                        }
                    }
                }
            }
            NodePath::Album(ai, bi) => {
                if let Some(album) = self
                    .artists
                    .get_mut(ai)
                    .and_then(|a| a.albums.get_mut(bi))
                {
                    for track in &mut album.tracks {
                        track.checked = checked;
                    }
                }
            }
            NodePath::Track(ai, bi, ti) => {
                if let Some(track) = self
                    .artists
                    .get_mut(ai)
                    .and_then(|a| a.albums.get_mut(bi))
                    .and_then(|a| a.tracks.get_mut(ti))
                {
                    track.checked = checked;
                }
            }
        }
    }

    pub fn set_all_checked(&mut self, checked: bool) {
        for artist in &mut self.artists {
            for album in &mut artist.albums {
                for track in &mut album.tracks {
                    track.checked = checked;
                }
            }
        }
    }

    pub fn set_all_expanded(&mut self, expanded: bool) {
        for artist in &mut self.artists {
            artist.expanded = expanded;
            for album in &mut artist.albums {
                album.expanded = expanded;
            }
        }
    }

    pub fn tracks(&self) -> impl Iterator<Item = &TrackNode> {
        self.artists
            .iter()
            .flat_map(|a| a.albums.iter())
            .flat_map(|a| a.tracks.iter())
    }

    pub fn checked_ids(&self) -> Vec<u32> {
        self.tracks().filter(|t| t.checked).map(|t| t.id).collect()
    }

    pub fn track_count(&self) -> usize {
        self.tracks().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: u32, artist: &str, album: &str, title: &str) -> DeviceTrack {
        DeviceTrack {
            id,
            name: format!("{title}.mp3"),
            size: 100,
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
        }
    }

    fn sized(id: u32, artist: &str, album: &str, title: &str, size: u64) -> DeviceTrack {
        DeviceTrack {
            size,
            ..track(id, artist, album, title)
        }
    }

    #[test]
    fn tracks_group_into_artists_and_albums() {
        let tree = MusicTree::build(&[
            track(1, "A-ha", "Hunting High and Low", "Take On Me"),
            track(2, "A-ha", "Hunting High and Low", "Train of Thought"),
            track(3, "Judas Priest", "Painkiller", "Painkiller"),
        ]);

        assert_eq!(tree.artists.len(), 2);
        assert_eq!(tree.artists[0].name, "A-ha");
        assert_eq!(tree.artists[0].albums[0].tracks.len(), 2);
        assert_eq!(tree.track_count(), 3);
    }

    #[test]
    fn missing_tags_fall_back_to_placeholders_rather_than_vanishing() {
        let tree = MusicTree::build(&[track(1, "", "", "Mystery")]);

        assert_eq!(tree.artists[0].name, UNKNOWN_ARTIST);
        assert_eq!(tree.artists[0].albums[0].name, UNKNOWN_ALBUM);
        assert_eq!(tree.track_count(), 1);
    }

    #[test]
    fn a_track_with_no_title_falls_back_to_its_filename() {
        let mut t = track(1, "X", "Y", "");
        t.name = "weird-file.mp3".into();
        let tree = MusicTree::build(&[t]);

        assert_eq!(tree.artists[0].albums[0].tracks[0].title, "weird-file.mp3");
    }

    #[test]
    fn collapsed_artists_hide_their_albums_and_tracks() {
        let tree = MusicTree::build(&[track(1, "A-ha", "Hunting High and Low", "Take On Me")]);
        let (rows, paths) = tree.rows();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, NodeKind::Artist);
        assert_eq!(paths[0], NodePath::Artist(0));
    }

    #[test]
    fn expanding_reveals_the_next_level_only() {
        let mut tree = MusicTree::build(&[track(1, "A-ha", "Hunting High and Low", "Take On Me")]);

        tree.toggle_expanded(NodePath::Artist(0));
        let (rows, _) = tree.rows();
        assert_eq!(rows.len(), 2, "artist plus its album");
        assert_eq!(rows[1].kind, NodeKind::Album);

        tree.toggle_expanded(NodePath::Album(0, 0));
        let (rows, paths) = tree.rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].kind, NodeKind::Track);
        assert_eq!(paths[2], NodePath::Track(0, 0, 0));
    }

    #[test]
    fn checking_an_artist_checks_every_track_beneath_it() {
        let mut tree = MusicTree::build(&[
            track(1, "A-ha", "Hunting High and Low", "Take On Me"),
            track(2, "A-ha", "Scoundrel Days", "The Swing of Things"),
        ]);

        tree.set_checked(NodePath::Artist(0), true);

        assert_eq!(tree.checked_ids().len(), 2);
        let (rows, _) = tree.rows();
        assert_eq!(rows[0].check, CheckState::Checked);
    }

    #[test]
    fn a_partly_checked_artist_reports_partial() {
        let mut tree = MusicTree::build(&[
            track(1, "A-ha", "Hunting High and Low", "Take On Me"),
            track(2, "A-ha", "Scoundrel Days", "The Swing of Things"),
        ]);

        tree.set_checked(NodePath::Album(0, 0), true);

        let (rows, _) = tree.rows();
        assert_eq!(rows[0].check, CheckState::Partial);
        assert_eq!(tree.checked_ids(), vec![1]);
    }

    #[test]
    fn unchecking_an_album_leaves_other_albums_alone() {
        let mut tree = MusicTree::build(&[
            track(1, "A-ha", "Hunting High and Low", "Take On Me"),
            track(2, "A-ha", "Scoundrel Days", "The Swing of Things"),
        ]);

        tree.set_all_checked(true);
        tree.set_checked(NodePath::Album(0, 0), false);

        assert_eq!(tree.checked_ids(), vec![2]);
    }

    #[test]
    fn artist_rows_total_their_tracks_and_bytes() {
        let tree = MusicTree::build(&[
            sized(1, "A-ha", "Hunting High and Low", "Take On Me", 40),
            sized(2, "A-ha", "Scoundrel Days", "The Swing of Things", 60),
        ]);

        let (rows, _) = tree.rows();
        assert_eq!(rows[0].track_count, 2);
        assert_eq!(rows[0].size, 100);
    }

    #[test]
    fn artists_differing_only_in_case_are_one_artist() {
        let tree = MusicTree::build(&[
            track(1, "A-ha", "Hunting High and Low", "Take On Me"),
            track(2, "A-HA", "Hunting High and Low", "Train of Thought"),
        ]);

        assert_eq!(tree.artists.len(), 1);
        assert_eq!(tree.artists[0].albums.len(), 1);
    }

    #[test]
    fn expand_all_reveals_every_row() {
        let mut tree = MusicTree::build(&[
            track(1, "A-ha", "Hunting High and Low", "Take On Me"),
            track(2, "Judas Priest", "Painkiller", "Painkiller"),
        ]);

        tree.set_all_expanded(true);
        let (rows, _) = tree.rows();

        // Two artists, each with one album and one track.
        assert_eq!(rows.len(), 6);
    }
}
