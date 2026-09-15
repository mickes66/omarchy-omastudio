use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramData {
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    pub luma: Vec<u32>,
    pub max_count: u32,
    pub shadow_clipping_percent: f32,
    pub highlight_clipping_percent: f32,
}

impl Default for HistogramData {
    fn default() -> Self {
        Self {
            red: vec![0; 256],
            green: vec![0; 256],
            blue: vec![0; 256],
            luma: vec![0; 256],
            max_count: 1,
            shadow_clipping_percent: 0.0,
            highlight_clipping_percent: 0.0,
        }
    }
}

pub fn compute_histogram(rgb_buffer: &[u8], channels: usize) -> HistogramData {
    let mut red = vec![0u32; 256];
    let mut green = vec![0u32; 256];
    let mut blue = vec![0u32; 256];
    let mut luma = vec![0u32; 256];

    let mut shadow_clips = 0u32;
    let mut highlight_clips = 0u32;
    let total_pixels = rgb_buffer.len() / channels;

    for chunk in rgb_buffer.chunks_exact(channels) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let y = ((0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) as u32).min(255) as u8;

        red[r as usize] += 1;
        green[g as usize] += 1;
        blue[b as usize] += 1;
        luma[y as usize] += 1;

        if y <= 1 {
            shadow_clips += 1;
        } else if y >= 254 {
            highlight_clips += 1;
        }
    }

    // Determine max peak excluding absolute 0 and 255 spikes for better visual scaling
    let mut max_count = 1;
    for i in 1..255 {
        max_count = max_count
            .max(red[i])
            .max(green[i])
            .max(blue[i])
            .max(luma[i]);
    }

    let shadow_percent = if total_pixels > 0 {
        (shadow_clips as f32 / total_pixels as f32) * 100.0
    } else {
        0.0
    };

    let highlight_percent = if total_pixels > 0 {
        (highlight_clips as f32 / total_pixels as f32) * 100.0
    } else {
        0.0
    };

    HistogramData {
        red,
        green,
        blue,
        luma,
        max_count,
        shadow_clipping_percent: shadow_percent,
        highlight_clipping_percent: highlight_percent,
    }
}

pub fn compute_histogram_16(rgb_buffer: &[u16], channels: usize) -> HistogramData {
    let mut red = vec![0u32; 256];
    let mut green = vec![0u32; 256];
    let mut blue = vec![0u32; 256];
    let mut luma = vec![0u32; 256];

    let mut shadow_clips = 0u32;
    let mut highlight_clips = 0u32;
    let total_pixels = rgb_buffer.len() / channels;

    for chunk in rgb_buffer.chunks_exact(channels) {
        let r = (chunk[0] >> 8) as u8;
        let g = (chunk[1] >> 8) as u8;
        let b = (chunk[2] >> 8) as u8;
        let y = ((0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) as u32).min(255) as u8;

        red[r as usize] += 1;
        green[g as usize] += 1;
        blue[b as usize] += 1;
        luma[y as usize] += 1;

        if y <= 1 {
            shadow_clips += 1;
        } else if y >= 254 {
            highlight_clips += 1;
        }
    }

    let mut max_count = 1;
    for i in 1..255 {
        max_count = max_count
            .max(red[i])
            .max(green[i])
            .max(blue[i])
            .max(luma[i]);
    }

    let shadow_percent = if total_pixels > 0 {
        (shadow_clips as f32 / total_pixels as f32) * 100.0
    } else {
        0.0
    };

    let highlight_percent = if total_pixels > 0 {
        (highlight_clips as f32 / total_pixels as f32) * 100.0
    } else {
        0.0
    };

    HistogramData {
        red,
        green,
        blue,
        luma,
        max_count,
        shadow_clipping_percent: shadow_percent,
        highlight_clipping_percent: highlight_percent,
    }
}
