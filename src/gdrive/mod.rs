use crate::security::{
    run_bounded_command, run_bounded_command_stream_to_file, secure_command,
    SecureDir, SecureDirCleanupGuard,
};
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_remote: bool,
    pub thumbnail: String,
    pub size: i64,
}

pub fn default_gdrive_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cache/omastudio/gdrive")
}

/// Checks if rclone has configured gdrive remote
pub fn is_gdrive_available() -> bool {
    let mut cmd = secure_command("rclone");
    cmd.arg("listremotes");
    if let Ok((code, stdout, _)) = run_bounded_command(cmd, Duration::from_secs(5)) {
        if code == 0 {
            let out = String::from_utf8_lossy(&stdout);
            return out.contains("gdrive:");
        }
    }
    false
}

/// Lists files and subfolders in Google Drive (e.g. "Photos" or root)
pub fn list_gdrive_folder(subfolder: &str) -> Result<Vec<RemoteItem>, String> {
    let clean_sub = subfolder.trim_matches('/');
    if clean_sub.contains("..") || clean_sub.contains('\0') {
        return Err("Invalid subfolder path: directory traversal or null bytes detected".to_string());
    }
    let target = if clean_sub.is_empty() {
        "gdrive:".to_string()
    } else {
        format!("gdrive:{}", clean_sub)
    };

    let mut cmd = secure_command("rclone");
    cmd.arg("lsjson")
        .arg("--max-depth")
        .arg("1")
        .arg("--")
        .arg(&target);

    let (code, stdout, stderr) = run_bounded_command(cmd, Duration::from_secs(35))
        .map_err(|e| format!("Failed to execute rclone: {}", e))?;

    if code != 0 {
        return Err(format!(
            "rclone exited with code {}: {}",
            code,
            String::from_utf8_lossy(&stderr)
        ));
    }

    #[derive(Deserialize)]
    struct RcloneEntry {
        #[serde(rename = "Path")]
        path: String,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Size")]
        size: i64,
        #[serde(rename = "IsDir")]
        is_dir: bool,
    }

    let entries: Vec<RcloneEntry> = serde_json::from_slice(&stdout)
        .map_err(|e| format!("Failed to parse rclone json: {}", e))?;

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let thumb_dir = PathBuf::from(&home).join(".cache/omastudio/thumbnails");

    let mut items = Vec::new();
    for entry in entries {
        let full_remote_path = if clean_sub.is_empty() {
            entry.path.clone()
        } else {
            format!("{}/{}", clean_sub, entry.path)
        };

        // Filter for RAW formats or directories
        let is_raw = entry.is_dir || {
            let lower = entry.name.to_lowercase();
            lower.ends_with(".nef")
                || lower.ends_with(".nrw")
                || lower.ends_with(".raf")
                || lower.ends_with(".cr2")
                || lower.ends_with(".cr3")
                || lower.ends_with(".arw")
                || lower.ends_with(".dng")
                || lower.ends_with(".rwl")
                || lower.ends_with(".orf")
                || lower.ends_with(".rw2")
        };

        if is_raw {
            let thumb = if !entry.is_dir {
                let stem = Path::new(&entry.name).file_stem().and_then(|s| s.to_str()).unwrap_or("thumb");
                let thumb_candidate = thumb_dir.join(format!("{}.jpg", stem));
                if thumb_candidate.exists() {
                    thumb_candidate.to_string_lossy().to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            items.push(RemoteItem {
                name: entry.name,
                path: full_remote_path,
                is_dir: entry.is_dir,
                is_remote: true,
                thumbnail: thumb,
                size: entry.size,
            });
        }
    }

    // Sort folders first, then files alphabetically
    items.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(items)
}

pub const MAX_RAW_DOWNLOAD_BYTES: usize = 250 * 1024 * 1024; // 250 MiB hard limit

/// Downloads a remote RAW file into local secure cache with strict byte limits,
/// RAII partial file cleanup, descriptor-bound directory operations, and validation before atomic publish.
pub fn fetch_remote_raw(remote_path: &str) -> Result<PathBuf, String> {
    if remote_path.contains("..") || remote_path.contains('\0') {
        return Err("Invalid remote path: directory traversal or null bytes detected".to_string());
    }

    let cache_dir = default_gdrive_cache_dir();
    let secure_dir = SecureDir::open_or_create_hierarchy(&cache_dir)
        .map_err(|e| format!("Could not create secure cache directory hierarchy: {}", e))?;

    let clean_path = remote_path.trim_start_matches('/');
    let target_remote = format!("gdrive:{}", clean_path);

    let filename = Path::new(clean_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Invalid remote file path".to_string())?;

    if filename.is_empty() || filename.starts_with('.') {
        return Err("Invalid file name".to_string());
    }

    let local_dest = cache_dir.join(filename);
    let c_dest_name = CString::new(filename.as_bytes())
        .map_err(|e| format!("Invalid filename string: {}", e))?;

    // If cached, verify it via open descriptor: regular file, owned by user, mode 0600, size limit, LibRaw valid.
    // Purge corrupted/partial cache entries descriptor-bound.
    if let Ok(cached_file) = secure_dir.open_existing_file_ro(&c_dest_name) {
        let is_valid = (|| -> Option<()> {
            let len = crate::security::verify_secure_open_file(&cached_file, MAX_RAW_DOWNLOAD_BYTES).ok()?;
            if len < 1024 {
                return None;
            }
            if crate::raw::RawImage::open_from_file(&cached_file).is_err() {
                return None;
            }
            Some(())
        })()
        .is_some();

        if is_valid {
            return Ok(local_dest);
        } else {
            // Remove corrupted or partial cached file descriptor-bound
            let _ = secure_dir.unlink_file(&c_dest_name);
        }
    }

    // Create private staging file descriptor-bound with mode 0600 and O_NOFOLLOW
    let (mut staging_file, c_staging_name) = secure_dir
        .open_staging_file(".tmp_gdrive", filename)
        .map_err(|e| format!("Failed to create private staging file in cache directory: {}", e))?;

    let mut cleanup_guard = SecureDirCleanupGuard {
        secure_dir: &secure_dir,
        filename: c_staging_name.clone(),
        active: true,
    };

    let mut cmd = secure_command("rclone");
    cmd.arg("cat")
        .arg("--")
        .arg(&target_remote);

    run_bounded_command_stream_to_file(
        cmd,
        &mut staging_file,
        Duration::from_secs(180),
        MAX_RAW_DOWNLOAD_BYTES,
    )
    .map_err(|e| format!("Failed to download from Google Drive: {}", e))?;

    // Verify open staging file descriptor: regular file, owned by current user, mode 0600, within size bounds
    crate::security::verify_secure_open_file(&staging_file, MAX_RAW_DOWNLOAD_BYTES)
        .map_err(|e| format!("Staging file security verification failed: {}", e))?;

    // Verify RAW image integrity directly via open file descriptor (no pathname swap window)
    crate::raw::RawImage::open_from_file(&staging_file)
        .map_err(|e| format!("Downloaded file is not a valid RAW image: {}", e))?;

    // Ensure staging file contents are flushed and synced to disk
    staging_file
        .sync_all()
        .map_err(|e| format!("Failed to sync staging file: {}", e))?;

    // Atomically publish into destination cache path descriptor-bound
    secure_dir
        .rename_file(&c_staging_name, &c_dest_name)
        .map_err(|e| format!("Failed to atomically publish downloaded file into cache: {}", e))?;

    cleanup_guard.active = false;
    drop(staging_file);

    // Automatically extract thumbnail for the fetched photo
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let thumb_dir = PathBuf::from(&home).join(".cache/omastudio/thumbnails");
    let _ = SecureDir::open_or_create_hierarchy(&thumb_dir);
    let stem = Path::new(&filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("thumb");
    let thumb_path = thumb_dir.join(format!("{}.jpg", stem));
    if !thumb_path.exists() {
        if let Ok(raw) = crate::raw::RawImage::open(&local_dest) {
            let _ = raw.extract_thumbnail(&thumb_path);
        }
    }

    Ok(local_dest)
}

/// Uploads an exported photo directly to Google Drive
pub fn upload_export_to_gdrive(local_path: &Path, remote_dest_folder: &str) -> Result<String, String> {
    if !local_path.exists() {
        return Err("Export file does not exist locally".to_string());
    }

    let clean_dest = remote_dest_folder.trim_matches('/');
    if clean_dest.contains("..") || clean_dest.contains('\0') {
        return Err("Invalid destination folder: traversal detected".to_string());
    }
    let target = if clean_dest.is_empty() {
        "gdrive:Photos/Exports".to_string()
    } else {
        format!("gdrive:{}", clean_dest)
    };

    let filename = local_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Invalid filename".to_string())?;

    let mut cmd = secure_command("rclone");
    cmd.arg("copy")
        .arg("--")
        .arg(local_path)
        .arg(&target);

    let (code, _, stderr) = run_bounded_command(cmd, Duration::from_secs(60))
        .map_err(|e| format!("Failed to upload to Google Drive: {}", e))?;

    if code != 0 {
        return Err(format!(
            "rclone upload failed: {}",
            String::from_utf8_lossy(&stderr)
        ));
    }

    Ok(format!("{}/{}", target, filename))
}
