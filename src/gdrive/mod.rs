use crate::security::{ensure_secure_dir, run_bounded_command, secure_command};
use serde::{Deserialize, Serialize};
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
    let target = if clean_sub.is_empty() {
        "gdrive:".to_string()
    } else {
        format!("gdrive:{}", clean_sub)
    };

    let mut cmd = secure_command("rclone");
    cmd.arg("lsjson")
        .arg("--max-depth")
        .arg("1")
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

/// Downloads a remote RAW file into local secure cache
pub fn fetch_remote_raw(remote_path: &str) -> Result<PathBuf, String> {
    let cache_dir = default_gdrive_cache_dir();
    ensure_secure_dir(&cache_dir)
        .map_err(|e| format!("Could not create secure cache directory: {}", e))?;

    let clean_path = remote_path.trim_start_matches('/');
    let target_remote = format!("gdrive:{}", clean_path);

    let filename = Path::new(clean_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Invalid remote file path".to_string())?;

    let local_dest = cache_dir.join(filename);

    // If already cached and valid size, return cached file
    if local_dest.exists() {
        if let Ok(meta) = std::fs::metadata(&local_dest) {
            if meta.len() > 1024 {
                return Ok(local_dest);
            }
        }
    }

    let mut cmd = secure_command("rclone");
    cmd.arg("copyto")
        .arg(&target_remote)
        .arg(&local_dest);

    let (code, _, stderr) = run_bounded_command(cmd, Duration::from_secs(120))
        .map_err(|e| format!("Failed to download from Google Drive: {}", e))?;

    if code != 0 {
        return Err(format!(
            "rclone download failed: {}",
            String::from_utf8_lossy(&stderr)
        ));
    }

    // Automatically extract thumbnail for the fetched photo
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let thumb_dir = PathBuf::from(&home).join(".cache/omastudio/thumbnails");
    let _ = ensure_secure_dir(&thumb_dir);
    let stem = Path::new(&filename).file_stem().and_then(|s| s.to_str()).unwrap_or("thumb");
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
