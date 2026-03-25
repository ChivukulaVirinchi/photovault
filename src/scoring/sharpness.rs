//! Sharpness scoring using Laplacian variance
//!
//! The Laplacian operator highlights edges. A sharp image has
//! strong edges, resulting in high variance of the Laplacian.

use image::{DynamicImage, GrayImage};

/// Sharpness scoring using Laplacian variance
pub struct SharpnessScorer;

impl SharpnessScorer {
    /// Calculate sharpness score for an image
    ///
    /// Returns a value between 0 and 1, where higher is sharper.
    pub fn score(image: &DynamicImage) -> f32 {
        let gray = image.to_luma8();
        let variance = Self::laplacian_variance(&gray);

        // Normalize to 0-1 range
        // Empirically, variance > 500 is very sharp, < 100 is blurry
        Self::normalize(variance, 50.0, 500.0)
    }

    /// Calculate Laplacian variance of a grayscale image
    fn laplacian_variance(image: &GrayImage) -> f64 {
        let (width, height) = image.dimensions();

        if width < 3 || height < 3 {
            return 0.0;
        }

        // Laplacian kernel: [0, 1, 0]
        //                   [1,-4, 1]
        //                   [0, 1, 0]
        let mut sum = 0.0f64;
        let mut sum_sq = 0.0f64;
        let mut count = 0u64;

        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let center = image.get_pixel(x, y).0[0] as f64;
                let top = image.get_pixel(x, y - 1).0[0] as f64;
                let bottom = image.get_pixel(x, y + 1).0[0] as f64;
                let left = image.get_pixel(x - 1, y).0[0] as f64;
                let right = image.get_pixel(x + 1, y).0[0] as f64;

                let laplacian = top + bottom + left + right - 4.0 * center;

                sum += laplacian;
                sum_sq += laplacian * laplacian;
                count += 1;
            }
        }

        if count == 0 {
            return 0.0;
        }

        let mean = sum / count as f64;
        let variance = (sum_sq / count as f64) - (mean * mean);

        variance.max(0.0)
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
    fn test_uniform_image_is_not_sharp() {
        // Uniform gray image should have low sharpness
        let img: GrayImage = ImageBuffer::from_fn(100, 100, |_, _| Luma([128u8]));
        let variance = SharpnessScorer::laplacian_variance(&img);

        assert!(
            variance < 1.0,
            "Uniform image should have near-zero variance"
        );
    }

    #[test]
    fn test_edge_image_is_sharp() {
        // Image with sharp vertical edge should have high sharpness
        let img: GrayImage =
            ImageBuffer::from_fn(
                100,
                100,
                |x, _| {
                    if x < 50 {
                        Luma([0u8])
                    } else {
                        Luma([255u8])
                    }
                },
            );
        let variance = SharpnessScorer::laplacian_variance(&img);

        assert!(variance > 100.0, "Edge image should have high variance");
    }
}
