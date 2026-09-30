//! Reading audio through libsndfile, as `soundfile.read(..., dtype="float64")`
//! does.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

#[repr(C)]
#[derive(Default)]
struct SfInfo {
    frames: i64,
    samplerate: c_int,
    channels: c_int,
    format: c_int,
    sections: c_int,
    seekable: c_int,
}

const SFM_READ: c_int = 0x10;

unsafe extern "C" {
    fn sf_open(path: *const c_char, mode: c_int, info: *mut SfInfo) -> *mut c_void;
    fn sf_readf_double(file: *mut c_void, ptr: *mut f64, frames: i64) -> i64;
    fn sf_close(file: *mut c_void) -> c_int;
    fn sf_strerror(file: *mut c_void) -> *const c_char;
    fn sf_version_string() -> *const c_char;
}

/// Decoded audio: interleaved float64 frames.
#[derive(Clone, Debug)]
pub struct Audio {
    pub data: Vec<f64>,
    pub channels: usize,
    pub rate: u32,
}

impl Audio {
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels.max(1)
    }
}

fn last_error(file: *mut c_void) -> String {
    // SAFETY: sf_strerror accepts null and returns a static C string.
    unsafe { CStr::from_ptr(sf_strerror(file)).to_string_lossy().into_owned() }
}

fn c_path(path: &Path) -> Result<CString, String> {
    CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("{}: path contains a NUL byte", path.display()))
}

/// Every frame of a file, normalized to [-1, 1) for integer formats.
pub fn read(path: &Path) -> Result<Audio, String> {
    let c = c_path(path)?;
    let mut info = SfInfo::default();
    // SAFETY: valid C string and SF_INFO pointer; the handle is closed below.
    let file = unsafe { sf_open(c.as_ptr(), SFM_READ, &mut info) };
    if file.is_null() {
        return Err(format!("{}: {}", path.display(), last_error(std::ptr::null_mut())));
    }
    let channels = info.channels.max(0) as usize;
    let mut data = vec![0.0; info.frames.max(0) as usize * channels];
    // SAFETY: the buffer holds frames * channels doubles.
    let read = unsafe { sf_readf_double(file, data.as_mut_ptr(), info.frames) };
    // SAFETY: file is an open handle.
    unsafe { sf_close(file) };
    data.truncate(read.max(0) as usize * channels);
    Ok(Audio {
        data,
        channels,
        rate: info.samplerate as u32,
    })
}

/// The linked libsndfile, e.g. `libsndfile-1.2.2`.
pub fn version() -> String {
    // SAFETY: returns a static C string.
    unsafe { CStr::from_ptr(sf_version_string()).to_string_lossy().into_owned() }
}
