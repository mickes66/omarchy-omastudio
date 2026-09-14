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

/// Applies 8-channel HSL color mixer adjustments
#[inline(always)]
pub fn apply_hsl_mixer(r: f32, g: f32, b: f32, recipe: &Recipe) -> (f32, f32, f32) {
    let (h, mut s, mut l) = rgb_to_hsl(r, g, b);

    if s < 0.05 {
        // Skip nearly achromatic pixels
        return (r, g, b);
    }

    let mut delta_h = 0.0;
    let mut delta_s = 0.0;
    let mut delta_l = 0.0;

    for (i, &center) in BAND_CENTERS.iter().enumerate() {
        
        let mut diff = (h - center).abs();
        if diff > 180.0 {
            diff = 360.0 - diff;
        }

        // Soft bell weighting (width ~30 deg)
        let weight = (1.0 - diff / 35.0).max(0.0);
        if weight > 0.0 {
            let smooth_w = weight * weight * (3.0 - 2.0 * weight);
            delta_h += (recipe.hsl_hue[i] / 100.0) * 25.0 * smooth_w; // Up to +-25 deg hue shift
            delta_s += (recipe.hsl_sat[i] / 100.0) * smooth_w;
            delta_l += (recipe.hsl_lum[i] / 100.0) * 0.25 * smooth_w;
        }
    }

    let mut new_h = (h + delta_h) % 360.0;
    if new_h < 0.0 {
        new_h += 360.0;
    }

    s = (s * (1.0 + delta_s)).clamp(0.0, 1.0);
    l = (l + delta_l).clamp(0.0, 1.0);

    hsl_to_rgb(new_h, s, l)
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
    let lift_t = (1.0 - y * 2.0).clamp(0.0, 1.0);
    let w_lift = lift_t * lift_t;

    let gain_t = ((y - 0.5) * 2.0).clamp(0.0, 1.0);
    let w_gain = gain_t * gain_t;

    let w_gamma = (1.0 - w_lift - w_gain).max(0.0);

    let dr = w_lift * (recipe.lift[0] * 0.4 + recipe.lift_luma * 0.4)
           + w_gamma * (recipe.gamma[0] * 0.4 + recipe.gamma_luma * 0.4)
           + w_gain * (recipe.gain[0] * 0.4 + recipe.gain_luma * 0.4)
           + (recipe.offset[0] * 0.3 + recipe.offset_luma * 0.3);

    let dg = w_lift * (recipe.lift[1] * 0.4 + recipe.lift_luma * 0.4)
           + w_gamma * (recipe.gamma[1] * 0.4 + recipe.gamma_luma * 0.4)
           + w_gain * (recipe.gain[1] * 0.4 + recipe.gain_luma * 0.4)
           + (recipe.offset[1] * 0.3 + recipe.offset_luma * 0.3);

    let db = w_lift * (recipe.lift[2] * 0.4 + recipe.lift_luma * 0.4)
           + w_gamma * (recipe.gamma[2] * 0.4 + recipe.gamma_luma * 0.4)
           + w_gain * (recipe.gain[2] * 0.4 + recipe.gain_luma * 0.4)
           + (recipe.offset[2] * 0.3 + recipe.offset_luma * 0.3);

    r = (r + dr).clamp(0.0, 2.0);
    g = (g + dg).clamp(0.0, 2.0);
    b = (b + db).clamp(0.0, 2.0);

    (r, g, b)
}
