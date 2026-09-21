//! Decodes plain (non-RAW) photo formats — JPEG and TIFF — through the `image`
//! crate instead of LibRaw, producing the same `ProcessedBuffer` shape the RAW
//! pipeline expects so every downstream consumer (color pipeline, AI, export,
//! thumbnails) is unaware of which decoder actually ran.

use super::{ProcessedBuffer, RawMetadata};
use std::path::Path;

/// Extensions this module can decode. Kept separate from the RAW extension list
/// in `gdrive::is_raw`/`scan_local_folder` so both call sites can build a combined
/// "openable" set without this module needing to know about RAW formats.
pub fn is_standard_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "tif" | "tiff"
    )
}

fn decode(path: &Path) -> Result<image::DynamicImage, String> {
    crate::security::verify_safe_file(path)
        .map_err(|e| format!("Security check failed for {}: {}", path.display(), e))?;
    image::open(path).map_err(|e| format!("Failed to decode {}: {}", path.display(), e))
}

/// Reads EXIF (camera make/model/lens/ISO/shutter/aperture/timestamp) when present.
/// JPEG and TIFF both carry EXIF via the same TIFF-based tag structure that
/// `kamadak-exif` understands directly from the file's own container, so no
/// separate demux step is needed for either format.
pub fn read_metadata(path: &Path) -> Result<RawMetadata, String> {
    let (width, height) = image::image_dimensions(path)
        .map_err(|e| format!("Failed to read dimensions for {}: {}", path.display(), e))?;

    let mut meta = RawMetadata {
        width,
        height,
        raw_width: width,
        raw_height: height,
        make: String::new(),
        model: String::new(),
        lens: String::new(),
        iso: 0.0,
        shutter: 0.0,
        aperture: 0.0,
        focal_length: 0.0,
        timestamp: 0,
        cam_mul: [1.0, 1.0, 1.0, 1.0],
    };

    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open {} for EXIF read: {}", path.display(), e))?;
    let mut bufreader = std::io::BufReader::new(&file);
    let exifreader = exif::Reader::new();
    let Ok(fields) = exifreader.read_from_container(&mut bufreader) else {
        // No EXIF segment (common for web-exported JPEGs) — dimensions-only metadata is fine.
        return Ok(meta);
    };

    use exif::{In, Tag, Value};

    let as_string = |tag: Tag| -> Option<String> {
        fields.get_field(tag, In::PRIMARY).and_then(|f| match &f.value {
            // Extract the raw ASCII bytes directly rather than going through
            // display_value(), which wraps string fields in literal quote marks.
            Value::Ascii(chunks) => chunks.first().map(|bytes| {
                String::from_utf8_lossy(bytes)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string()
            }),
            _ => None,
        }).filter(|s| !s.is_empty())
    };
    let as_f32 = |tag: Tag| -> Option<f32> {
        fields.get_field(tag, In::PRIMARY).and_then(|f| match &f.value {
            Value::Rational(v) if !v.is_empty() => Some(v[0].to_f32()),
            Value::SRational(v) if !v.is_empty() => Some(v[0].to_f32()),
            Value::Short(v) if !v.is_empty() => Some(v[0] as f32),
            Value::Long(v) if !v.is_empty() => Some(v[0] as f32),
            _ => None,
        })
    };

    if let Some(v) = as_string(Tag::Make) {
        meta.make = v;
    }
    if let Some(v) = as_string(Tag::Model) {
        meta.model = v;
    }
    if let Some(v) = as_string(Tag::LensModel) {
        meta.lens = v;
    }
    if let Some(v) = as_f32(Tag::PhotographicSensitivity) {
        meta.iso = v;
    }
    if let Some(v) = as_f32(Tag::ExposureTime) {
        meta.shutter = v;
    }
    if let Some(v) = as_f32(Tag::FNumber) {
        meta.aperture = v;
    }
    if let Some(v) = as_f32(Tag::FocalLength) {
        meta.focal_length = v;
    }
    if let Some(dt) = as_string(Tag::DateTimeOriginal).or_else(|| as_string(Tag::DateTime)) {
        // EXIF datetime is "YYYY:MM:DD HH:MM:SS" with no timezone; treat as local/UTC-naive,
        // matching the precision LibRaw's own timestamp extraction provides.
        if let Ok(parsed) = chrono_naive_to_epoch(&dt) {
            meta.timestamp = parsed;
        }
    }

    Ok(meta)
}

/// Minimal "YYYY:MM:DD HH:MM:SS" -> unix epoch seconds parser, avoiding a chrono
/// dependency for a single field that's only ever informational in the UI.
fn chrono_naive_to_epoch(s: &str) -> Result<i64, ()> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return Err(());
    }
    let n = |r: std::ops::Range<usize>| -> Result<i64, ()> {
        std::str::from_utf8(&bytes[r]).ok().and_then(|s| s.parse().ok()).ok_or(())
    };
    let (year, month, day) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hour, min, sec) = (n(11..13)?, n(14..16)?, n(17..19)?);

    // Days since epoch via a civil-calendar algorithm (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Ok(days * 86400 + hour * 3600 + min * 60 + sec)
}

/// Writes a resized JPEG thumbnail, matching the on-disk contract of
/// `RawImage::extract_thumbnail` (a plain JPEG file at `dest`).
pub fn write_thumbnail(path: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        crate::security::ensure_secure_dir(parent)
            .map_err(|e| format!("Failed to ensure thumbnail directory: {}", e))?;
    }
    let img = decode(path)?;
    let thumb = img.thumbnail(512, 512);
    thumb
        .to_rgb8()
        .save_with_format(dest, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to write thumbnail: {}", e))
}

/// Decodes to an 8-bit RGB `ProcessedBuffer`. `half_size` downsamples by 2 up
/// front (cheap preview), mirroring LibRaw's half-size decode mode.
pub fn process_preview(path: &Path, half_size: bool) -> Result<ProcessedBuffer, String> {
    let img = decode(path)?;
    let img = if half_size {
        let (w, h) = (img.width().max(2) / 2, img.height().max(2) / 2);
        img.resize(w, h, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    Ok(ProcessedBuffer::from_owned(rgb.into_raw(), w, h, 3, 8))
}

/// Decodes to a 16-bit RGB `ProcessedBuffer`. JPEG has no native 16-bit samples,
/// so this simply widens 8-bit values (`v << 8 | v`) to fill the 16-bit range —
/// TIFF sources that actually carry 16-bit samples get their real precision back
/// via `to_rgb16()`.
pub fn process_preview_16(path: &Path, half_size: bool) -> Result<ProcessedBuffer, String> {
    let img = decode(path)?;
    let img = if half_size {
        let (w, h) = (img.width().max(2) / 2, img.height().max(2) / 2);
        img.resize(w, h, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgb16 = img.to_rgb16();
    let (w, h) = rgb16.dimensions();
    let mut bytes = Vec::with_capacity(rgb16.as_raw().len() * 2);
    for v in rgb16.as_raw() {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    Ok(ProcessedBuffer::from_owned(bytes, w, h, 3, 16))
}

pub fn process_full(path: &Path, _quality: i32) -> Result<ProcessedBuffer, String> {
    process_preview(path, false)
}

pub fn process_full_16(path: &Path, _quality: i32) -> Result<ProcessedBuffer, String> {
    process_preview_16(path, false)
}
