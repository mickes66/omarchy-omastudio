pub mod ffi;
pub mod standard;

use ffi::*;
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMetadata {
    pub width: u32,
    pub height: u32,
    pub raw_width: u32,
    pub raw_height: u32,
    pub make: String,
    pub model: String,
    pub lens: String,
    pub iso: f32,
    pub shutter: f32,
    pub aperture: f32,
    pub focal_length: f32,
    pub timestamp: i64,
    pub cam_mul: [f32; 4],
}

pub struct ProcessedBuffer {
    ptr: *mut u8,
    /// true when `ptr` came from a Rust allocation (e.g. a decoded JPEG/TIFF buffer)
    /// and must be freed with `Vec::from_raw_parts` instead of the LibRaw C shim's
    /// `omaraw_free_image`.
    owned: bool,
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
    pub data_size: usize,
}

// SAFETY: The buffer is exclusively owned and safe to pass across threads
unsafe impl Send for ProcessedBuffer {}
unsafe impl Sync for ProcessedBuffer {}

impl ProcessedBuffer {
    pub fn as_slice(&self) -> &[u8] {
        if self.ptr.is_null() || self.data_size == 0 {
            &[]
        } else {
            // SAFETY: ptr points to valid heap memory of data_size allocated by C shim
            unsafe { std::slice::from_raw_parts(self.ptr, self.data_size) }
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }

    pub fn as_slice_u16(&self) -> &[u16] {
        if self.ptr.is_null() || self.data_size < 2 {
            &[]
        } else {
            // SAFETY: ptr was allocated by malloc in C shim, which is aligned to at least 8/16 bytes,
            // satisfying the 2-byte alignment required for u16. data_size contains exactly
            // (data_size / 2) u16 values when bits_per_sample == 16.
            unsafe {
                std::slice::from_raw_parts(self.ptr as *const u16, self.data_size / 2)
            }
        }
    }

    pub fn to_vec_u16(&self) -> Vec<u16> {
        self.as_slice_u16().to_vec()
    }

    /// Wraps a Rust-decoded pixel buffer (e.g. from the `image` crate) in the same
    /// type the LibRaw path produces, so pipeline/export/AI code downstream never
    /// needs to know which decoder a photo came from.
    pub(crate) fn from_owned(
        mut data: Vec<u8>,
        width: u32,
        height: u32,
        channels: u32,
        bits_per_sample: u32,
    ) -> Self {
        data.shrink_to_fit();
        let data_size = data.len();
        let ptr = data.as_mut_ptr();
        // Ownership transfers to the raw pointer; reclaimed in Drop via Vec::from_raw_parts.
        std::mem::forget(data);
        Self {
            ptr,
            owned: true,
            width,
            height,
            channels,
            bits_per_sample,
            data_size,
        }
    }
}

impl Drop for ProcessedBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            if self.owned {
                // SAFETY: ptr/data_size came from a Vec<u8> forgotten in from_owned after
                // shrink_to_fit, so len == capacity and the original allocator/layout match.
                unsafe {
                    drop(Vec::from_raw_parts(self.ptr, self.data_size, self.data_size));
                }
            } else {
                // SAFETY: ptr was allocated by malloc in C shim and is freed here
                unsafe {
                    omaraw_free_image(self.ptr);
                }
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

pub struct RawImage {
    // Path accessor
    handle: *mut std::os::raw::c_void,
    path: PathBuf,
}

// SAFETY: RawImage handle is managed exclusively by Rust wrapper
unsafe impl Send for RawImage {}

impl RawImage {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path_ref = path.as_ref();
        crate::security::verify_safe_file(path_ref)
            .map_err(|e| format!("Security check failed for {}: {}", path_ref.display(), e))?;

        let c_path = CString::new(path_ref.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path string: {}", e))?;

        let mut errcode: std::os::raw::c_int = 0;
        // SAFETY: c_path is a null-terminated C string, errcode is a valid pointer
        let handle = unsafe { omaraw_open(c_path.as_ptr(), &mut errcode) };

        if handle.is_null() {
            return Err(format!("LibRaw failed to open {}: code {}", path_ref.display(), errcode));
        }

        Ok(Self {
            handle,
            path: path_ref.to_path_buf(),
        })
    }

    /// Opens a RAW image directly from a verified open file descriptor via /proc/self/fd/{fd}
    /// without releasing the descriptor or following mutable disk paths.
    pub fn open_from_file(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::io::AsRawFd;

        let fd = file.as_raw_fd();
        let proc_path = format!("/proc/self/fd/{}", fd);
        let c_path = CString::new(proc_path.as_bytes())
            .map_err(|e| format!("Invalid proc path string: {}", e))?;

        let mut errcode: std::os::raw::c_int = 0;
        // SAFETY: c_path is a valid null-terminated C string, errcode is a valid pointer
        let handle = unsafe { omaraw_open(c_path.as_ptr(), &mut errcode) };

        if handle.is_null() {
            return Err(format!("LibRaw failed to open descriptor {}: code {}", fd, errcode));
        }

        Ok(Self {
            handle,
            path: PathBuf::from(proc_path),
        })
    }

    pub fn get_metadata(&self) -> Result<RawMetadata, String> {
        let mut cmeta = COmaRawMetadata {
            width: 0,
            height: 0,
            raw_width: 0,
            raw_height: 0,
            make: [0; 64],
            model: [0; 64],
            lens: [0; 128],
            iso: 0.0,
            shutter: 0.0,
            aperture: 0.0,
            focal_len: 0.0,
            timestamp: 0,
            cam_mul: [1.0, 1.0, 1.0, 1.0],
        };

        // SAFETY: handle is valid and cmeta is allocated on stack
        let ret = unsafe { omaraw_get_metadata(self.handle, &mut cmeta) };
        if ret != 0 {
            return Err("Failed to extract metadata from RAW file".to_string());
        }

        let make = unsafe { CStr::from_ptr(cmeta.make.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        let model = unsafe { CStr::from_ptr(cmeta.model.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        let lens = unsafe { CStr::from_ptr(cmeta.lens.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();

        Ok(RawMetadata {
            width: cmeta.width.max(0) as u32,
            height: cmeta.height.max(0) as u32,
            raw_width: cmeta.raw_width.max(0) as u32,
            raw_height: cmeta.raw_height.max(0) as u32,
            make,
            model,
            lens,
            iso: cmeta.iso,
            shutter: cmeta.shutter,
            aperture: cmeta.aperture,
            focal_length: cmeta.focal_len,
            timestamp: cmeta.timestamp,
            cam_mul: cmeta.cam_mul,
        })
    }

    pub fn extract_thumbnail<P: AsRef<Path>>(&self, dest_path: P) -> Result<(), String> {
        let dest = dest_path.as_ref();
        if let Some(parent) = dest.parent() {
            crate::security::ensure_secure_dir(parent)
                .map_err(|e| format!("Failed to ensure thumbnail directory: {}", e))?;
        }

        let c_dest = CString::new(dest.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid destination path: {}", e))?;

        // SAFETY: handle is valid, c_dest is valid C string
        let ret = unsafe { omaraw_extract_thumb_file(self.handle, c_dest.as_ptr()) };
        if ret != 0 {
            return Err(format!("Failed to extract embedded thumbnail: code {}", ret));
        }

        Ok(())
    }

    pub fn process_custom(&self, half_size: bool, quality: i32, bps: i32) -> Result<ProcessedBuffer, String> {
        let mut out_w = 0;
        let mut out_h = 0;
        let mut out_colors = 0;
        let mut out_size = 0;

        let hs = if half_size { 1 } else { 0 };
        // SAFETY: handle is valid, all output pointers point to stack variables
        let ptr = unsafe {
            omaraw_process_image(
                self.handle,
                hs,
                quality,
                bps,
                &mut out_w,
                &mut out_h,
                &mut out_colors,
                &mut out_size,
            )
        };

        if ptr.is_null() || out_size <= 0 {
            return Err("Failed to process RAW image".to_string());
        }

        Ok(ProcessedBuffer {
            ptr,
            owned: false,
            width: out_w.max(0) as u32,
            height: out_h.max(0) as u32,
            channels: out_colors.max(0) as u32,
            bits_per_sample: if bps == 16 { 16 } else { 8 },
            data_size: out_size.max(0) as usize,
        })
    }

    pub fn process_preview(&self, half_size: bool) -> Result<ProcessedBuffer, String> {
        self.process_custom(half_size, 0, 8)
    }

    pub fn process_preview_16(&self, half_size: bool) -> Result<ProcessedBuffer, String> {
        self.process_custom(half_size, 0, 16)
    }

    pub fn process_full(&self, quality: i32) -> Result<ProcessedBuffer, String> {
        self.process_custom(false, quality, 8)
    }

    pub fn process_full_16(&self, quality: i32) -> Result<ProcessedBuffer, String> {
        self.process_custom(false, quality, 16)
    }
}

impl Drop for RawImage {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle was opened by omaraw_open and is cleanly released here
            unsafe {
                omaraw_close(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

impl RawImage {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Any photo OmaStudio can open: a camera RAW file decoded by LibRaw, or a
/// plain JPEG/TIFF decoded by the `image` crate. Both variants expose the same
/// method surface as the old `RawImage`-only API, so callers that only ever
/// used `RawImage::open(...)` can switch to `PhotoSource::open(...)` and keep
/// every other line unchanged.
pub enum PhotoSource {
    Raw(RawImage),
    Standard(PathBuf),
}

impl PhotoSource {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path_ref = path.as_ref();
        let ext = path_ref
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if standard::is_standard_ext(&ext) {
            crate::security::verify_safe_file(path_ref)
                .map_err(|e| format!("Security check failed for {}: {}", path_ref.display(), e))?;
            Ok(PhotoSource::Standard(path_ref.to_path_buf()))
        } else {
            RawImage::open(path_ref).map(PhotoSource::Raw)
        }
    }

    pub fn get_metadata(&self) -> Result<RawMetadata, String> {
        match self {
            PhotoSource::Raw(r) => r.get_metadata(),
            PhotoSource::Standard(p) => standard::read_metadata(p),
        }
    }

    pub fn extract_thumbnail<P: AsRef<Path>>(&self, dest_path: P) -> Result<(), String> {
        match self {
            PhotoSource::Raw(r) => r.extract_thumbnail(dest_path),
            PhotoSource::Standard(p) => standard::write_thumbnail(p, dest_path.as_ref()),
        }
    }

    pub fn process_preview(&self, half_size: bool) -> Result<ProcessedBuffer, String> {
        match self {
            PhotoSource::Raw(r) => r.process_preview(half_size),
            PhotoSource::Standard(p) => standard::process_preview(p, half_size),
        }
    }

    pub fn process_preview_16(&self, half_size: bool) -> Result<ProcessedBuffer, String> {
        match self {
            PhotoSource::Raw(r) => r.process_preview_16(half_size),
            PhotoSource::Standard(p) => standard::process_preview_16(p, half_size),
        }
    }

    pub fn process_full(&self, quality: i32) -> Result<ProcessedBuffer, String> {
        match self {
            PhotoSource::Raw(r) => r.process_full(quality),
            PhotoSource::Standard(p) => standard::process_full(p, quality),
        }
    }

    pub fn process_full_16(&self, quality: i32) -> Result<ProcessedBuffer, String> {
        match self {
            PhotoSource::Raw(r) => r.process_full_16(quality),
            PhotoSource::Standard(p) => standard::process_full_16(p, quality),
        }
    }
}
