pub mod ai;
pub mod export;
pub mod gdrive;
pub mod icc;
pub mod pipeline;
pub mod raw;
pub mod recipe;
pub mod security;

use ai::{ai_auto_enhance, ai_classify_scene, ai_optimize_for_social};
use export::{export_photo, ExportOptions};
use gdrive::{fetch_remote_raw, is_gdrive_available, list_gdrive_folder};
use pipeline::{process_buffer, process_split_comparison};
use raw::RawImage;
use recipe::{Catalog, CatalogItem, Recipe};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct ResponseWrapper<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<T: Serialize> ResponseWrapper<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

fn print_json<T: Serialize>(resp: &ResponseWrapper<T>) {
    if let Ok(json) = serde_json::to_string(resp) {
        println!("{}", json);
    }
}

#[derive(Serialize)]
struct InspectResult {
    path: String,
    metadata: raw::RawMetadata,
    thumbnail: String,
    recipe: Recipe,
    scene: ai::SceneAnalysis,
}

#[derive(Serialize)]
struct RenderResult {
    image: String,
    histogram: pipeline::histogram::HistogramData,
}

#[derive(Serialize)]
struct ExportResult {
    exported_path: String,
}

struct DaemonCache {
    current_path: Option<String>,
    raw_buffer: Option<Vec<u8>>,
    width: u32,
    height: u32,
    channels: u32,
    metadata: Option<raw::RawMetadata>,
}


#[derive(Serialize)]
struct FolderScanItem {
    name: String,
    path: String,
    thumbnail: String,
    is_remote: bool,
}

fn scan_directory(dir_path: &str) -> Result<Vec<FolderScanItem>, String> {
    let expanded = if let Some(stripped) = dir_path.strip_prefix("~/") {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(stripped)
    } else {
        PathBuf::from(dir_path)
    };

    if !expanded.exists() {
        return Err(format!("Directory does not exist: {}", expanded.display()));
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let thumb_dir = PathBuf::from(home).join(".cache/omastudio/thumbnails");
    let _ = security::ensure_secure_dir(&thumb_dir);

    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&expanded) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ["nef", "nrw", "raf", "cr2", "cr3", "arw", "dng", "rwl", "orf", "rw2"].contains(&ext_lower.as_str()) {
                        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("thumb");
                        let thumb_path = thumb_dir.join(format!("{}.jpg", stem));
                        if !thumb_path.exists() {
                            if let Ok(raw) = RawImage::open(&path) {
                                let _ = raw.extract_thumbnail(&thumb_path);
                            }
                        }

                        items.push(FolderScanItem {
                            name: path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string(),
                            path: path.to_string_lossy().to_string(),
                            thumbnail: thumb_path.to_string_lossy().to_string(),
                            is_remote: false,
                        });
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("OmaStudio Engine v0.1.0 - Lightroom-Grade Photo RAW Engine for Omarchy Linux");
        eprintln!("Usage: omastudio-engine <command> [arguments]");
        eprintln!("Commands:");
        eprintln!("  inspect <raw_file>");
        eprintln!("  render <raw_file> [--recipe <json>] [--split <0.0-1.0>] [--out <dest>]");
        eprintln!("  ai-auto <raw_file>");
        eprintln!("  export <raw_file> [--recipe <json>] [--options <json>]");
        eprintln!("  gdrive list [folder]");
        eprintln!("  gdrive fetch <remote_file>");
        eprintln!("  catalog list");
        eprintln!("  daemon");
        return;
    }

    match args[1].as_str() {
        "inspect" => {
            if args.len() < 3 {
                print_json::<()>(&ResponseWrapper::err("Missing RAW file path"));
                return;
            }
            let raw_path = &args[2];
            match inspect_file(raw_path) {
                Ok(res) => print_json(&ResponseWrapper::ok(res)),
                Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
            }
        }
        "render" => {
            if args.len() < 3 {
                print_json::<()>(&ResponseWrapper::err("Missing RAW file path"));
                return;
            }
            let raw_path = &args[2];
            let mut recipe = Recipe::default();
            let mut split = 0.0f32;
            let mut out_path = "/dev/shm/omastudio_preview.ppm".to_string();

            let mut idx = 3;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--recipe" if idx + 1 < args.len() => {
                        if let Ok(r) = serde_json::from_str::<Recipe>(&args[idx + 1]) {
                            recipe = r;
                        }
                        idx += 2;
                    }
                    "--split" if idx + 1 < args.len() => {
                        split = args[idx + 1].parse().unwrap_or(0.0);
                        idx += 2;
                    }
                    "--out" if idx + 1 < args.len() => {
                        out_path = args[idx + 1].clone();
                        idx += 2;
                    }
                    _ => idx += 1,
                }
            }

            match render_file(raw_path, &recipe, split, &out_path) {
                Ok(res) => print_json(&ResponseWrapper::ok(res)),
                Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
            }
        }
        "ai-auto" => {
            if args.len() < 3 {
                print_json::<()>(&ResponseWrapper::err("Missing RAW file path"));
                return;
            }
            let raw_path = &args[2];
            match run_ai_auto(raw_path) {
                Ok(res) => print_json(&ResponseWrapper::ok(res)),
                Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
            }
        }
        "ai-social" => {
            if args.len() < 3 {
                print_json::<()>(&ResponseWrapper::err("Missing RAW file path"));
                return;
            }
            let raw_path = &args[2];
            let platform = if args.len() >= 4 { &args[3] } else { "ig" };
            match run_ai_social(raw_path, platform) {
                Ok(res) => print_json(&ResponseWrapper::ok(res)),
                Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
            }
        }
        "export" => {
            if args.len() < 3 {
                print_json::<()>(&ResponseWrapper::err("Missing RAW file path"));
                return;
            }
            let raw_path = &args[2];
            let mut recipe = Recipe::default();
            let mut options = ExportOptions::default();

            let mut idx = 3;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--recipe" if idx + 1 < args.len() => {
                        if let Ok(r) = serde_json::from_str::<Recipe>(&args[idx + 1]) {
                            recipe = r;
                        }
                        idx += 2;
                    }
                    "--options" if idx + 1 < args.len() => {
                        if let Ok(o) = serde_json::from_str::<ExportOptions>(&args[idx + 1]) {
                            options = o;
                        }
                        idx += 2;
                    }
                    _ => idx += 1,
                }
            }

            match export_photo(raw_path, &recipe, &options) {
                Ok(dest) => print_json(&ResponseWrapper::ok(ExportResult {
                    exported_path: dest.to_string_lossy().to_string(),
                })),
                Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
            }
        }
        "gdrive" => {
            if args.len() < 3 {
                print_json::<()>(&ResponseWrapper::err("Missing gdrive subcommand"));
                return;
            }
            match args[2].as_str() {
                "check" => {
                    let avail = is_gdrive_available();
                    print_json(&ResponseWrapper::ok(avail));
                }
                "list" => {
                    let folder = if args.len() >= 4 { &args[3] } else { "" };
                    match list_gdrive_folder(folder) {
                        Ok(items) => print_json(&ResponseWrapper::ok(items)),
                        Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
                    }
                }
                "fetch" => {
                    if args.len() < 4 {
                        print_json::<()>(&ResponseWrapper::err("Missing remote path"));
                        return;
                    }
                    match fetch_remote_raw(&args[3]) {
                        Ok(p) => print_json(&ResponseWrapper::ok(p.to_string_lossy().to_string())),
                        Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
                    }
                }
                _ => print_json::<()>(&ResponseWrapper::err("Unknown gdrive subcommand")),
            }
        }
        "icc" => {
            if args.len() < 3 || args[2] == "list" {
                let profiles = icc::list_icc_profiles();
                print_json(&ResponseWrapper::ok(profiles));
            } else {
                print_json::<()>(&ResponseWrapper::err("Unknown icc subcommand"));
            }
        }
        "scan" => {
            let dir = if args.len() >= 3 { &args[2] } else { "." };
            match scan_directory(dir) {
                Ok(items) => print_json(&ResponseWrapper::ok(items)),
                Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
            }
        }
        "catalog" => {
            let cat = Catalog::load();
            print_json(&ResponseWrapper::ok(cat));
        }
        "status" | "--status" => {
            #[derive(Serialize)]
            struct StatusInfo {
                app: String,
                version: String,
                status: String,
                active_mode: String,
                active_theme: String,
                supported_raw_formats: Vec<String>,
                icc_profiles_count: usize,
                gdrive_available: bool,
                features: Vec<String>,
            }
            let icc_count = icc::list_icc_profiles().len();
            let gdrive = is_gdrive_available();
            let active_theme = env::var("HOME")
                .ok()
                .and_then(|h| std::fs::read_to_string(PathBuf::from(h).join(".local/state/omarchy/current/theme.name")).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "default".to_string());

            let status = StatusInfo {
                app: "OmaStudio".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                status: "ready".to_string(),
                active_mode: "studio".to_string(),
                active_theme,
                supported_raw_formats: vec![
                    "RAF".into(), "NEF".into(), "NRW".into(), "CR2".into(), "CR3".into(),
                    "ARW".into(), "SR2".into(), "DNG".into(), "RWL".into(), "ORF".into(),
                    "RW2".into(), "3FR".into(),
                ],
                icc_profiles_count: icc_count,
                gdrive_available: gdrive,
                features: vec![
                    "davinci_3way_color_wheels".into(),
                    "ai_social_media_optimizer".into(),
                    "lightroom_hsl_and_curves".into(),
                    "interactive_crop_and_guides".into(),
                    "icc_color_profile_management".into(),
                    "exif_preservation_and_gps_strip".into(),
                    "touchpad_mac_pinch_rotate_pan".into(),
                    "quickshell_ipc_agent_api".into(),
                    "omarchy_system_theme_sync".into(),
                ],
            };
            print_json(&ResponseWrapper::ok(status));
        }
        "version" | "--version" | "-v" => {
            println!("omastudio-engine v{}", env!("CARGO_PKG_VERSION"));
        }
        "help" | "--help" | "-h" => {
            println!("OmaStudio Engine v{} - Professional Photo RAW Engine for Omarchy Linux", env!("CARGO_PKG_VERSION"));
            println!("\nUsage: omastudio-engine <command> [arguments]\n");
            println!("Commands for Omarchy Agents & CLI:");
            println!("  status                                Output engine and feature status as JSON");
            println!("  inspect <raw_file>                    Parse EXIF, dynamic range, AI scene classification, histogram");
            println!("  ai-auto <raw_file>                    Calculate optimal tone, white balance, and contrast via AI");
            println!("  ai-social <raw_file> [platform]       Calculate social media crop & OLED recipe (ig, story, x, ig_square, fb, yt)");
            println!("  render <raw_file> [options]           Process RAW image to preview PPM/PNG/JPG");
            println!("  export <raw_file> [options]           High-res demosaic & export (jxl, avif, webp, tiff, png, jpg)");
            println!("  icc list                              List all detected system and bundled ICC color profiles");
            println!("  gdrive [check|list <dir>|fetch <f>]   Google Drive cloud RAW operations");
            println!("  scan [dir]                            Scan local folder for supported RAW photos");
            println!("  catalog                               Retrieve persistent photo catalog database");
            println!("  daemon                                Start persistent JSON-over-stdin processing daemon");
            println!("  theme                                 Display current Omarchy system theme and colors");
            println!("  version                               Print engine version");
            println!("  help                                  Show this help reference");
        }
        "theme" => {
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let theme_name_path = PathBuf::from(&home).join(".local/state/omarchy/current/theme.name");
            let colors_path = PathBuf::from(&home).join(".local/state/omarchy/current/theme/colors.toml");

            let theme_name = std::fs::read_to_string(&theme_name_path)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "default".to_string());

            let mut colors = std::collections::HashMap::new();
            if let Ok(content) = std::fs::read_to_string(&colors_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = trimmed.split_once('=') {
                        let key = k.trim().to_lowercase();
                        let val_raw = v.trim();
                        let val_clean = if let Some(first_quote) = val_raw.find('"') {
                            if let Some(second_quote) = val_raw[first_quote + 1..].find('"') {
                                &val_raw[first_quote + 1..first_quote + 1 + second_quote]
                            } else {
                                val_raw.trim_matches('"').trim_matches('\'')
                            }
                        } else if let Some(first_quote) = val_raw.find('\'') {
                            if let Some(second_quote) = val_raw[first_quote + 1..].find('\'') {
                                &val_raw[first_quote + 1..first_quote + 1 + second_quote]
                            } else {
                                val_raw.trim_matches('"').trim_matches('\'')
                            }
                        } else {
                            val_raw.split_whitespace().next().unwrap_or(val_raw)
                        };
                        colors.insert(key, val_clean.to_string());
                    }
                }
            }

            #[derive(Serialize)]
            struct ThemeInfo {
                theme: String,
                colors_path: String,
                colors: std::collections::HashMap<String, String>,
            }

            print_json(&ResponseWrapper::ok(ThemeInfo {
                theme: theme_name,
                colors_path: colors_path.to_string_lossy().to_string(),
                colors,
            }));
        }
        "daemon" => {
            run_daemon();
        }
        cmd => {
            print_json::<()>(&ResponseWrapper::err(format!("Unknown command: {}", cmd)));
        }
    }
}

fn inspect_file(raw_path: &str) -> Result<InspectResult, String> {
    let raw = RawImage::open(raw_path)?;
    let meta = raw.get_metadata()?;

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let thumb_dir = PathBuf::from(home).join(".cache/omastudio/thumbnails");
    security::ensure_secure_dir(&thumb_dir)
        .map_err(|e| format!("Thumbnail dir error: {}", e))?;

    let file_stem = Path::new(raw_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("thumb");
    let thumb_path = thumb_dir.join(format!("{}.jpg", file_stem));

    // Fast thumbnail extraction
    let _ = raw.extract_thumbnail(&thumb_path);

    // Load existing sidecar or create default
    let recipe = Recipe::load_sidecar(raw_path).unwrap_or_default();

    // Fast preview for AI scene analysis
    let preview = raw.process_preview(true)?;
    let scene = ai_classify_scene(preview.as_slice(), preview.width, preview.height, preview.channels, &meta);

    // Update catalog
    let mut catalog = Catalog::load();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    catalog.upsert(CatalogItem {
        id: file_stem.to_string(),
        path: raw_path.to_string(),
        remote_path: None,
        thumbnail_path: thumb_path.to_string_lossy().to_string(),
        metadata: meta.clone(),
        rating: 0,
        color_label: String::new(),
        flag: "unflagged".to_string(),
        recipe: recipe.clone(),
        updated_at: now,
    });
    let _ = catalog.save();

    Ok(InspectResult {
        path: raw_path.to_string(),
        metadata: meta,
        thumbnail: thumb_path.to_string_lossy().to_string(),
        recipe,
        scene,
    })
}

fn render_file(raw_path: &str, recipe: &Recipe, split: f32, out_path: &str) -> Result<RenderResult, String> {
    let raw = RawImage::open(raw_path)?;
    let preview = raw.process_preview(true)?;

    let (final_buf, hist) = if split > 0.001 {
        process_split_comparison(
            preview.as_slice(),
            preview.width,
            preview.height,
            preview.channels,
            recipe,
            split,
        )
    } else {
        process_buffer(
            preview.as_slice(),
            preview.width,
            preview.height,
            preview.channels,
            recipe,
        )
    };

    // Write output image (PPM or PNG/JPG)
    let dest_path = Path::new(out_path);
    if let Some(parent) = dest_path.parent() {
        let _ = security::ensure_secure_dir(parent);
    }

    if out_path.ends_with(".ppm") {
        let mut f = std::fs::File::create(dest_path)
            .map_err(|e| format!("Failed to create PPM: {}", e))?;
        write!(f, "P6\n{} {}\n255\n", preview.width, preview.height)
            .map_err(|e| format!("Failed to write PPM header: {}", e))?;
        f.write_all(&final_buf)
            .map_err(|e| format!("Failed to write PPM body: {}", e))?;
    } else {
        let img: image::ImageBuffer<image::Rgb<u8>, Vec<u8>> = image::ImageBuffer::from_raw(
            preview.width,
            preview.height,
            final_buf,
        ).ok_or_else(|| "Failed to create ImageBuffer".to_string())?;
        img.save(dest_path)
            .map_err(|e| format!("Failed to save preview: {}", e))?;
    }

    Ok(RenderResult {
        image: out_path.to_string(),
        histogram: hist,
    })
}

fn run_ai_auto(raw_path: &str) -> Result<InspectResult, String> {
    let raw = RawImage::open(raw_path)?;
    let meta = raw.get_metadata()?;
    let preview = raw.process_preview(true)?;

    let auto_recipe = ai_auto_enhance(preview.as_slice(), preview.width, preview.height, preview.channels, &meta);
    let scene = ai_classify_scene(preview.as_slice(), preview.width, preview.height, preview.channels, &meta);

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let thumb_path = PathBuf::from(home)
        .join(".cache/omastudio/thumbnails")
        .join(format!("{}.jpg", Path::new(raw_path).file_stem().and_then(|s| s.to_str()).unwrap_or("thumb")));

    Ok(InspectResult {
        path: raw_path.to_string(),
        metadata: meta,
        thumbnail: thumb_path.to_string_lossy().to_string(),
        recipe: auto_recipe,
        scene,
    })
}

fn run_ai_social(raw_path: &str, platform: &str) -> Result<ai::SocialOptimizationResult, String> {
    let raw = RawImage::open(raw_path)?;
    let preview = raw.process_preview(true)?;
    let sidecar = Recipe::load_sidecar(raw_path).unwrap_or_default();
    let result = ai_optimize_for_social(
        preview.as_slice(),
        preview.width,
        preview.height,
        preview.channels,
        platform,
        &sidecar,
    );
    Ok(result)
}

#[derive(Deserialize)]
struct DaemonCommand {
    cmd: String, // "load", "adjust", "ai", "save_recipe", "exit"
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    recipe: Option<Recipe>,
    #[serde(default)]
    split: Option<f32>,
    #[serde(default)]
    out: Option<String>,
}

fn run_daemon() {
    let cache = Arc::new(Mutex::new(DaemonCache {
        current_path: None,
        raw_buffer: None,
        width: 0,
        height: 0,
        channels: 0,
        metadata: None,
    }));

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line_str = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line_str.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cmd_obj: DaemonCommand = match serde_json::from_str(trimmed) {
            Ok(c) => c,
            Err(e) => {
                print_json::<()>(&ResponseWrapper::err(format!("Invalid JSON command: {}", e)));
                continue;
            }
        };

        match cmd_obj.cmd.as_str() {
            "load" => {
                let p = match cmd_obj.path {
                    Some(ref path) => path,
                    None => {
                        print_json::<()>(&ResponseWrapper::err("Missing path in load command"));
                        continue;
                    }
                };

                match RawImage::open(p) {
                    Ok(raw) => {
                        let meta = raw.get_metadata().unwrap_or(raw::RawMetadata {
                            width: 0,
                            height: 0,
                            raw_width: 0,
                            raw_height: 0,
                            make: String::new(),
                            model: String::new(),
                            lens: String::new(),
                            iso: 0.0,
                            shutter: 0.0,
                            aperture: 0.0,
                            focal_length: 0.0,
                            timestamp: 0,
                            cam_mul: [1.0, 1.0, 1.0, 1.0],
                        });

                        match raw.process_preview(true) {
                            Ok(prev) => {
                                let mut c = cache.lock().unwrap();
                                c.current_path = Some(p.clone());
                                c.width = prev.width;
                                c.height = prev.height;
                                c.channels = prev.channels;
                                c.raw_buffer = Some(prev.to_vec());
                                c.metadata = Some(meta.clone());

                                let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
                                let thumb_path = PathBuf::from(home)
                                    .join(".cache/omastudio/thumbnails")
                                    .join(format!("{}.jpg", Path::new(p).file_stem().and_then(|s| s.to_str()).unwrap_or("thumb")));
                                let _ = raw.extract_thumbnail(&thumb_path);

                                let sidecar = Recipe::load_sidecar(p).unwrap_or_default();
                                let scene = ai_classify_scene(prev.as_slice(), prev.width, prev.height, prev.channels, &meta);

                                print_json(&ResponseWrapper::ok(InspectResult {
                                    path: p.clone(),
                                    metadata: meta,
                                    thumbnail: thumb_path.to_string_lossy().to_string(),
                                    recipe: sidecar,
                                    scene,
                                }));
                            }
                            Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
                        }
                    }
                    Err(e) => print_json::<()>(&ResponseWrapper::err(e)),
                }
            }
            "adjust" => {
                let c = cache.lock().unwrap();
                if c.raw_buffer.is_none() {
                    print_json::<()>(&ResponseWrapper::err("No RAW image currently loaded in daemon cache"));
                    continue;
                }

                let buffer = c.raw_buffer.as_ref().unwrap();
                let recipe = cmd_obj.recipe.unwrap_or_default();
                let split = cmd_obj.split.unwrap_or(0.0);
                let out_dest = cmd_obj.out.unwrap_or_else(|| "/dev/shm/omastudio_viewport.ppm".to_string());

                let (final_buf, hist) = if split > 0.001 {
                    process_split_comparison(
                        buffer,
                        c.width,
                        c.height,
                        c.channels,
                        &recipe,
                        split,
                    )
                } else {
                    process_buffer(
                        buffer,
                        c.width,
                        c.height,
                        c.channels,
                        &recipe,
                    )
                };

                let dest_path = Path::new(&out_dest);
                if out_dest.ends_with(".ppm") {
                    if let Ok(mut f) = std::fs::File::create(dest_path) {
                        let _ = write!(f, "P6\n{} {}\n255\n", c.width, c.height);
                        let _ = f.write_all(&final_buf);
                    }
                } else {
                    if let Some(img) = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(
                        c.width,
                        c.height,
                        final_buf,
                    ) {
                        let _ = img.save(dest_path);
                    }
                }

                print_json(&ResponseWrapper::ok(RenderResult {
                    image: out_dest,
                    histogram: hist,
                }));
            }
            "save_recipe" => {
                let c = cache.lock().unwrap();
                if let (Some(ref p), Some(ref recipe)) = (&c.current_path, &cmd_obj.recipe) {
                    match recipe.save_sidecar(p) {
                        Ok(sidecar_path) => print_json(&ResponseWrapper::ok(sidecar_path.to_string_lossy().to_string())),
                        Err(e) => print_json::<()>(&ResponseWrapper::err(format!("Failed to save sidecar: {}", e))),
                    }
                } else {
                    print_json::<()>(&ResponseWrapper::err("No image loaded or recipe missing"));
                }
            }
            "exit" => {
                print_json(&ResponseWrapper::ok("Daemon exiting"));
                break;
            }
            _ => {
                print_json::<()>(&ResponseWrapper::err("Unknown daemon command"));
            }
        }
    }
}
