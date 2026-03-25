//! Blur detection using frequency domain analysis
//!
//! Blurry images have less high-frequency content.
//! We use a simplified approach based on edge density.

use image::{DynamicImage, GrayImage};

/// Blur detection scorer
pub struct BlurDetector;

impl BlurDetector {
    /// Calculate blur score for an image
    ///
    /// Returns a value between 0 and 1, where higher means LESS blur (sharper).
    pub fn score(image: &DynamicImage) -> f32 {
        let gray = image.to_luma8();
        let edge_density = Self::calculate_edge_density(&gray);

        // Normalize to 0-1 range
        Self::normalize(edge_density, 0.01, 0.15)
    }

    /// Calculate edge density using Sobel operator
    fn calculate_edge_density(image: &GrayImage) -> f64 {
        let (width, height) = image.dimensions();

        if width < 3 || height < 3 {
            return 0.0;
        }

        let mut edge_count = 0u64;
        let mut total_pixels = 0u64;
        let edge_threshold = 30.0; // Threshold for considering a pixel as edge

        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                // Sobel X kernel
                let gx = Self::pixel(image, x + 1, y - 1) as f64
                    + 2.0 * Self::pixel(image, x + 1, y) as f64
                    + Self::pixel(image, x + 1, y + 1) as f64
                    - Self::pixel(image, x - 1, y - 1) as f64
                    - 2.0 * Self::pixel(image, x - 1, y) as f64
                    - Self::pixel(image, x - 1, y + 1) as f64;

                // Sobel Y kernel
                let gy = Self::pixel(image, x - 1, y + 1) as f64
                    + 2.0 * Self::pixel(image, x, y + 1) as f64
                    + Self::pixel(image, x + 1, y + 1) as f64
                    - Self::pixel(image, x - 1, y - 1) as f64
                    - 2.0 * Self::pixel(image, x, y - 1) as f64
                    - Self::pixel(image, x + 1, y - 1) as f64;

                let magnitude = (gx * gx + gy * gy).sqrt();

                if magnitude > edge_threshold {
                    edge_count += 1;
                }
                total_pixels += 1;
            }
        }

        if total_pixels == 0 {
            return 0.0;
        }

        edge_count as f64 / total_pixels as f64
    }

    /// Get pixel value as u8
    fn pixel(image: &GrayImage, x: u32, y: u32) -> u8 {
        image.get_pixel(x, y).0[0]
    }

    /// Normalize a value to 0-1 range
    fn normalize(value: f64, min: f64, max: f64) -> f32 {
        let clamped = value.clamp(min, max);
        ((clamped - min) / (max - min)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    #[test]
    fn test_uniform_image_is_blurry() {
        let img: GrayImage = ImageBuffer::from_fn(100, 100, |_, _| Luma([128u8]));
        let edge_density = BlurDetector::calculate_edge_density(&img);

        assert!(
            edge_density < 0.01,
            "Uniform image should have very low edge density"
        );
    }
}
