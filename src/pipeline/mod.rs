pub mod color_grading;
pub mod detail;
pub mod histogram;
pub mod tone;
pub mod white_balance;

use crate::recipe::Recipe;
use histogram::{compute_histogram, HistogramData};
use rayon::prelude::*;

/// Processes a linear RGB buffer according to the given Recipe using parallel multi-threading
pub fn process_buffer(
    input: &[u8],
    width: u32,
    height: u32,
    channels: u32,
    recipe: &Recipe,
) -> (Vec<u8>, HistogramData) {
    let mut output = vec![0u8; input.len()];
    let ch = channels as usize;

    let (wb_r, wb_g, wb_b) = white_balance::kelvin_to_rgb_multipliers(
        recipe.wb_temperature,
        recipe.wb_tint,
    );
    let exp_factor = 2.0f32.powf(recipe.exposure);

    // Process rows in parallel with Rayon
    let row_stride = (width as usize) * ch;
    output
        .par_chunks_mut(row_stride)
        .enumerate()
        .for_each(|(y_idx, row)| {
            let in_row = &input[y_idx * row_stride..(y_idx + 1) * row_stride];
            let y = y_idx as u32;

            for x in 0..width {
                let px = (x as usize) * ch;

                let r_norm = (in_row[px] as f32 / 255.0) * wb_r;
                let g_norm = (in_row[px + 1] as f32 / 255.0) * wb_g;
                let b_norm = (in_row[px + 2] as f32 / 255.0) * wb_b;

                // 1. Tone & Dynamic range
                let (r1, g1, b1) = tone::apply_tone_pixel(r_norm, g_norm, b_norm, recipe, exp_factor);

                // 2. DaVinci Resolve 3-Way Color Wheels (Lift, Gamma, Gain, Offset)
                let (r2, g2, b2) = color_grading::apply_color_wheels(r1, g1, b1, recipe);

                // 3. 8-Band HSL Color Mixer
                let (r3, g3, b3) = color_grading::apply_hsl_mixer(r2, g2, b2, recipe);

                // 4. Defringe (Chromatic Aberration reduction)
                let (r4, g4, b4) = detail::apply_defringe(r3, g3, b3, recipe.defringe);

                // 5. Vignette
                let (r5, g5, b5) = detail::apply_vignette(r4, g4, b4, x, y, width, height, recipe.vignette);

                row[px] = (r5.clamp(0.0, 1.0) * 255.0) as u8;
                row[px + 1] = (g5.clamp(0.0, 1.0) * 255.0) as u8;
                row[px + 2] = (b5.clamp(0.0, 1.0) * 255.0) as u8;

                if ch == 4 {
                    row[px + 3] = in_row[px + 3];
                }
            }
        });

    let hist = compute_histogram(&output, ch);
    (output, hist)
}

/// Generates a Before / After split comparison buffer
pub fn process_split_comparison(
    input: &[u8],
    width: u32,
    height: u32,
    channels: u32,
    recipe: &Recipe,
    split_ratio: f32, // 0.0 to 1.0 (e.g. 0.5 for half split)
) -> (Vec<u8>, HistogramData) {
    let mut output = vec![0u8; input.len()];
    let ch = channels as usize;
    let split_x = ((width as f32) * split_ratio.clamp(0.0, 1.0)) as u32;

    let (wb_r, wb_g, wb_b) = white_balance::kelvin_to_rgb_multipliers(
        recipe.wb_temperature,
        recipe.wb_tint,
    );
    let exp_factor = 2.0f32.powf(recipe.exposure);

    let row_stride = (width as usize) * ch;
    output
        .par_chunks_mut(row_stride)
        .enumerate()
        .for_each(|(y_idx, row)| {
            let in_row = &input[y_idx * row_stride..(y_idx + 1) * row_stride];
            let y = y_idx as u32;

            for x in 0..width {
                let px = (x as usize) * ch;

                if x < split_x {
                    // BEFORE (Original unprocessed RAW)
                    row[px] = in_row[px];
                    row[px + 1] = in_row[px + 1];
                    row[px + 2] = in_row[px + 2];
                } else if x == split_x || x == split_x + 1 {
                    // White split separator line
                    row[px] = 255;
                    row[px + 1] = 255;
                    row[px + 2] = 255;
                } else {
                    // AFTER (Fine-tuned Recipe)
                    let r_norm = (in_row[px] as f32 / 255.0) * wb_r;
                    let g_norm = (in_row[px + 1] as f32 / 255.0) * wb_g;
                    let b_norm = (in_row[px + 2] as f32 / 255.0) * wb_b;

                    let (r1, g1, b1) = tone::apply_tone_pixel(r_norm, g_norm, b_norm, recipe, exp_factor);
                    let (r2, g2, b2) = color_grading::apply_color_wheels(r1, g1, b1, recipe);
                    let (r3, g3, b3) = color_grading::apply_hsl_mixer(r2, g2, b2, recipe);
                    let (r4, g4, b4) = detail::apply_defringe(r3, g3, b3, recipe.defringe);
                    let (r5, g5, b5) = detail::apply_vignette(r4, g4, b4, x, y, width, height, recipe.vignette);

                    row[px] = (r5.clamp(0.0, 1.0) * 255.0) as u8;
                    row[px + 1] = (g5.clamp(0.0, 1.0) * 255.0) as u8;
                    row[px + 2] = (b5.clamp(0.0, 1.0) * 255.0) as u8;
                }

                if ch == 4 {
                    row[px + 3] = in_row[px + 3];
                }
            }
        });

    let hist = compute_histogram(&output, ch);
    (output, hist)
}
