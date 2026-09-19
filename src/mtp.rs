use crate::library::LocalTrack;
use anyhow::{Context, Result, anyhow};
use libmtp_rs::device::raw::detect_raw_devices;
use libmtp_rs::device::{BatteryLevel, MtpDevice, StorageSort};
use libmtp_rs::error::{Error as LibmtpError, MtpErrorKind};
use libmtp_rs::object::Object;
use libmtp_rs::object::filetypes::Filetype;
use libmtp_rs::object::properties::Property;
use libmtp_rs::storage::files::FileMetadata;
use libmtp_rs::storage::Parent;
use libmtp_rs::util::CallbackReturn;
use std::path::Path;

#[derive(Clone, Debug)]
pub enum Battery {
    /// Carries the device's raw pair as well as the percentage, because the
    /// scale is the device's own (the Zen counts to 255, not 100) and a bare
    /// percentage gives no way to tell a real reading from a misparsed one.
    Level { current: u8, max: u8 },
    ExternalPower,
}

impl Battery {
    pub fn percent(&self) -> Option<u8> {
        match self {
            Battery::Level { current, max } if *max > 0 => {
                Some(((*current as u32 * 100 / *max as u32).min(100)) as u8)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeviceInfo {
    pub model: String,
    pub manufacturer: String,
    pub storage: String,
    pub capacity: u64,
    pub free: u64,
    pub battery: Option<Battery>,
}

#[derive(Clone, Debug)]
pub struct DeviceTrack {
    pub id: u32,
    pub name: String,
    pub size: u64,
    /// Read back from MTP object properties. Empty means the device reported
    /// nothing for that field; the grouping layer decides what to show instead.
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// Stand-in album name for tracks whose tags carry none. The Zen builds its
/// album menu from the AlbumName property, and a track sent without one is
/// filed nowhere and becomes unreachable on the player — so an obviously
/// placeholder album beats no album at all.
pub const UNKNOWN_ALBUM: &str = "unknown";

pub struct Zen {
    device: MtpDevice,
}

fn filetype_for(path: &Path) -> Filetype {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => Filetype::Mp3,
        Some("wma") => Filetype::Wma,
        Some("wav") => Filetype::Wav,
        Some("ogg") => Filetype::Ogg,
        Some("flac") => Filetype::Flac,
        Some("m4a") => Filetype::M4a,
        Some("aac") => Filetype::Aac,
        Some("mp4") => Filetype::Mp4,
        Some("mp2") => Filetype::Mp2,
        _ => Filetype::UndefAudio,
    }
}

/// Whether a device object should appear as music.
///
/// The declared MTP filetype is not enough on its own: tracks put on the device
/// by other software often come back as `Unknown`, and filtering those out
/// hides files that really are there — invisible in the tree, undeletable, and
/// still taking up space. So fall back to the extension, which also keeps the
/// device's own `Unknown` files (WMPInfo.xml, DevIcon.fil) out.
pub fn is_audio_object(ftype: &Filetype, name: &str) -> bool {
    if matches!(
        ftype,
        Filetype::Mp3
            | Filetype::Wma
            | Filetype::Wav
            | Filetype::Ogg
            | Filetype::Flac
            | Filetype::M4a
            | Filetype::Aac
            | Filetype::Mp4
            | Filetype::Mp2
            | Filetype::UndefAudio
    ) {
        return true;
    }

    matches!(ftype, Filetype::Unknown) && crate::library::is_audio(Path::new(name))
}

impl Zen {
    pub fn open() -> Result<Self> {
        let raw_devices = detect_raw_devices().map_err(|e| match e {
            LibmtpError::MtpError {
                kind: MtpErrorKind::NoDeviceAttached,
                ..
            } => anyhow!("No MTP device detected."),
            other => anyhow!("Could not scan for MTP devices: {other}"),
        })?;
        let raw = raw_devices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No MTP device detected."))?;
        let device = raw
            .open_uncached()
            .ok_or_else(|| anyhow!("Found an MTP device but could not open it"))?;
        Ok(Self { device })
    }

    pub fn info(&mut self) -> Result<DeviceInfo> {
        self.device
            .update_storage(StorageSort::ByFreeSpace)
            .context("Failed to read device storage")?;

        let model = self.device.model_name().unwrap_or_default();
        let manufacturer = self.device.manufacturer_name().unwrap_or_default();
        // libmtp hands back (current, max) and reports against the device's own
        // scale — the Zen's max is 255, so the raw level is not a percentage.
        // Note libmtp-rs treats a current level of 0 as "on external power",
        // which is its heuristic, not something the device states.
        let battery = match self.device.battery_level() {
            Ok((BatteryLevel::OnBattery(current), max)) => Some(Battery::Level { current, max }),
            Ok((BatteryLevel::OnExternalPower, _)) => Some(Battery::ExternalPower),
            Err(_) => None,
        };

        let mut capacity = 0;
        let mut free = 0;
        let mut storage = String::new();
        for (_, s) in self.device.storage_pool().iter() {
            capacity += s.maximum_capacity();
            free += s.free_space_in_bytes();
            if storage.is_empty() {
                storage = s.description().unwrap_or("Storage").to_string();
            }
        }

        Ok(DeviceInfo {
            model,
            manufacturer,
            storage,
            capacity,
            free,
            battery,
        })
    }

    /// Audio extensions this particular device accepts. The Zen Micro reports
    /// only nine filetypes in total and FLAC, OGG and M4A are not among them,
    /// so trusting a hardcoded list would queue files it can never play.
    pub fn supported_extensions(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(types) = self.device.supported_filetypes() {
            for ftype in types {
                let ext = match ftype {
                    Filetype::Mp3 => "mp3",
                    Filetype::Wma => "wma",
                    Filetype::Wav => "wav",
                    Filetype::Ogg => "ogg",
                    Filetype::Flac => "flac",
                    Filetype::M4a => "m4a",
                    Filetype::Aac => "aac",
                    Filetype::Mp4 => "mp4",
                    Filetype::Mp2 => "mp2",
                    Filetype::Audible => "aa",
                    _ => continue,
                };
                out.push(ext.to_string());
            }
        }
        out
    }

    pub fn list_music(&mut self) -> Result<Vec<DeviceTrack>> {
        self.device
            .update_storage(StorageSort::ByFreeSpace)
            .context("Failed to read device storage")?;

        let music_folder = self.device.default_music_folder();
        let pool = self.device.storage_pool();

        let root = if music_folder == 0 {
            Parent::Root
        } else {
            Parent::Folder(music_folder)
        };

        let mut out = Vec::new();
        // Depth bound keeps a pathological device tree from stalling the UI.
        let mut stack = vec![(root, 0u32)];
        while let Some((parent, depth)) = stack.pop() {
            for file in pool.files_and_folders(parent) {
                let ftype = file.ftype();
                if matches!(ftype, Filetype::Folder) {
                    if depth < 4 {
                        stack.push((Parent::Folder(file.id()), depth + 1));
                    }
                } else if is_audio_object(&ftype, file.name()) {
                    // One USB round trip per property, so this is the slow part
                    // of a refresh. A device that does not support a property
                    // just yields an empty string.
                    out.push(DeviceTrack {
                        id: file.id(),
                        name: file.name().to_string(),
                        size: file.size(),
                        title: file.get_string(Property::Name).unwrap_or_default(),
                        artist: file.get_string(Property::Artist).unwrap_or_default(),
                        album: file.get_string(Property::AlbumName).unwrap_or_default(),
                    });
                }
            }
        }

        out.sort_by_key(|t| t.name.to_lowercase());
        Ok(out)
    }

    /// Sends `track` to the device and tags the resulting object with its
    /// metadata. The Zen builds its music menus from these MTP properties, not
    /// from the file itself, so a plain file copy would land on the device
    /// without ever appearing under Music.
    pub fn send(
        &mut self,
        track: &LocalTrack,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<()> {
        self.device
            .update_storage(StorageSort::ByFreeSpace)
            .context("Failed to read device storage")?;

        let music_folder = self.device.default_music_folder();
        let pool = self.device.storage_pool();

        let storage = pool
            .iter()
            .max_by_key(|(_, s)| s.free_space_in_bytes())
            .map(|(_, s)| s)
            .ok_or_else(|| anyhow!("Device reported no storage"))?;

        if storage.free_space_in_bytes() < track.size {
            return Err(anyhow!(
                "Not enough space on {}: need {}, {} free",
                storage.description().unwrap_or("device"),
                format_bytes(track.size),
                format_bytes(storage.free_space_in_bytes())
            ));
        }

        let parent = if music_folder == 0 {
            Parent::Root
        } else {
            Parent::Folder(music_folder)
        };

        let modification_date = std::fs::metadata(&track.path)
            .and_then(|m| m.modified())
            .map(Into::into)
            .unwrap_or_else(|_| chrono::Utc::now());

        let metadata = FileMetadata {
            file_size: track.size,
            file_name: &track.file_name,
            file_type: filetype_for(&track.path),
            modification_date,
        };

        let sent = storage
            .send_file_from_path_with_callback(&track.path, parent, metadata, |sent, total| {
                if progress(sent, total) {
                    CallbackReturn::Continue
                } else {
                    CallbackReturn::Cancel
                }
            })
            .with_context(|| format!("Failed to send {}", track.file_name))?;

        // Best effort: devices reject properties they do not support, and a
        // missing genre is not worth failing an otherwise good transfer.
        let _ = sent.set_string(Property::Name, &track.title);
        if !track.artist.is_empty() {
            let _ = sent.set_string(Property::Artist, &track.artist);
            let _ = sent.set_string(Property::AlbumArtist, &track.artist);
        }
        let album = if track.album.is_empty() {
            UNKNOWN_ALBUM
        } else {
            &track.album
        };
        let _ = sent.set_string(Property::AlbumName, album);
        if !track.genre.is_empty() {
            let _ = sent.set_string(Property::Genre, &track.genre);
        }
        if track.track_no > 0 {
            let _ = sent.set_u16(Property::Track, track.track_no.min(u16::MAX as u32) as u16);
        }
        let millis = track.duration.as_millis();
        if millis > 0 {
            let _ = sent.set_u32(Property::Duration, millis.min(u32::MAX as u128) as u32);
        }

        Ok(())
    }

    pub fn delete(&mut self, id: u32) -> Result<()> {
        self.device
            .update_storage(StorageSort::ByFreeSpace)
            .context("Failed to read device storage")?;
        let file = self
            .device
            .search_file(id)
            .with_context(|| format!("Could not find object {id} on device"))?;
        file.delete().context("Failed to delete track")?;
        Ok(())
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_audio_filetypes_are_music() {
        assert!(is_audio_object(&Filetype::Mp3, "song.mp3"));
        assert!(is_audio_object(&Filetype::Flac, "song.flac"));
        // Some devices report audio only as "undefined audio".
        assert!(is_audio_object(&Filetype::UndefAudio, "song.mp3"));
    }

    #[test]
    fn audio_reported_as_unknown_is_still_music() {
        // Tracks written by other software come back like this; dropping them
        // would hide files that are really on the device.
        assert!(is_audio_object(&Filetype::Unknown, "01 Overcompensate.mp3"));
        assert!(is_audio_object(&Filetype::Unknown, "Track.WMA"));
    }

    #[test]
    fn the_devices_own_unknown_files_are_not_music() {
        assert!(!is_audio_object(&Filetype::Unknown, "WMPInfo.xml"));
        assert!(!is_audio_object(&Filetype::Unknown, "DevIcon.fil"));
        assert!(!is_audio_object(&Filetype::Unknown, "DevLogo.fil"));
    }

    #[test]
    fn non_audio_filetypes_are_never_music() {
        assert!(!is_audio_object(&Filetype::Jpeg, "cover.jpg"));
        assert!(!is_audio_object(&Filetype::Folder, "Music"));
        // An extension alone must not override a declared non-audio type.
        assert!(!is_audio_object(&Filetype::Text, "notes.mp3"));
    }
}
