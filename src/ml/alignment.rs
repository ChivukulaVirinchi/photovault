//! Face alignment via 5-point similarity transform.
//!
//! Maps detected 5-point landmarks (left eye, right eye, nose, left mouth,
//! right mouth) onto the canonical InsightFace 112x112 template via a least-squares
//! similarity transform (uniform scale + rotation + translation), then bilinearly
//! samples the source image to produce a 112x112 RGB crop ready for the embedder.
//!
//! Replaces the naive eye-center-crop approach which did not correct for head
//! tilt and produced different embeddings for the same face at small rotations.

use image::{DynamicImage, Rgb, RgbImage};

/// InsightFace canonical 5-point template (ArcFace / GLinTR) at 112x112.
/// Order: left eye, right eye, nose, left mouth, right mouth.
pub const CANONICAL_TEMPLATE_112: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];

/// 2x3 affine transform: q = M * [p_x, p_y, 1]^T.
///
/// Represents a similarity transform (uniform scale + rotation + translation).
#[derive(Debug, Clone, Copy)]
pub struct SimilarityTransform {
    /// a = s*cos(theta)
    pub a: f32,
    /// b = s*sin(theta)
    pub b: f32,
    pub tx: f32,
    pub ty: f32,
}

impl SimilarityTransform {
    /// Apply inverse: target point -> source point.
    #[inline]
    pub fn apply_inverse(&self, q: (f32, f32)) -> Option<(f32, f32)> {
        let det = self.a * self.a + self.b * self.b;
        if det <= f32::EPSILON {
            return None;
        }
        let qx = q.0 - self.tx;
        let qy = q.1 - self.ty;
        Some((
            (self.a * qx + self.b * qy) / det,
            (-self.b * qx + self.a * qy) / det,
        ))
    }

    #[cfg(test)]
    #[inline]
    fn apply(&self, p: (f32, f32)) -> (f32, f32) {
        (
            self.a * p.0 - self.b * p.1 + self.tx,
            self.b * p.0 + self.a * p.1 + self.ty,
        )
    }
}

/// Least-squares similarity transform from source -> target (both 5 points).
///
/// Solves for (a, b, tx, ty) such that:
///   target_x = a*src_x - b*src_y + tx
///   target_y = b*src_x + a*src_y + ty
///
/// Closed-form via centering and normal equations. Returns None if the source
/// points are degenerate (zero spread).
pub fn estimate_similarity(
    src: &[(f32, f32); 5],
    dst: &[(f32, f32); 5],
) -> Option<SimilarityTransform> {
    let n = src.len() as f32;

    let (mut mpx, mut mpy, mut mqx, mut mqy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for i in 0..src.len() {
        mpx += src[i].0;
        mpy += src[i].1;
        mqx += dst[i].0;
        mqy += dst[i].1;
    }
    mpx /= n;
    mpy /= n;
    mqx /= n;
    mqy /= n;

    let mut num_a = 0.0f32;
    let mut num_b = 0.0f32;
    let mut den = 0.0f32;
    for i in 0..src.len() {
        let px = src[i].0 - mpx;
        let py = src[i].1 - mpy;
        let qx = dst[i].0 - mqx;
        let qy = dst[i].1 - mqy;

        num_a += px * qx + py * qy;
        num_b += px * qy - py * qx;
        den += px * px + py * py;
    }

    if den <= f32::EPSILON {
        return None;
    }

    let a = num_a / den;
    let b = num_b / den;
    let tx = mqx - a * mpx + b * mpy;
    let ty = mqy - b * mpx - a * mpy;

    Some(SimilarityTransform { a, b, tx, ty })
}

/// Warp a source image onto the 112x112 canonical face template using the
/// given 5-point landmarks. Bilinear sampling, black fill for out-of-bounds.
///
/// Returns None only if the landmarks are degenerate (e.g. all collinear or
/// coincident), which signals a bad detection that should be rejected.
pub fn align_face_112(image: &DynamicImage, landmarks: &[(f32, f32); 5]) -> Option<RgbImage> {
    let transform = estimate_similarity(landmarks, &CANONICAL_TEMPLATE_112)?;
    let rgb = image.to_rgb8();
    let (w, h) = (rgb.width() as i32, rgb.height() as i32);

    let mut out = RgbImage::new(112, 112);

    for v in 0..112u32 {
        for u in 0..112u32 {
            let (sx, sy) = match transform.apply_inverse((u as f32, v as f32)) {
                Some(p) => p,
                None => continue,
            };

            let pixel = bilinear_sample(&rgb, sx, sy, w, h);
            out.put_pixel(u, v, pixel);
        }
    }

    Some(out)
}

#[inline]
fn bilinear_sample(img: &RgbImage, x: f32, y: f32, w: i32, h: i32) -> Rgb<u8> {
    if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
        return Rgb([0, 0, 0]);
    }

    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);

    let dx = x - x0 as f32;
    let dy = y - y0 as f32;

    let p00 = img.get_pixel(x0 as u32, y0 as u32).0;
    let p10 = img.get_pixel(x1 as u32, y0 as u32).0;
    let p01 = img.get_pixel(x0 as u32, y1 as u32).0;
    let p11 = img.get_pixel(x1 as u32, y1 as u32).0;

    let w00 = (1.0 - dx) * (1.0 - dy);
    let w10 = dx * (1.0 - dy);
    let w01 = (1.0 - dx) * dy;
    let w11 = dx * dy;

    let blend = |i: usize| -> u8 {
        let v =
            p00[i] as f32 * w00 + p10[i] as f32 * w10 + p01[i] as f32 * w01 + p11[i] as f32 * w11;
        v.round().clamp(0.0, 255.0) as u8
    };

    Rgb([blend(0), blend(1), blend(2)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_transform_on_canonical() {
        let t = estimate_similarity(&CANONICAL_TEMPLATE_112, &CANONICAL_TEMPLATE_112).unwrap();
        assert!((t.a - 1.0).abs() < 1e-4);
        assert!(t.b.abs() < 1e-4);
        assert!(t.tx.abs() < 1e-3);
        assert!(t.ty.abs() < 1e-3);
    }

    #[test]
    fn scaled_rotated_translated_recovers_parameters() {
        let theta = 0.3f32;
        let s = 1.7f32;
        let (tx, ty) = (15.0f32, -8.0f32);
        let (c, sn) = (theta.cos(), theta.sin());

        let dst: [(f32, f32); 5] = core::array::from_fn(|i| {
            let (px, py) = CANONICAL_TEMPLATE_112[i];
            (s * (c * px - sn * py) + tx, s * (sn * px + c * py) + ty)
        });

        let t = estimate_similarity(&CANONICAL_TEMPLATE_112, &dst).unwrap();
        assert!((t.a - s * c).abs() < 1e-3);
        assert!((t.b - s * sn).abs() < 1e-3);
        assert!((t.tx - tx).abs() < 1e-2);
        assert!((t.ty - ty).abs() < 1e-2);
    }

    #[test]
    fn inverse_roundtrip() {
        let t = SimilarityTransform {
            a: 0.8,
            b: 0.4,
            tx: 10.0,
            ty: -5.0,
        };
        let p = (42.0f32, 17.0f32);
        let q = t.apply(p);
        let p_back = t.apply_inverse(q).unwrap();
        assert!((p_back.0 - p.0).abs() < 1e-3);
        assert!((p_back.1 - p.1).abs() < 1e-3);
    }
}
