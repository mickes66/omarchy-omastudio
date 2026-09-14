#![allow(clippy::too_many_arguments)]

/// Applies radial vignetting effect
#[inline(always)]
pub fn apply_vignette(
    r: f32,
    g: f32,
    b: f32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    vignette_amount: f32,
) -> (f32, f32, f32) {
    if vignette_amount.abs() < 0.01 {
        return (r, g, b);
    }

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let max_dist = (cx * cx + cy * cy).sqrt();

    let dx = x as f32 - cx;
    let dy = y as f32 - cy;
    let dist = (dx * dx + dy * dy).sqrt() / max_dist;

    // Smooth cosine falloff from 0.4 radius to outer edge
    let falloff = ((dist - 0.4) / 0.6).clamp(0.0, 1.0);
    let smooth = falloff * falloff * (3.0 - 2.0 * falloff);

    let factor = 1.0 + (vignette_amount / 100.0) * smooth;
    let clamped_factor = factor.max(0.0);

    (r * clamped_factor, g * clamped_factor, b * clamped_factor)
}

/// Reduces chromatic aberration fringing (purple and green edges)
#[inline(always)]
pub fn apply_defringe(r: f32, g: f32, b: f32, amount: f32) -> (f32, f32, f32) {
    if amount <= 0.01 {
        return (r, g, b);
    }
    let factor = (amount / 100.0).clamp(0.0, 1.0);
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

    // Detect purple fringing: R and B much higher than G
    let purple = (r.min(b) - g).max(0.0);
    // Detect green fringing: G much higher than (R + B) / 2
    let green = (g - (r + b) * 0.5).max(0.0);

    let fringe = (purple + green).clamp(0.0, 1.0) * factor * 0.6;
    if fringe > 0.001 {
        let nr = r * (1.0 - fringe) + luma * fringe;
        let ng = g * (1.0 - fringe) + luma * fringe;
        let nb = b * (1.0 - fringe) + luma * fringe;
        return (nr, ng, nb);
    }
    (r, g, b)
}
