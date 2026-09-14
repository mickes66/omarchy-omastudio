use crate::recipe::Recipe;

/// Applies photometric exposure, dynamic range tone adjustments, and presence
#[inline(always)]
pub fn apply_tone_pixel(
    mut r: f32,
    mut g: f32,
    mut b: f32,
    recipe: &Recipe,
    exp_factor: f32,
) -> (f32, f32, f32) {
    // 1. Photometric Exposure: I_out = I_in * 2^EV
    r *= exp_factor;
    g *= exp_factor;
    b *= exp_factor;

    // Calculate luminance
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if luma <= 0.0 {
        return (0.0, 0.0, 0.0);
    }

    let mut luma_adj = luma;

    // 2. Highlights recovery / compression
    if recipe.highlights != 0.0 {
        let h_weight = (luma - 0.4).max(0.0) / 0.6; // Active mostly in top 60%
        let h_adj = (recipe.highlights / 100.0) * h_weight * 0.4;
        luma_adj += h_adj;
    }

    // 3. Shadows lift / crush
    if recipe.shadows != 0.0 {
        let s_weight = (0.6 - luma).max(0.0) / 0.6; // Active mostly in bottom 60%
        let s_adj = (recipe.shadows / 100.0) * s_weight * 0.4;
        luma_adj += s_adj;
    }

    // 4. Whites & Blacks anchor points
    if recipe.whites != 0.0 {
        let w_weight = (luma - 0.6).max(0.0) / 0.4;
        luma_adj += (recipe.whites / 100.0) * w_weight * 0.25;
    }
    if recipe.blacks != 0.0 {
        let b_weight = (0.35 - luma).max(0.0) / 0.35;
        luma_adj += (recipe.blacks / 100.0) * b_weight * 0.25;
    }

    // 5. Contrast (S-curve centered around midtone 0.18)
    if recipe.contrast != 0.0 {
        let c = recipe.contrast / 100.0;
        let diff = luma_adj - 0.18;
        luma_adj = 0.18 + diff * (1.0 + c * 0.6) + (diff * diff * diff) * c * 0.4;
    }

    // Tone Curve 4-zone parametric offsets
    if recipe.curve_highlights != 0.0 || recipe.curve_lights != 0.0 || recipe.curve_darks != 0.0 || recipe.curve_shadows != 0.0 {
        let ch = (recipe.curve_highlights / 100.0) * (luma_adj - 0.75).max(0.0) / 0.25;
        let cl = (recipe.curve_lights / 100.0) * ((luma_adj - 0.5).max(0.0) * (0.75 - luma_adj).max(0.0) * 4.0);
        let cd = (recipe.curve_darks / 100.0) * ((luma_adj - 0.25).max(0.0) * (0.5 - luma_adj).max(0.0) * 4.0);
        let cs = (recipe.curve_shadows / 100.0) * (0.25 - luma_adj).max(0.0) / 0.25;
        luma_adj += (ch + cl + cd + cs) * 0.3;
    }

    luma_adj = luma_adj.clamp(0.0, 1.5);

    // Apply luma adjustment ratio to color channels
    let luma_ratio = if luma > 0.0001 { luma_adj / luma } else { 1.0 };
    r *= luma_ratio;
    g *= luma_ratio;
    b *= luma_ratio;

    // 6. Presence: Vibrance and Saturation
    let max_c = r.max(g).max(b);
    let min_c = r.min(g).min(b);
    let current_sat = if max_c > 0.0001 { (max_c - min_c) / max_c } else { 0.0 };

    let mut sat_delta = recipe.saturation / 100.0;

    // Vibrance boosts less saturated colors more, protecting skin tones
    if recipe.vibrance != 0.0 {
        let vib_factor = (1.0 - current_sat) * (recipe.vibrance / 100.0);
        sat_delta += vib_factor;
    }

    if sat_delta != 0.0 {
        let new_luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let sat_mult = (1.0 + sat_delta).max(0.0);
        r = new_luma + (r - new_luma) * sat_mult;
        g = new_luma + (g - new_luma) * sat_mult;
        b = new_luma + (b - new_luma) * sat_mult;
    }

    (r, g, b)
}
