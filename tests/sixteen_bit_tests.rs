use omastudio_engine::export::{export_photo, ExportOptions};
use omastudio_engine::pipeline::{
    process_buffer_16_to_8, process_buffer_16_to_16, process_split_comparison_16_to_8,
};
use omastudio_engine::raw::RawImage;
use omastudio_engine::recipe::Recipe;
use std::path::Path;

#[test]
fn test_16bit_pipeline_precision_and_dynamic_range() {
    let width = 64;
    let height = 64;
    let mut buffer16 = vec![0u16; width * height * 3];

    // Create 16-bit smooth gradient (0 to 65535)
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 3;
            buffer16[idx] = ((x * 65535) / width) as u16;
            buffer16[idx + 1] = ((y * 65535) / height) as u16;
            buffer16[idx + 2] = 32768; // Mid gray in blue
        }
    }

    let recipe = Recipe {
        exposure: 1.0,
        shadows: 40.0,
        highlights: -30.0,
        ..Default::default()
    };

    // Test 16-bit to 8-bit viewport rendering
    let (buf8, hist8) = process_buffer_16_to_8(
        &buffer16,
        width as u32,
        height as u32,
        3,
        &recipe,
    );
    assert_eq!(buf8.len(), width * height * 3);
    assert!(hist8.max_count > 0);

    // Test 16-bit to 16-bit export rendering
    let (buf16_out, hist16) = process_buffer_16_to_16(
        &buffer16,
        width as u32,
        height as u32,
        3,
        &recipe,
    );
    assert_eq!(buf16_out.len(), buffer16.len());
    assert!(hist16.max_count > 0);

    // Test Before/After split comparison
    let (split_buf, _) = process_split_comparison_16_to_8(
        &buffer16,
        width as u32,
        height as u32,
        3,
        &recipe,
        0.5,
    );
    assert_eq!(split_buf.len(), width * height * 3);
}

#[test]
fn test_real_raw_16bit_loading_and_tiff_export() {
    let sample_raw = Path::new("/home/ozdil/Downloads/yurt/_DSF2246.RAF");
    if !sample_raw.exists() {
        eprintln!("Sample RAW not present on system, skipping real RAW integration test");
        return;
    }

    let raw = RawImage::open(sample_raw).expect("Should open sample Fujifilm RAW");
    let preview16 = raw.process_preview_16(true).expect("Should process 16-bit preview");
    assert_eq!(preview16.bits_per_sample, 16);
    assert!(preview16.width > 0);
    assert!(preview16.height > 0);
    assert_eq!(preview16.data_size, (preview16.width * preview16.height * preview16.channels * 2) as usize);

    let u16_slice = preview16.as_slice_u16();
    assert_eq!(u16_slice.len(), (preview16.width * preview16.height * preview16.channels) as usize);

    // Test 16-bit viewport pipeline processing
    let recipe = Recipe::default();
    let (preview8, hist) = process_buffer_16_to_8(
        u16_slice,
        preview16.width,
        preview16.height,
        preview16.channels,
        &recipe,
    );
    assert_eq!(preview8.len(), (preview16.width * preview16.height * preview16.channels) as usize);
    assert!(hist.max_count > 0);

    // Test 16-bit TIFF Export
    let tmp_dir = std::env::temp_dir().join("omastudio_16bit_test");
    let export_opts = ExportOptions {
        format: "tiff".to_string(),
        scale_percent: 25, // Scale down for fast test
        output_dir: tmp_dir.to_string_lossy().to_string(),
        preserve_exif: false,
        icc_profile: None,
        ..Default::default()
    };

    let exported = export_photo(sample_raw, &recipe, &export_opts).expect("Export to 16-bit TIFF should succeed");
    assert!(exported.exists());

    // Verify it is encoded as 16-bit RGB
    let loaded = image::open(&exported).expect("Should open exported TIFF");
    assert_eq!(loaded.color(), image::ColorType::Rgb16);

    let _ = std::fs::remove_file(exported);
    let _ = std::fs::remove_dir(tmp_dir);
}
