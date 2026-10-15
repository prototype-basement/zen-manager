//! Playlist reading and editing, over raw libmtp.
//!
//! libmtp-rs wraps no playlist calls, so these go through libmtp-sys directly.
//!
//! Everything here works on an *uncached* connection, which is what the app
//! opens. That rules out `LIBMTP_Get_Playlist_List`: it walks libmtp's object
//! cache, and `flush_handles` returns immediately when the device is uncached,
//! so it always reports zero playlists. Playlist objects are found in the same
//! folder listing the music tree already uses, then read one at a time with
//! `LIBMTP_Get_Playlist`, which fetches on demand and does work uncached.
use libmtp_sys as ffi;
use std::ffi::{CStr, CString};

#[derive(Clone, Debug, Default)]
pub struct Playlist {
    pub id: u32,
    pub name: String,
    /// Track object ids, in the order the player will play them.
    pub tracks: Vec<u32>,
}

unsafe fn to_string(ptr: *const std::os::raw::c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    }
}

/// Walks the device for playlist objects and reads each one.
///
/// # Safety
/// `device` must be an open libmtp device.
pub unsafe fn list(device: *mut ffi::LIBMTP_mtpdevice_t) -> Vec<Playlist> {
    let mut out = Vec::new();
    // Same depth bound as the music tree, for the same reason.
    let mut stack: Vec<(u32, u32)> = vec![(ffi::LIBMTP_FILES_AND_FOLDERS_ROOT, 0)];

    while let Some((parent, depth)) = stack.pop() {
        let mut file = unsafe { ffi::LIBMTP_Get_Files_And_Folders(device, 0, parent) };
        while !file.is_null() {
            let (ftype, id, next) = unsafe { ((*file).filetype, (*file).item_id, (*file).next) };

            if ftype == ffi::LIBMTP_filetype_t_LIBMTP_FILETYPE_FOLDER {
                if depth < 4 {
                    stack.push((id, depth + 1));
                }
            } else if ftype == ffi::LIBMTP_filetype_t_LIBMTP_FILETYPE_PLAYLIST
                && let Some(playlist) = unsafe { read(device, id) }
            {
                out.push(playlist);
            }

            unsafe { ffi::LIBMTP_destroy_file_t(file) };
            file = next;
        }
    }

    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// # Safety
/// `device` must be an open libmtp device.
unsafe fn read(device: *mut ffi::LIBMTP_mtpdevice_t, id: u32) -> Option<Playlist> {
    let raw = unsafe { ffi::LIBMTP_Get_Playlist(device, id) };
    if raw.is_null() {
        return None;
    }

    let playlist = unsafe {
        let tracks = if (*raw).tracks.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts((*raw).tracks, (*raw).no_tracks as usize).to_vec()
        };
        Playlist {
            id: (*raw).playlist_id,
            name: to_string((*raw).name),
            tracks,
        }
    };

    unsafe { ffi::LIBMTP_destroy_playlist_t(raw) };
    Some(playlist)
}

/// Builds a libmtp playlist struct. The caller must not let libmtp free the
/// track array, which belongs to the Vec passed in.
unsafe fn build(name: &str, tracks: &mut [u32]) -> Option<*mut ffi::LIBMTP_playlist_t> {
    let name = CString::new(name).ok()?;
    let raw = unsafe { ffi::LIBMTP_new_playlist_t() };
    if raw.is_null() {
        return None;
    }
    unsafe {
        // strdup because destroy_playlist_t frees this pointer.
        (*raw).name = libc::strdup(name.as_ptr());
        (*raw).tracks = tracks.as_mut_ptr();
        (*raw).no_tracks = tracks.len() as u32;
    }
    Some(raw)
}

/// Hands the borrowed track array back before libmtp frees the struct.
unsafe fn release(raw: *mut ffi::LIBMTP_playlist_t) {
    unsafe {
        (*raw).tracks = std::ptr::null_mut();
        (*raw).no_tracks = 0;
        ffi::LIBMTP_destroy_playlist_t(raw);
    }
}

/// Creates a playlist and returns its new id.
///
/// # Safety
/// `device` must be an open libmtp device.
pub unsafe fn create(
    device: *mut ffi::LIBMTP_mtpdevice_t,
    name: &str,
    tracks: &mut [u32],
) -> Result<u32, ()> {
    let raw = unsafe { build(name, tracks) }.ok_or(())?;
    // parent_id 0 lets libmtp choose the device's playlist folder.
    unsafe {
        (*raw).parent_id = 0;
        (*raw).storage_id = 0;
    }
    let rc = unsafe { ffi::LIBMTP_Create_New_Playlist(device, raw) };
    let id = unsafe { (*raw).playlist_id };
    unsafe { release(raw) };

    if rc == 0 { Ok(id) } else { Err(()) }
}

/// Renames a playlist and replaces its contents, order included.
///
/// # Safety
/// `device` must be an open libmtp device.
pub unsafe fn update(
    device: *mut ffi::LIBMTP_mtpdevice_t,
    id: u32,
    name: &str,
    tracks: &mut [u32],
) -> Result<(), ()> {
    let raw = unsafe { build(name, tracks) }.ok_or(())?;
    unsafe { (*raw).playlist_id = id };
    let rc = unsafe { ffi::LIBMTP_Update_Playlist(device, raw) };
    unsafe { release(raw) };

    if rc == 0 { Ok(()) } else { Err(()) }
}
