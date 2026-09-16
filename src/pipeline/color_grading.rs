use crate::recipe::Recipe;

const BAND_CENTERS: [f32; 8] = [
    0.0,   // Red
    30.0,  // Orange
    60.0,  // Yellow
    120.0, // Green
    180.0, // Aqua
    240.0, // Blue
    280.0, // Purple
    320.0, // Magenta
];

#[inline(always)]
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < 1e-5 {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };

    let mut h = if (max - r).abs() < 1e-5 {
        (g - b) / d + (if g < b { 6.0 } else { 0.0 })
    } else if (max - g).abs() < 1e-5 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h *= 60.0;

    (h, s, l)
}

#[inline(always)]
fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

#[inline(always)]
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-5 {
        return (l, l, l);
    }

    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hk = h / 360.0;

    let r = hue_to_rgb(p, q, hk + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, hk);
    let b = hue_to_rgb(p, q, hk - 1.0 / 3.0);

    (r, g, b)
}

/// Applies 8-channel HSL color mixer adjustments with Perceived Luminance Anchoring.
///
/// Ensures modifying Hue or Saturation in any color band (e.g. skies, foliage, skin)
/// preserves original Rec. 709 perceived luminance, matching Capture One & Lightroom standards.
#[inline(always)]
pub fn apply_hsl_mixer(r: f32, g: f32, b: f32, recipe: &Recipe) -> (f32, f32, f32) {
    let mut has_adj = false;
    for i in 0..8 {
        if recipe.hsl_hue[i] != 0.0 || recipe.hsl_sat[i] != 0.0 || recipe.hsl_lum[i] != 0.0 {
            has_adj = true;
            break;
        }
    }
    if !has_adj {
        return (r, g, b);
    }

    let (h, mut s, mut l) = rgb_to_hsl(r, g, b);

    if s < 0.04 {
        // Skip nearly achromatic pixels
        return (r, g, b);
    }

    let orig_luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

    let mut delta_h = 0.0;
    let mut delta_s = 0.0;
    let mut delta_l = 0.0;

    for (i, &center) in BAND_CENTERS.iter().enumerate() {
        let mut diff = (h - center).abs();
        if diff > 180.0 {
            diff = 360.0 - diff;
        }

        // Soft cubic bell weighting (bandwidth ~ 35 deg)
        let weight = (1.0 - diff / 35.0).max(0.0);
        if weight > 0.0 {
            let smooth_w = weight * weight * (3.0 - 2.0 * weight);
            delta_h += (recipe.hsl_hue[i] / 100.0) * 30.0 * smooth_w; // Up to +-30 deg hue shift
            delta_s += (recipe.hsl_sat[i] / 100.0) * smooth_w;
            delta_l += (recipe.hsl_lum[i] / 100.0) * 0.30 * smooth_w;
        }
    }

    let mut new_h = (h + delta_h) % 360.0;
    if new_h < 0.0 {
        new_h += 360.0;
    }

    s = (s * (1.0 + delta_s)).clamp(0.0, 1.0);
    l = (l + delta_l * 0.5).clamp(0.0, 1.0);

    let (mut nr, mut ng, mut nb) = hsl_to_rgb(new_h, s, l);

    // Perceptual Luminance Anchor:
    // Guarantees hue and saturation shifts preserve photometric perceived luminance
    let target_luma = (orig_luma + delta_l).clamp(0.0, 1.5);
    let new_luma = 0.2126 * nr + 0.7152 * ng + 0.0722 * nb;
    if new_luma > 1e-5 {
        let l_ratio = target_luma / new_luma;
        // Softly preserve chrominance vector
        let cr = nr - new_luma;
        let cg = ng - new_luma;
        let cb = nb - new_luma;
        nr = target_luma + cr * l_ratio.powf(0.35).min(1.25);
        ng = target_luma + cg * l_ratio.powf(0.35).min(1.25);
        nb = target_luma + cb * l_ratio.powf(0.35).min(1.25);
    }

    (nr, ng, nb)
}

/// Applies DaVinci Resolve style 3-Way Color Wheels (Lift, Gamma, Gain, Offset)
#[inline(always)]
pub fn apply_color_wheels(mut r: f32, mut g: f32, mut b: f32, recipe: &Recipe) -> (f32, f32, f32) {
    let has_lift = recipe.lift[0] != 0.0 || recipe.lift[1] != 0.0 || recipe.lift[2] != 0.0 || recipe.lift_luma != 0.0;
    let has_gamma = recipe.gamma[0] != 0.0 || recipe.gamma[1] != 0.0 || recipe.gamma[2] != 0.0 || recipe.gamma_luma != 0.0;
    let has_gain = recipe.gain[0] != 0.0 || recipe.gain[1] != 0.0 || recipe.gain[2] != 0.0 || recipe.gain_luma != 0.0;
    let has_offset = recipe.offset[0] != 0.0 || recipe.offset[1] != 0.0 || recipe.offset[2] != 0.0 || recipe.offset_luma != 0.0;

    if !has_lift && !has_gamma && !has_gain && !has_offset {
        return (r, g, b);
    }

    let y = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);

    // DaVinci Resolve Lift: Quadratic falloff towards highlights (1 - y)^2
    let w_lift = (1.0 - y).max(0.0) * (1.0 - y).max(0.0);

    // DaVinci Resolve Gain: Linear-quadratic ramp from shadows to highlights y^2
    let w_gain = y * y;

    // DaVinci Resolve Gamma: Bell shape peaking at midtones (1 - w_lift - w_gain)
    let w_gamma = (1.0 - w_lift - w_gain).max(0.0);

    let dr = w_lift * (recipe.lift[0] * 0.35 + recipe.lift_luma * 0.35)
           + w_gamma * (recipe.gamma[0] * 0.35 + recipe.gamma_luma * 0.35)
           + w_gain * (recipe.gain[0] * 0.35 + recipe.gain_luma * 0.35)
           + (recipe.offset[0] * 0.25 + recipe.offset_luma * 0.25);

    let dg = w_lift * (recipe.lift[1] * 0.35 + recipe.lift_luma * 0.35)
           + w_gamma * (recipe.gamma[1] * 0.35 + recipe.gamma_luma * 0.35)
           + w_gain * (recipe.gain[1] * 0.35 + recipe.gain_luma * 0.35)
           + (recipe.offset[1] * 0.25 + recipe.offset_luma * 0.25);

    let db = w_lift * (recipe.lift[2] * 0.35 + recipe.lift_luma * 0.35)
           + w_gamma * (recipe.gamma[2] * 0.35 + recipe.gamma_luma * 0.35)
           + w_gain * (recipe.gain[2] * 0.35 + recipe.gain_luma * 0.35)
           + (recipe.offset[2] * 0.25 + recipe.offset_luma * 0.25);

    r = (r + dr).max(0.0);
    g = (g + dg).max(0.0);
    b = (b + db).max(0.0);

    (r, g, b)
}
