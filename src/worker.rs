use crate::music::{CheckState, MusicTree, NodeKind, NodePath};
use crate::mtp::{Battery, DeviceTrack, Zen, format_bytes};
use crate::queue::{self, DeviceState, QueuedFile};
use crate::{AppWindow, MusicRow, PlaylistRow, PlaylistTrackRow, QueueRow};
use slint::{ModelRc, VecModel};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Status codes shared with the UI.
///
/// Prose lives in `ui/app.slint` so `@tr()` can reach it — Slint's translation
/// macro only covers `.slint` files, so anything formatted here would be stuck
/// in English. The worker sends a code plus numbers; the UI does the wording.
pub mod status {
    pub const NONE: i32 = 0;
    pub const READY: i32 = 1;
    pub const READY_SKIPPING: i32 = 2;
    pub const NONE_SENDABLE: i32 = 3;
    pub const READING_FILES: i32 = 4;
    pub const LOOKING_FOR_DEVICE: i32 = 5;
    pub const SENDING: i32 = 6;
    pub const SENT: i32 = 7;
    pub const CANCELLED: i32 = 8;
    pub const DELETING: i32 = 9;
    pub const DELETED_MANY: i32 = 10;
    pub const TRACKS_ON_DEVICE: i32 = 12;
    pub const NO_DEVICE_CONNECTED: i32 = 13;
    pub const PLAYLIST_SAVED: i32 = 14;
    pub const PLAYLIST_DELETED: i32 = 15;
    pub const PLAYLIST_FAILED: i32 = 16;
}

pub enum Command {
    RefreshDevice,
    AddPaths(Vec<PathBuf>),
    RemoveAt(usize),
    ClearQueue,
    Transfer,
    ToggleExpanded(usize),
    SetChecked(usize, bool),
    SetAllChecked(bool),
    SetAllExpanded(bool),
    DeleteChecked,
    RefreshPlaylists,
    SelectPlaylist(usize),
    NewPlaylist,
    DeletePlaylist,
    /// Appends whatever is ticked in the music tree to the playlist being
    /// edited. Not written to the device until Save.
    AddCheckedToDraft,
    RemoveTrackFromPlaylist(usize),
    MoveTrackInPlaylist(usize, i32),
    /// Drag-and-drop reorder: take the track at `from` and drop it at `to`.
    MoveTrackToPosition(usize, usize),
    SavePlaylist(String),
    DiscardPlaylistEdits,
}

pub struct WorkerHandle {
    tx: Sender<Command>,
    cancel: Arc<AtomicBool>,
}

impl WorkerHandle {
    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }

    pub fn canceller(&self) -> impl Fn() + 'static {
        let cancel = Arc::clone(&self.cancel);
        move || cancel.store(true, Ordering::Relaxed)
    }
}

pub fn spawn(ui: slint::Weak<AppWindow>) -> WorkerHandle {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);

    std::thread::spawn(move || {
        let mut worker = Worker {
            ui,
            zen: None,
            queue: Vec::new(),
            device: Vec::new(),
            supported_extensions: Vec::new(),
            playlists: Vec::new(),
            selected_playlist: None,
            draft_name: String::new(),
            draft_tracks: Vec::new(),
            draft_is_new: false,
            tree: MusicTree::default(),
            paths: Vec::new(),
            free_space: 0,
            capacity: 0,
            cancel: worker_cancel,
        };
        worker.run(rx);
    });

    WorkerHandle { tx, cancel }
}

struct Worker {
    ui: slint::Weak<AppWindow>,
    zen: Option<Zen>,
    queue: Vec<QueuedFile>,
    device: Vec<DeviceTrack>,
    supported_extensions: Vec<String>,
    playlists: Vec<crate::playlists::Playlist>,
    /// Index into `playlists`, or none when nothing is selected.
    selected_playlist: Option<usize>,
    /// Edits live here until saved, so a half-made playlist never reaches the
    /// device and Discard can simply drop them.
    draft_name: String,
    draft_tracks: Vec<u32>,
    draft_is_new: bool,
    tree: MusicTree,
    /// Maps each visible row index to its node, rebuilt on every push so the
    /// indices the UI sends back always refer to the rows it is showing.
    paths: Vec<NodePath>,
    free_space: u64,
    capacity: u64,
    cancel: Arc<AtomicBool>,
}

#[allow(clippy::too_many_arguments)]
fn post_status(
    ui: &slint::Weak<AppWindow>,
    code: i32,
    n: i32,
    total: i32,
    name: String,
    failed: i32,
    detail: String,
) {
    post(ui, move |ui| {
        ui.set_status_code(code);
        ui.set_status_n(n);
        ui.set_status_total(total);
        ui.set_status_name(name.into());
        ui.set_status_failed(failed);
        ui.set_status_detail(detail.into());
    });
}

fn post(ui: &slint::Weak<AppWindow>, f: impl FnOnce(AppWindow) + Send + 'static) {
    let ui = ui.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            f(handle);
        }
    });
}

/// SVG path for a pie wedge covering `fraction` of a circle, swept clockwise
/// from twelve o'clock within a 100×100 viewbox.
fn pie_wedge(fraction: f32) -> String {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction <= 0.0 {
        return String::new();
    }
    if fraction >= 1.0 {
        return "M 50 0 A 50 50 0 1 1 50 100 A 50 50 0 1 1 50 0 Z".to_string();
    }

    let angle = fraction * std::f32::consts::TAU;
    let x = 50.0 + 50.0 * angle.sin();
    let y = 50.0 - 50.0 * angle.cos();
    let large_arc = if fraction > 0.5 { 1 } else { 0 };
    format!("M 50 50 L 50 0 A 50 50 0 {large_arc} 1 {x:.3} {y:.3} Z")
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs == 0 {
        return "—".to_string();
    }
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// An em dash rather than an empty string, so a missing tag reads as
/// "nothing here" instead of looking like a rendering gap.
fn or_dash(value: &str) -> String {
    if value.trim().is_empty() {
        "—".to_string()
    } else {
        value.to_string()
    }
}

fn queue_row(file: &QueuedFile) -> QueueRow {
    let track = file.track.as_ref();
    QueueRow {
        name: file.name.clone().into(),
        detail: format_bytes(file.size).into(),
        glyph: file.issue.glyph().into(),
        message: file.issue.message().into(),
        blocked: file.issue.blocks_transfer(),
        title: track.map(|t| or_dash(&t.title)).unwrap_or_else(|| "—".into()).into(),
        artist: track.map(|t| or_dash(&t.artist)).unwrap_or_else(|| "—".into()).into(),
        album: track.map(|t| or_dash(&t.album)).unwrap_or_else(|| "—".into()).into(),
        genre: track.map(|t| or_dash(&t.genre)).unwrap_or_else(|| "—".into()).into(),
        track_no: track
            .map(|t| if t.track_no == 0 { "—".to_string() } else { t.track_no.to_string() })
            .unwrap_or_else(|| "—".into())
            .into(),
        duration: track
            .map(|t| format_duration(t.duration))
            .unwrap_or_else(|| "—".into())
            .into(),
    }
}

fn check_code(state: CheckState) -> i32 {
    match state {
        CheckState::Unchecked => 0,
        CheckState::Partial => 1,
        CheckState::Checked => 2,
    }
}

impl Worker {
    fn run(&mut self, rx: Receiver<Command>) {
        self.refresh_device();
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Command::RefreshDevice => self.refresh_device(),
                Command::AddPaths(paths) => self.add_paths(paths),
                Command::RemoveAt(index) => {
                    if index < self.queue.len() {
                        self.queue.remove(index);
                    }
                    self.push_queue();
                }
                Command::ClearQueue => {
                    self.queue.clear();
                    self.push_queue();
                }
                Command::Transfer => self.transfer(),
                Command::ToggleExpanded(index) => {
                    if let Some(path) = self.paths.get(index).copied() {
                        self.tree.toggle_expanded(path);
                        self.push_music();
                    }
                }
                Command::SetChecked(index, checked) => {
                    if let Some(path) = self.paths.get(index).copied() {
                        self.tree.set_checked(path, checked);
                        self.push_music();
                    }
                }
                Command::SetAllChecked(checked) => {
                    self.tree.set_all_checked(checked);
                    self.push_music();
                }
                Command::SetAllExpanded(expanded) => {
                    self.tree.set_all_expanded(expanded);
                    self.push_music();
                }
                Command::DeleteChecked => self.delete_checked(),
                Command::RefreshPlaylists => self.refresh_playlists(),
                Command::SelectPlaylist(index) => self.select_playlist(index),
                Command::NewPlaylist => self.new_playlist(),
                Command::DeletePlaylist => self.delete_playlist(),
                Command::AddCheckedToDraft => self.add_checked_to_draft(),
                Command::RemoveTrackFromPlaylist(index) => {
                    if index < self.draft_tracks.len() {
                        self.draft_tracks.remove(index);
                        self.push_playlists();
                    }
                }
                Command::MoveTrackInPlaylist(index, delta) => self.move_track(index, delta),
                Command::MoveTrackToPosition(from, to) => self.move_track_to(from, to),
                Command::SavePlaylist(name) => self.save_playlist(name),
                Command::DiscardPlaylistEdits => {
                    match self.selected_playlist {
                        Some(index) => self.select_playlist(index),
                        None => {
                            self.draft_tracks.clear();
                            self.draft_name.clear();
                            self.draft_is_new = false;
                            self.push_playlists();
                        }
                    }
                }
            }
        }
    }

    fn status(&self, code: i32) {
        post_status(&self.ui, code, 0, 0, String::new(), 0, String::new());
    }

    fn status_n(&self, code: i32, n: i32, total: i32) {
        post_status(&self.ui, code, n, total, String::new(), 0, String::new());
    }

    fn device_state(&self) -> Option<DeviceState> {
        self.zen.as_ref().map(|_| DeviceState {
            free_space: self.free_space,
            track_names: self
                .device
                .iter()
                .map(|t| t.name.to_lowercase())
                .collect(),
            supported_extensions: self.supported_extensions.iter().cloned().collect(),
        })
    }

    /// Re-evaluates the queue and pushes it to the UI. Every path that changes
    /// the queue or the device state ends here, so the two never drift apart.
    fn push_queue(&mut self) {
        let device_state = self.device_state();
        queue::evaluate(&mut self.queue, device_state.as_ref());

        let rows: Vec<QueueRow> = self.queue.iter().map(queue_row).collect();
        let count = self.queue.len();
        let transferable = queue::transferable(&self.queue).len();
        let total = queue::total_size(&self.queue);

        let blocked = count - transferable;
        let code = if count == 0 {
            status::NONE
        } else if blocked == 0 {
            status::READY
        } else if transferable == 0 {
            status::NONE_SENDABLE
        } else {
            status::READY_SKIPPING
        };
        let size_text = format_bytes(total);

        post(&self.ui, move |ui| {
            ui.set_queue(ModelRc::new(VecModel::from(rows)));
            ui.set_queue_count(count as i32);
            ui.set_queue_size_text(size_text.into());
            ui.set_transferable_count(transferable as i32);
        });
        self.status_n(code, blocked as i32, count as i32);
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        self.status(status::READING_FILES);
        let mut added = queue::entries_for_paths(&paths);

        // Dropping the same file twice should not queue it twice.
        added.retain(|candidate| !self.queue.iter().any(|q| q.path == candidate.path));
        self.queue.append(&mut added);

        self.push_queue();
    }

    fn refresh_device(&mut self) {
        self.status(status::LOOKING_FOR_DEVICE);
        if self.zen.is_none() {
            match Zen::open() {
                Ok(zen) => self.zen = Some(zen),
                Err(e) => {
                    self.disconnect(e.to_string());
                    return;
                }
            }
        }

        let zen = self.zen.as_mut().expect("device opened above");

        let info = match zen.info() {
            Ok(info) => info,
            Err(e) => {
                self.disconnect(e.to_string());
                return;
            }
        };

        match zen.list_music() {
            Ok(tracks) => self.device = tracks,
            Err(e) => {
                self.disconnect(e.to_string());
                return;
            }
        }

        self.free_space = info.free;
        self.capacity = info.capacity;
        self.supported_extensions = zen.supported_extensions();

        let name = if info.model.is_empty() {
            "MTP device".to_string()
        } else if info.manufacturer.is_empty() {
            info.model.clone()
        } else {
            format!("{} {}", info.manufacturer, info.model)
        };

        let free_text = format_bytes(info.free);
        let capacity_text = format_bytes(info.capacity);
        let (battery_state, battery_percent, battery_current, battery_max) = match &info.battery {
            Some(battery @ Battery::Level { current, max }) => (
                1,
                battery.percent().unwrap_or(0) as i32,
                *current as i32,
                *max as i32,
            ),
            Some(Battery::ExternalPower) => (2, 0, 0, 0),
            None => (0, 0, 0, 0),
        };
        // Device-reported, so not translatable.
        let detail = info.storage.clone();

        let used = info.capacity.saturating_sub(info.free);
        let fraction = if info.capacity > 0 {
            used as f32 / info.capacity as f32
        } else {
            0.0
        };
        let wedge = pie_wedge(fraction);
        let used_text = format_bytes(used);
        let used_percent_text = format!("{:.1}", fraction * 100.0);

        post(&self.ui, move |ui| {
            ui.set_device_connected(true);
            ui.set_device_name(name.into());
            ui.set_device_detail(detail.into());
            ui.set_battery_state(battery_state);
            ui.set_battery_percent(battery_percent);
            ui.set_battery_current(battery_current);
            ui.set_battery_max(battery_max);
            ui.set_storage_free_text(free_text.into());
            ui.set_storage_capacity_text(capacity_text.into());
            ui.set_storage_used_fraction(fraction);
            ui.set_storage_wedge(wedge.into());
            ui.set_storage_used_text(used_text.into());
            ui.set_storage_used_percent_text(used_percent_text.into());
        });

        self.rebuild_tree();
        // Playlists come along with the device listing, so the page is right
        // whether or not it has been opened yet.
        self.refresh_playlists();
        let tracks = self.device.len() as i32;
        self.push_queue();
        // Only announce the device listing when nothing more specific is
        // pending, so a transfer summary is not immediately overwritten.
        if self.queue.is_empty() {
            self.status_n(status::TRACKS_ON_DEVICE, tracks, 0);
        }
    }

    /// Rebuilds the tree from the current device listing, preserving nothing:
    /// ids change after a device round trip, so stale selections would be
    /// dangerous for an operation that deletes.
    fn rebuild_tree(&mut self) {
        self.tree = MusicTree::build(&self.device);
        self.push_music();
    }

    fn push_music(&mut self) {
        let (rows, paths) = self.tree.rows();
        self.paths = paths;

        let ui_rows: Vec<MusicRow> = rows
            .iter()
            .map(|row| MusicRow {
                label: row.label.clone().into(),
                track_count: row.track_count,
                size_text: format_bytes(row.size).into(),
                depth: row.depth,
                check: check_code(row.check),
                expanded: row.expanded,
                expandable: row.kind != NodeKind::Track,
            })
            .collect();

        let checked = self.tree.checked_ids().len() as i32;
        let total = self.tree.track_count() as i32;

        post(&self.ui, move |ui| {
            ui.set_music_rows(ModelRc::new(VecModel::from(ui_rows)));
            ui.set_music_checked(checked);
            ui.set_music_total(total);
        });
    }

    /// Drops the handle so the next refresh reopens the device — libmtp sessions
    /// do not survive the device being unplugged mid-operation.
    fn disconnect(&mut self, message: String) {
        // The message is libmtp's own wording, so it is shown verbatim rather
        // than translated — error text is explicitly out of scope for v1.
        post_status(&self.ui, status::NO_DEVICE_CONNECTED, 0, 0, String::new(), 0, message.clone());
        self.zen = None;
        self.device.clear();
        self.supported_extensions.clear();
        self.tree = MusicTree::default();
        self.paths.clear();
        self.free_space = 0;
        self.capacity = 0;

        post(&self.ui, move |ui| {
            ui.set_device_connected(false);
            ui.set_device_name(String::new().into());
            ui.set_device_detail(String::new().into());
            ui.set_battery_state(0);
            ui.set_storage_used_fraction(0.0);
            ui.set_storage_wedge(String::new().into());
            ui.set_music_rows(ModelRc::new(VecModel::from(Vec::<MusicRow>::new())));
            ui.set_music_checked(0);
            ui.set_music_total(0);
        });

        self.push_queue();
    }

    fn transfer(&mut self) {
        let indices = queue::transferable(&self.queue);
        if indices.is_empty() {
            return;
        }

        let mut sent_paths: Vec<PathBuf> = Vec::new();

        {
            let Worker {
                ui, zen, queue, cancel, ..
            } = self;
            let Some(zen) = zen.as_mut() else {
                post_status(ui, status::NO_DEVICE_CONNECTED, 0, 0, String::new(), 0, String::new());
                return;
            };

            cancel.store(false, Ordering::Relaxed);
            post(ui, |ui| ui.set_busy(true));

            let total = indices.len();
            let mut failures: Vec<String> = Vec::new();
            let mut cancelled = false;

            for (nth, index) in indices.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    break;
                }
                let Some(entry) = queue.get(*index) else {
                    continue;
                };
                let Some(track) = entry.track.as_ref() else {
                    continue;
                };

                let title = track.title.clone();
                post_status(ui, status::SENDING, nth as i32 + 1, total as i32, title, 0, String::new());

                let progress_cancel = Arc::clone(cancel);
                let ui_progress = ui.clone();
                let mut last_percent = u32::MAX;
                let base = nth as f32;
                let mut on_progress = move |done: u64, total_bytes: u64| {
                    if progress_cancel.load(Ordering::Relaxed) {
                        return false;
                    }
                    // checked_div also covers the zero-byte file libmtp can
                    // report a total of 0 for.
                    if let Some(percent) = (done * 100).checked_div(total_bytes) {
                        let percent = percent as u32;
                        // libmtp fires this callback per USB chunk; only touching
                        // the event loop on whole-percent changes keeps it from
                        // flooding.
                        if percent != last_percent {
                            last_percent = percent;
                            let overall = (base + percent as f32 / 100.0) / total as f32;
                            post(&ui_progress, move |ui| ui.set_progress(overall));
                        }
                    }
                    true
                };

                match zen.send(track, &mut on_progress) {
                    Ok(()) => sent_paths.push(track.path.clone()),
                    Err(e) => {
                        if cancel.load(Ordering::Relaxed) {
                            cancelled = true;
                            break;
                        }
                        failures.push(format!("{}: {}", track.title, e));
                    }
                }
            }

            post(ui, |ui| {
                ui.set_busy(false);
                ui.set_progress(0.0);
            });

            let code = if cancelled { status::CANCELLED } else { status::SENT };
            let detail = failures.first().cloned().unwrap_or_default();
            post_status(
                ui,
                code,
                sent_paths.len() as i32,
                total as i32,
                String::new(),
                failures.len() as i32,
                detail,
            );
        }

        // Sent files leave the queue; anything that failed stays so it can be retried.
        self.queue.retain(|f| !sent_paths.contains(&f.path));

        // Drop the session so the next refresh reopens it.
        //
        // libmtp caches an object's properties when the object is created, and
        // its header states that the get/set property calls "do not update the
        // cache". Sending a file therefore caches it with no metadata, and the
        // artist/album written straight afterwards never reach that cache — so
        // re-listing in the same session reports every fresh track as unknown,
        // while a restart shows it correctly. A new session rebuilds the cache.
        if !sent_paths.is_empty() {
            self.zen = None;
        }

        self.refresh_device();
    }


    fn playlist_row(&self, playlist: &crate::playlists::Playlist) -> PlaylistRow {
        PlaylistRow {
            name: playlist.name.clone().into(),
            track_count: playlist.tracks.len() as i32,
        }
    }

    /// Looks up a device track by object id for display.
    fn track_label(&self, id: u32) -> (String, String) {
        match self.device.iter().find(|t| t.id == id) {
            Some(track) => {
                let title = if track.title.trim().is_empty() {
                    track.name.clone()
                } else {
                    track.title.clone()
                };
                (title, track.artist.clone())
            }
            // The playlist references something not in the music listing, e.g.
            // a file the device does not class as a track.
            None => (String::new(), String::new()),
        }
    }

    fn push_playlists(&mut self) {
        let rows: Vec<PlaylistRow> = self
            .playlists
            .iter()
            .map(|p| self.playlist_row(p))
            .collect();

        let tracks: Vec<PlaylistTrackRow> = self
            .draft_tracks
            .iter()
            .map(|id| {
                let (title, artist) = self.track_label(*id);
                PlaylistTrackRow {
                    title: if title.is_empty() { String::new() } else { title }.into(),
                    artist: artist.into(),
                    missing: !self.device.iter().any(|t| t.id == *id),
                }
            })
            .collect();

        let selected = self.selected_playlist.map(|i| i as i32).unwrap_or(-1);
        let has_draft = self.draft_is_new || self.selected_playlist.is_some();
        let dirty = match self.selected_playlist {
            Some(index) => self.playlists[index].tracks != self.draft_tracks,
            None => self.draft_is_new,
        };
        let name = self.draft_name.clone();

        post(&self.ui, move |ui| {
            ui.set_playlists(ModelRc::new(VecModel::from(rows)));
            ui.set_playlist_tracks(ModelRc::new(VecModel::from(tracks)));
            ui.set_playlist_selected(selected);
            ui.set_playlist_name(name.into());
            ui.set_playlist_has_draft(has_draft);
            ui.set_playlist_dirty(dirty);
        });
    }

    fn refresh_playlists(&mut self) {
        let Some(zen) = self.zen.as_mut() else {
            self.playlists.clear();
            self.push_playlists();
            return;
        };
        self.playlists = zen.list_playlists();

        // Keep the selection on the same playlist across a refresh where we can.
        if self.selected_playlist.is_some_and(|i| i >= self.playlists.len()) {
            self.selected_playlist = None;
            self.draft_name.clear();
            self.draft_tracks.clear();
        }
        self.push_playlists();
    }

    fn select_playlist(&mut self, index: usize) {
        if let Some(playlist) = self.playlists.get(index) {
            self.selected_playlist = Some(index);
            self.draft_name = playlist.name.clone();
            self.draft_tracks = playlist.tracks.clone();
            self.draft_is_new = false;
            self.push_playlists();
        }
    }

    fn new_playlist(&mut self) {
        self.selected_playlist = None;
        self.draft_name = String::new();
        self.draft_tracks.clear();
        self.draft_is_new = true;
        self.push_playlists();
    }

    fn move_track(&mut self, index: usize, delta: i32) {
        let target = index as i32 + delta;
        if index < self.draft_tracks.len() && target >= 0 && (target as usize) < self.draft_tracks.len() {
            self.draft_tracks.swap(index, target as usize);
            self.push_playlists();
        }
    }

    fn move_track_to(&mut self, from: usize, to: usize) {
        let len = self.draft_tracks.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let track = self.draft_tracks.remove(from);
        self.draft_tracks.insert(to, track);
        self.push_playlists();
    }

    fn save_playlist(&mut self, name: String) {
        let name = if name.trim().is_empty() {
            "Playlist".to_string()
        } else {
            name
        };
        self.draft_name = name.clone();
        let mut tracks = self.draft_tracks.clone();

        let Some(zen) = self.zen.as_mut() else {
            post_status(&self.ui, status::NO_DEVICE_CONNECTED, 0, 0, String::new(), 0, String::new());
            return;
        };

        let result = match self.selected_playlist.and_then(|i| self.playlists.get(i)) {
            Some(existing) => zen.update_playlist(existing.id, &name, &mut tracks).map(|_| ()),
            None => zen.create_playlist(&name, &mut tracks).map(|_| ()),
        };

        match result {
            Ok(()) => {
                self.draft_is_new = false;
                self.refresh_playlists();
                // Reselect by name, since ids shift when a new one is created.
                if let Some(index) = self.playlists.iter().position(|p| p.name == name) {
                    self.select_playlist(index);
                }
                post_status(&self.ui, status::PLAYLIST_SAVED, 0, 0, name, 0, String::new());
            }
            Err(error) => post_status(
                &self.ui,
                status::PLAYLIST_FAILED,
                0,
                0,
                String::new(),
                0,
                error.to_string(),
            ),
        }
    }

    fn delete_playlist(&mut self) {
        let Some(index) = self.selected_playlist else {
            // Nothing saved yet; just drop the draft.
            self.new_playlist();
            self.draft_is_new = false;
            self.push_playlists();
            return;
        };
        let Some(playlist) = self.playlists.get(index).cloned() else { return };
        let Some(zen) = self.zen.as_mut() else { return };

        match zen.delete(playlist.id) {
            Ok(()) => {
                self.selected_playlist = None;
                self.draft_name.clear();
                self.draft_tracks.clear();
                self.refresh_playlists();
                post_status(&self.ui, status::PLAYLIST_DELETED, 0, 0, playlist.name, 0, String::new());
            }
            Err(error) => post_status(
                &self.ui,
                status::PLAYLIST_FAILED,
                0,
                0,
                String::new(),
                0,
                error.to_string(),
            ),
        }
    }

    /// Appends the tree's ticked tracks to the draft, skipping ones already in
    /// it, then clears the ticks so the chooser opens empty next time.
    fn add_checked_to_draft(&mut self) {
        for id in self.tree.checked_ids() {
            if !self.draft_tracks.contains(&id) {
                self.draft_tracks.push(id);
            }
        }
        self.tree.set_all_checked(false);
        self.push_music();
        self.push_playlists();
    }

    fn delete_checked(&mut self) {
        let ids = self.tree.checked_ids();
        if ids.is_empty() {
            return;
        }

        {
            let Worker { ui, zen, .. } = self;
            let Some(zen) = zen.as_mut() else {
                post_status(ui, status::NO_DEVICE_CONNECTED, 0, 0, String::new(), 0, String::new());
                return;
            };

            post(ui, |ui| ui.set_busy(true));

            let total = ids.len();
            let mut deleted = 0usize;
            let mut failures: Vec<String> = Vec::new();

            for (nth, id) in ids.iter().enumerate() {
                post_status(ui, status::DELETING, nth as i32 + 1, total as i32, String::new(), 0, String::new());

                match zen.delete(*id) {
                    Ok(()) => deleted += 1,
                    Err(e) => failures.push(e.to_string()),
                }
            }

            post(ui, |ui| {
                ui.set_busy(false);
                ui.set_progress(0.0);
            });

            let detail = failures.first().cloned().unwrap_or_default();
            post_status(
                ui,
                status::DELETED_MANY,
                deleted as i32,
                total as i32,
                String::new(),
                failures.len() as i32,
                detail,
            );
        }

        self.refresh_device();
    }
}

#[cfg(test)]
mod tests {
    use super::pie_wedge;

    #[test]
    fn an_empty_wedge_draws_nothing() {
        assert_eq!(pie_wedge(0.0), "");
        assert_eq!(pie_wedge(-0.5), "");
    }

    #[test]
    fn a_full_wedge_draws_a_closed_circle() {
        let full = pie_wedge(1.0);
        assert!(full.starts_with("M 50 0"));
        assert!(full.ends_with('Z'));
        assert_eq!(pie_wedge(2.0), full);
    }

    #[test]
    fn a_quarter_wedge_ends_at_three_oclock() {
        let path = pie_wedge(0.25);
        assert!(path.contains("100.000 50.000"), "got {path}");
        // Under half a turn takes the short arc.
        assert!(path.contains("A 50 50 0 0 1"), "got {path}");
    }

    #[test]
    fn a_half_wedge_ends_at_six_oclock() {
        let path = pie_wedge(0.5);
        assert!(path.contains("50.000 100.000"), "got {path}");
    }

    #[test]
    fn wedges_past_the_halfway_point_take_the_long_arc() {
        assert!(pie_wedge(0.75).contains("A 50 50 0 1 1"));
    }

    #[test]
    fn a_tiny_wedge_is_still_drawn_rather_than_rounded_away() {
        // The user's Zen sits under 1% used; that sliver must still render.
        let path = pie_wedge(0.008);
        assert!(!path.is_empty());
        assert!(path.starts_with("M 50 50 L 50 0"), "got {path}");
    }
}
