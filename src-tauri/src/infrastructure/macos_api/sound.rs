//! Play the bundled clipboard sounds through System Sound Services
//! (AudioToolbox), the path macOS uses for its own interface sounds. Same WAV
//! data as Windows (see `bundled_sound`).
//!
//! The system sound server renders them, so no output stream opens in TieZ.
//! NSSound opened one on every play: the call blocked until it started, and it
//! stayed open for seconds after each sound, holding the output device (#174).
//!
//! System sounds have no volume of their own, so the slider is applied by
//! registering a copy of the WAV scaled to that volume. The server re-reads the
//! file on later plays and plays nothing if it is gone, so the copy lives as
//! long as its sound ID, and a play that finds it missing registers it again.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::infrastructure::bundled_sound::{scale_wav_volume, COPY_SOUND, PASTE_SOUND};

type CFURLRef = *const c_void;
type OSStatus = i32;
type SystemSoundID = u32;

/// `kAudioServicesPropertyIsUISound`. Cleared so the sounds follow TieZ's own
/// switch, not "Play user interface sound effects" in System Settings.
const K_AUDIO_SERVICES_PROPERTY_IS_UI_SOUND: u32 = u32::from_be_bytes(*b"isui");

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(
        allocator: *const c_void,
        buffer: *const u8,
        buf_len: isize,
        is_directory: u8,
    ) -> CFURLRef;
    fn CFRelease(cf: *const c_void);
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioServicesCreateSystemSoundID(url: CFURLRef, out_id: *mut SystemSoundID) -> OSStatus;
    fn AudioServicesDisposeSystemSoundID(id: SystemSoundID) -> OSStatus;
    fn AudioServicesPlaySystemSound(id: SystemSoundID);
    fn AudioServicesSetProperty(
        property: u32,
        specifier_size: u32,
        specifier: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> OSStatus;
}

/// A sound registered with the sound server and the file the server reads.
struct Registered {
    id: SystemSoundID,
    percent: u32,
    path: PathBuf,
}

impl Drop for Registered {
    fn drop(&mut self) {
        unsafe {
            AudioServicesDisposeSystemSoundID(self.id);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

static COPY_SLOT: Mutex<Option<Registered>> = Mutex::new(None);
static PASTE_SLOT: Mutex<Option<Registered>> = Mutex::new(None);

fn slot_for(kind: &str) -> (&'static Mutex<Option<Registered>>, &'static str, &'static [u8]) {
    match kind {
        "paste" => (&PASTE_SLOT, "paste", PASTE_SOUND),
        _ => (&COPY_SLOT, "copy", COPY_SOUND),
    }
}

fn sound_dir() -> PathBuf {
    std::env::temp_dir().join(if cfg!(test) { "tiez-sounds-test" } else { "tiez-sounds" })
}

fn create_sound_id(path: &Path) -> Option<SystemSoundID> {
    use std::os::unix::ffi::OsStrExt;

    let raw = path.as_os_str().as_bytes();
    let url = unsafe {
        CFURLCreateFromFileSystemRepresentation(std::ptr::null(), raw.as_ptr(), raw.len() as isize, 0)
    };
    if url.is_null() {
        return None;
    }
    let mut id: SystemSoundID = 0;
    let status = unsafe { AudioServicesCreateSystemSoundID(url, &mut id) };
    unsafe { CFRelease(url) };
    if status != 0 {
        eprintln!("[sound] AudioServicesCreateSystemSoundID failed: {status}");
        return None;
    }

    let is_ui_sound: u32 = 0;
    unsafe {
        AudioServicesSetProperty(
            K_AUDIO_SERVICES_PROPERTY_IS_UI_SOUND,
            std::mem::size_of::<SystemSoundID>() as u32,
            (&id as *const SystemSoundID).cast(),
            std::mem::size_of::<u32>() as u32,
            (&is_ui_sound as *const u32).cast(),
        );
    }
    Some(id)
}

/// Write `wav` scaled to `percent` and register it with the sound server.
fn register(name: &str, wav: &[u8], percent: u32) -> Option<Registered> {
    let dir = sound_dir();
    let path = dir.join(format!("{name}-{percent}.wav"));
    let data = if percent >= 100 {
        wav.to_vec()
    } else {
        scale_wav_volume(wav, percent as f32 / 100.0).ok()?
    };
    if let Err(err) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, data)) {
        eprintln!("[sound] could not write {}: {err}", path.display());
        return None;
    }

    match create_sound_id(&path) {
        Some(id) => Some(Registered { id, percent, path }),
        None => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// Run `f` with the sound ID for `kind` at `volume`, registering it first when
/// the volume changed or its file is gone. The lock stays held across `f`, so
/// no other thread can dispose the ID in between.
fn with_sound_id<R>(kind: &str, volume: f64, f: impl FnOnce(SystemSoundID) -> R) -> Option<R> {
    let percent = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;
    if percent == 0 {
        return None;
    }

    let (slot, name, wav) = slot_for(kind);
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    let usable = slot
        .as_ref()
        .is_some_and(|s| s.percent == percent && s.path.is_file());
    if !usable {
        *slot = None; // dispose the old ID and its file before writing the next
        *slot = register(name, wav, percent);
    }
    slot.as_ref().map(|s| f(s.id))
}

pub fn play_clipboard_sound(kind: &str, volume: f64) {
    with_sound_id(kind, volume, |id| unsafe { AudioServicesPlaySystemSound(id) });
}

/// Register both sounds at startup so the first copy feedback only has to play.
pub fn preload_clipboard_sounds(volume: f64) {
    with_sound_id("copy", volume, |_| ());
    with_sound_id("paste", volume, |_| ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_registers_when_the_volume_changes_or_the_file_is_gone() {
        let first = with_sound_id("paste", 0.7, |id| id).expect("registered at 70%");
        assert_eq!(with_sound_id("paste", 0.7, |id| id), Some(first));

        let path = |slot: &Mutex<Option<Registered>>| {
            slot.lock().unwrap().as_ref().map(|s| s.path.clone()).unwrap()
        };
        let at_70 = path(&PASTE_SLOT);
        let second = with_sound_id("paste", 0.3, |id| id).expect("registered at 30%");
        assert_ne!(second, first);
        assert!(!at_70.exists(), "the 70% copy is removed with its ID");

        let at_30 = path(&PASTE_SLOT);
        std::fs::remove_file(&at_30).unwrap();
        let third = with_sound_id("paste", 0.3, |id| id).expect("registered again");
        assert_ne!(third, second);
        assert!(at_30.is_file(), "the server reads the file on later plays");
    }

    #[test]
    fn plays_nothing_at_zero_volume() {
        assert_eq!(with_sound_id("copy", 0.0, |id| id), None);
    }
}
