pub mod color_grading;
pub mod detail;
pub mod histogram;
pub mod presence;
pub mod tone;
pub mod white_balance;

use crate::recipe::Recipe;
use histogram::{compute_histogram, compute_histogram_16, HistogramData};
use rayon::prelude::*;

/// Processes an 8-bit linear RGB buffer according to the given Recipe using parallel multi-threading
pub fn process_buffer(
    input: &[u8],
    width: u32,
    height: u32,
    channels: u32,
    recipe: &Recipe,
) -> (Vec<u8>, HistogramData) {
    let ch = channels as usize;
    let w = width as usize;
    let h = height as usize;
    let num_pixels = w * h;

    let (wb_r, wb_g, wb_b) = white_balance::kelvin_to_rgb_multipliers(
        recipe.wb_temperature,
        recipe.wb_tint,
    );
    let exp_factor = 2.0f32.powf(recipe.exposure);

    // Intermediate float buffer for multi-stage 16-bit precision processing
    let mut f32_buf = vec![0.0f32; num_pixels * 3];

    // PASS 1: Point Operations (White Balance, Exposure, Tone, Color Wheels, HSL Mixer)
    f32_buf
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y_idx, row_f32)| {
            let in_row = &input[y_idx * w * ch..(y_idx + 1) * w * ch];

            for x in 0..w {
                let px_in = x * ch;
                let px_out = x * 3;

                let r_norm = (in_row[px_in] as f32 / 255.0) * wb_r;
                let g_norm = (in_row[px_in + 1] as f32 / 255.0) * wb_g;
                let b_norm = (in_row[px_in + 2] as f32 / 255.0) * wb_b;

                // 1. Tone & Dynamic range (Chroma-preserving, smooth highlight knee & shadow toe)
                let (r1, g1, b1) = tone::apply_tone_pixel(r_norm, g_norm, b_norm, recipe, exp_factor);

                // 2. DaVinci Resolve 3-Way Color Wheels
                let (r2, g2, b2) = color_grading::apply_color_wheels(r1, g1, b1, recipe);

                // 3. 8-Band HSL Color Mixer (Perceptual Luminance Anchor)
                let (r3, g3, b3) = color_grading::apply_hsl_mixer(r2, g2, b2, recipe);

                row_f32[px_out] = r3;
                row_f32[px_out + 1] = g3;
                row_f32[px_out + 2] = b3;
            }
        });

    // PASS 2: Presence Engine (Texture, Clarity, Dehaze)
    presence::apply_presence(
        &mut f32_buf,
        w,
        h,
        recipe.clarity,
        recipe.texture,
        recipe.dehaze,
    );

    // PASS 3: Detail & Optics Engine (Sharpness, Luma Denoise, Chroma Denoise)
    detail::apply_detail(
        &mut f32_buf,
        w,
        h,
        recipe.sharpness,
        recipe.denoise_lum,
        recipe.denoise_col,
    );

    // PASS 4: Lens Distortion Correction (if enabled)
    if recipe.lens_distortion.abs() >= 0.1 {
        f32_buf = detail::apply_lens_distortion(&f32_buf, w, h, 3, recipe.lens_distortion);
    }

    // PASS 5: Defringe, Vignette & Final Quantization to 8-bit output
    let mut output = vec![0u8; num_pixels * ch];
    output
        .par_chunks_mut(w * ch)
        .enumerate()
        .for_each(|(y_idx, row)| {
            let in_f32 = &f32_buf[y_idx * w * 3..(y_idx + 1) * w * 3];
            let y = y_idx as u32;

            for x in 0..w {
                let px_out = x * ch;
                let px_f32 = x * 3;

                let (r4, g4, b4) = detail::apply_defringe(
                    in_f32[px_f32],
                    in_f32[px_f32 + 1],
                    in_f32[px_f32 + 2],
                    recipe.defringe,
                );

                let (r5, g5, b5) = detail::apply_vignette(
                    r4,
                    g4,
                    b4,
                    x as u32,
                    y,
                    width,
                    height,
                    recipe.vignette,
                );

                row[px_out] = (r5.clamp(0.0, 1.0) * 255.0) as u8;
                row[px_out + 1] = (g5.clamp(0.0, 1.0) * 255.0) as u8;
                row[px_out + 2] = (b5.clamp(0.0, 1.0) * 255.0) as u8;

                if ch == 4 {
                    let orig_alpha = input[y_idx * w * ch + px_out + 3];
                    row[px_out + 3] = orig_alpha;
                }
            }
        });

    let hist = compute_histogram(&output, ch);
    (output, hist)
}

/// Processes a 16-bit linear RGB buffer (0..65535) and outputs an 8-bit RGB buffer for high-speed viewport rendering
pub fn process_buffer_16_to_8(
    input: &[u16],
    width: u32,
    height: u32,
    channels: u32,
    recipe: &Recipe,
) -> (Vec<u8>, HistogramData) {
    let ch = channels as usize;
    let w = width as usize;
    let h = height as usize;
    let num_pixels = w * h;

    let (wb_r, wb_g, wb_b) = white_balance::kelvin_to_rgb_multipliers(
        recipe.wb_temperature,
        recipe.wb_tint,
    );
    let exp_factor = 2.0f32.powf(recipe.exposure);

    // Intermediate float buffer for multi-stage 16-bit precision processing
    let mut f32_buf = vec![0.0f32; num_pixels * 3];

    // PASS 1: Point Operations (White Balance, Exposure, Tone, Color Wheels, HSL Mixer)
    f32_buf
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y_idx, row_f32)| {
            let in_row = &input[y_idx * w * ch..(y_idx + 1) * w * ch];

            for x in 0..w {
                let px_in = x * ch;
                let px_out = x * 3;

                let r_norm = (in_row[px_in] as f32 / 65535.0) * wb_r;
                let g_norm = (in_row[px_in + 1] as f32 / 65535.0) * wb_g;
                let b_norm = (in_row[px_in + 2] as f32 / 65535.0) * wb_b;

                // 1. Tone & Dynamic range
                let (r1, g1, b1) = tone::apply_tone_pixel(r_norm, g_norm, b_norm, recipe, exp_factor);

                // 2. DaVinci Resolve 3-Way Color Wheels
                let (r2, g2, b2) = color_grading::apply_color_wheels(r1, g1, b1, recipe);

                // 3. 8-Band HSL Color Mixer
                let (r3, g3, b3) = color_grading::apply_hsl_mixer(r2, g2, b2, recipe);

                row_f32[px_out] = r3;
                row_f32[px_out + 1] = g3;
                row_f32[px_out + 2] = b3;
            }
        });

    // PASS 2: Presence Engine (Texture, Clarity, Dehaze)
    presence::apply_presence(
        &mut f32_buf,
        w,
        h,
        recipe.clarity,
        recipe.texture,
        recipe.dehaze,
    );

    // PASS 3: Detail & Optics Engine (Sharpness, Luma Denoise, Chroma Denoise)
    detail::apply_detail(
        &mut f32_buf,
        w,
        h,
        recipe.sharpness,
        recipe.denoise_lum,
        recipe.denoise_col,
    );

    // PASS 4: Lens Distortion Correction (if enabled)
    if recipe.lens_distortion.abs() >= 0.1 {
        f32_buf = detail::apply_lens_distortion(&f32_buf, w, h, 3, recipe.lens_distortion);
    }

    // PASS 5: Defringe, Vignette & Final Quantization to 8-bit viewport output
    let mut output = vec![0u8; num_pixels * ch];
    output
        .par_chunks_mut(w * ch)
        .enumerate()
        .for_each(|(y_idx, row)| {
            let in_f32 = &f32_buf[y_idx * w * 3..(y_idx + 1) * w * 3];
            let y = y_idx as u32;

            for x in 0..w {
                let px_out = x * ch;
                let px_f32 = x * 3;

                let (r4, g4, b4) = detail::apply_defringe(
                    in_f32[px_f32],
                    in_f32[px_f32 + 1],
                    in_f32[px_f32 + 2],
                    recipe.defringe,
                );

                let (r5, g5, b5) = detail::apply_vignette(
                    r4,
                    g4,
                    b4,
                    x as u32,
                    y,
                    width,
                    height,
                    recipe.vignette,
                );

                row[px_out] = (r5.clamp(0.0, 1.0) * 255.0) as u8;
                row[px_out + 1] = (g5.clamp(0.0, 1.0) * 255.0) as u8;
                row[px_out + 2] = (b5.clamp(0.0, 1.0) * 255.0) as u8;

                if ch == 4 {
                    let orig_alpha = input[y_idx * w * ch + px_out + 3];
                    row[px_out + 3] = (orig_alpha >> 8) as u8;
                }
            }
        });

    let hist = compute_histogram(&output, ch);
    (output, hist)
}

/// Processes a 16-bit linear RGB buffer and outputs a 16-bit RGB buffer (0..65535) for master export
pub fn process_buffer_16_to_16(
    input: &[u16],
    width: u32,
    height: u32,
    channels: u32,
    recipe: &Recipe,
) -> (Vec<u16>, HistogramData) {
    let ch = channels as usize;
    let w = width as usize;
    let h = height as usize;
    let num_pixels = w * h;

    let (wb_r, wb_g, wb_b) = white_balance::kelvin_to_rgb_multipliers(
        recipe.wb_temperature,
        recipe.wb_tint,
    );
    let exp_factor = 2.0f32.powf(recipe.exposure);

    // Intermediate float buffer for multi-stage 16-bit precision processing
    let mut f32_buf = vec![0.0f32; num_pixels * 3];

    // PASS 1: Point Operations (White Balance, Exposure, Tone, Color Wheels, HSL Mixer)
    f32_buf
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y_idx, row_f32)| {
            let in_row = &input[y_idx * w * ch..(y_idx + 1) * w * ch];

            for x in 0..w {
                let px_in = x * ch;
                let px_out = x * 3;

                let r_norm = (in_row[px_in] as f32 / 65535.0) * wb_r;
                let g_norm = (in_row[px_in + 1] as f32 / 65535.0) * wb_g;
                let b_norm = (in_row[px_in + 2] as f32 / 65535.0) * wb_b;

                // 1. Tone & Dynamic range
                let (r1, g1, b1) = tone::apply_tone_pixel(r_norm, g_norm, b_norm, recipe, exp_factor);

                // 2. DaVinci Resolve 3-Way Color Wheels
                let (r2, g2, b2) = color_grading::apply_color_wheels(r1, g1, b1, recipe);

                // 3. 8-Band HSL Color Mixer
                let (r3, g3, b3) = color_grading::apply_hsl_mixer(r2, g2, b2, recipe);

                row_f32[px_out] = r3;
                row_f32[px_out + 1] = g3;
                row_f32[px_out + 2] = b3;
            }
        });

    // PASS 2: Presence Engine (Texture, Clarity, Dehaze)
    presence::apply_presence(
        &mut f32_buf,
        w,
        h,
        recipe.clarity,
        recipe.texture,
        recipe.dehaze,
    );

    // PASS 3: Detail & Optics Engine (Sharpness, Luma Denoise, Chroma Denoise)
    detail::apply_detail(
        &mut f32_buf,
        w,
        h,
        recipe.sharpness,
        recipe.denoise_lum,
        recipe.denoise_col,
    );

    // PASS 4: Lens Distortion Correction (if enabled)
    if recipe.lens_distortion.abs() >= 0.1 {
        f32_buf = detail::apply_lens_distortion(&f32_buf, w, h, 3, recipe.lens_distortion);
    }

    // PASS 5: Defringe, Vignette & Final Quantization to 16-bit master output
    let mut output = vec![0u16; num_pixels * ch];
    output
        .par_chunks_mut(w * ch)
        .enumerate()
        .for_each(|(y_idx, row)| {
            let in_f32 = &f32_buf[y_idx * w * 3..(y_idx + 1) * w * 3];
            let y = y_idx as u32;

            for x in 0..w {
                let px_out = x * ch;
                let px_f32 = x * 3;

                let (r4, g4, b4) = detail::apply_defringe(
                    in_f32[px_f32],
                    in_f32[px_f32 + 1],
                    in_f32[px_f32 + 2],
                    recipe.defringe,
                );

                let (r5, g5, b5) = detail::apply_vignette(
                    r4,
                    g4,
                    b4,
                    x as u32,
                    y,
                    width,
                    height,
                    recipe.vignette,
                );

                row[px_out] = (r5.clamp(0.0, 1.0) * 65535.0) as u16;
                row[px_out + 1] = (g5.clamp(0.0, 1.0) * 65535.0) as u16;
                row[px_out + 2] = (b5.clamp(0.0, 1.0) * 65535.0) as u16;

                if ch == 4 {
                    row[px_out + 3] = input[y_idx * w * ch + px_out + 3];
                }
            }
        });

    let hist = compute_histogram_16(&output, ch);
    (output, hist)
}

/// Generates a Before / After split comparison buffer
pub fn process_split_comparison(
    input: &[u8],
    width: u32,
    height: u32,
    channels: u32,
    recipe: &Recipe,
    split_ratio: f32,
) -> (Vec<u8>, HistogramData) {
    let (processed, hist) = process_buffer(input, width, height, channels, recipe);
    let ch = channels as usize;
    let w = width as usize;
    let split_x = ((width as f32) * split_ratio.clamp(0.0, 1.0)) as usize;

    let mut output = vec![0u8; input.len()];
    output
        .par_chunks_mut(w * ch)
        .enumerate()
        .for_each(|(y, row)| {
            let orig_row = &input[y * w * ch..(y + 1) * w * ch];
            let proc_row = &processed[y * w * ch..(y + 1) * w * ch];

            for x in 0..w {
                let px = x * ch;
                if x < split_x {
                    // BEFORE
                    row[px..px + ch].copy_from_slice(&orig_row[px..px + ch]);
                } else if x == split_x || x == split_x + 1 {
                    // Split dividing white line
                    row[px] = 255;
                    row[px + 1] = 255;
                    row[px + 2] = 255;
                    if ch == 4 {
                        row[px + 3] = 255;
                    }
                } else {
                    // AFTER
                    row[px..px + ch].copy_from_slice(&proc_row[px..px + ch]);
                }
            }
        });

    (output, hist)
}

/// Generates a Before / After split comparison buffer from a 16-bit source to an 8-bit viewport buffer
pub fn process_split_comparison_16_to_8(
    input: &[u16],
    width: u32,
    height: u32,
    channels: u32,
    recipe: &Recipe,
    split_ratio: f32,
) -> (Vec<u8>, HistogramData) {
    let (processed, hist) = process_buffer_16_to_8(input, width, height, channels, recipe);
    let ch = channels as usize;
    let w = width as usize;
    let split_x = ((width as f32) * split_ratio.clamp(0.0, 1.0)) as usize;

    let mut output = vec![0u8; (width as usize) * (height as usize) * ch];
    output
        .par_chunks_mut(w * ch)
        .enumerate()
        .for_each(|(y, row)| {
            let orig_row = &input[y * w * ch..(y + 1) * w * ch];
            let proc_row = &processed[y * w * ch..(y + 1) * w * ch];

            for x in 0..w {
                let px = x * ch;
                if x < split_x {
                    // BEFORE (Original unprocessed RAW converted to 8-bit)
                    row[px] = (orig_row[px] >> 8) as u8;
                    row[px + 1] = (orig_row[px + 1] >> 8) as u8;
                    row[px + 2] = (orig_row[px + 2] >> 8) as u8;
                    if ch == 4 {
                        row[px + 3] = (orig_row[px + 3] >> 8) as u8;
                    }
                } else if x == split_x || x == split_x + 1 {
                    // White split separator line
                    row[px] = 255;
                    row[px + 1] = 255;
                    row[px + 2] = 255;
                    if ch == 4 {
                        row[px + 3] = 255;
                    }
                } else {
                    // AFTER (Fine-tuned Recipe)
                    row[px..px + ch].copy_from_slice(&proc_row[px..px + ch]);
                }
            }
        });

    (output, hist)
}
