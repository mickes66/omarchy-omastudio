#![allow(clippy::excessive_precision)]
/// Calculates RGB multipliers for a given color temperature in Kelvin (2000K - 12000K)
/// and Tint (-100 to +100)
pub fn kelvin_to_rgb_multipliers(kelvin: f32, tint: f32) -> (f32, f32, f32) {
    let temp = (kelvin / 100.0).clamp(20.0, 120.0);

    // Tanner-Helland Planckian approximation
    let red = if temp <= 66.0 {
        255.0
    } else {
        let r = temp - 60.0;
        329.698727446 * r.powf(-0.1332047592)
    };

    let green = if temp <= 66.0 {
        let g = temp;
        99.4708025861 * g.ln() - 161.1195681661
    } else {
        let g = temp - 60.0;
        288.1221695283 * g.powf(-0.0755148492)
    };

    let blue = if temp >= 66.0 {
        255.0
    } else if temp <= 19.0 {
        0.0
    } else {
        let b = temp - 10.0;
        138.5177312231 * b.ln() - 305.0447927307
    };

    // Reference neutral D65 (6500K)
    let ref_r = 255.0;
    let ref_g = 255.0;
    let ref_b = 255.0;

    let mut mul_r = (ref_r / red.max(1.0)).clamp(0.4, 2.8);
    let mut mul_g = (ref_g / green.max(1.0)).clamp(0.4, 2.5);
    let mut mul_b = (ref_b / blue.max(1.0)).clamp(0.4, 3.2);

    // Apply Tint: positive = magenta (boost R & B, reduce G), negative = green
    let tint_factor = tint / 100.0;
    mul_g *= 1.0 - (tint_factor * 0.25);
    mul_r *= 1.0 + (tint_factor * 0.12);
    mul_b *= 1.0 + (tint_factor * 0.12);

    (mul_r, mul_g, mul_b)
}
