#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use crate::image::ToRGB;
use crate::image::RGBAPLU;
use crate::image::RGBLU;
use imgref::*;
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use rayon::prelude::*;

const D65x: f32 = 0.9505;
const D65y: f32 = 1.0;
const D65z: f32 = 1.089;

pub type GBitmap = ImgVec<f32>;
pub(crate) trait ToLAB {
    fn to_lab(&self) -> (f32, f32, f32);
}

#[inline(always)]
fn fma_matrix(r: f32, rx: f32, g: f32, gx: f32, b: f32, bx: f32) -> f32 {
    b.mul_add(bx, g.mul_add(gx, r * rx))
}

const EPSILON: f32 = 216. / 24389.;
const K: f32 = 24389. / (27. * 116.); // http://www.brucelindbloom.com/LContinuity.html

impl ToLAB for RGBLU {
    fn to_lab(&self) -> (f32, f32, f32) {
        let fx = fma_matrix(self.r, 0.4124 / D65x, self.g, 0.3576 / D65x, self.b, 0.1805 / D65x);
        let fy = fma_matrix(self.r, 0.2126 / D65y, self.g, 0.7152 / D65y, self.b, 0.0722 / D65y);
        let fz = fma_matrix(self.r, 0.0193 / D65z, self.g, 0.1192 / D65z, self.b, 0.9505 / D65z);

        let X = if fx > EPSILON { cbrt_poly(fx) - 16. / 116. } else { K * fx };
        let Y = if fy > EPSILON { cbrt_poly(fy) - 16. / 116. } else { K * fy };
        let Z = if fz > EPSILON { cbrt_poly(fz) - 16. / 116. } else { K * fz };

        let lab = (
            (Y * 1.05f32), // 1.05 instead of 1.16 to boost color importance without pushing colors outside of 1.0 range
            (500.0 / 220.0f32).mul_add(X - Y, 86.2 / 220.0f32), /* 86 is a fudge to make the value positive */
            (200.0 / 220.0f32).mul_add(Y - Z, 107.9 / 220.0f32), /* 107 is a fudge to make the value positive */
        );
        debug_assert!(lab.0 <= 1.0 && lab.1 <= 1.0 && lab.2 <= 1.0);
        lab
    }
}

/// Cube root initial estimate via the standard bit-manipulation trick
/// (~5-bit accuracy). Cheap integer-only seed for Halley's refinement.
/// `B1 = 709_958_130` is the well-known fast-cbrt constant.
#[inline]
fn cbrt_initial(x: f32) -> f32 {
    const B1: u32 = 709_958_130;
    let ui = x.to_bits();
    let hx = (ui & 0x7FFF_FFFF) / 3 + B1;
    let ui_out = (ui & 0x8000_0000) | hx;
    f32::from_bits(ui_out)
}

/// Fast cube root: bit-trick seed + 2 Halley iterations.
/// Each Halley step roughly triples correct bits (5 → 15 → 45), so the
/// result is bounded by f32 precision (~24 bits), well inside the
/// existing tolerance tests.
#[inline]
fn cbrt_poly(x: f32) -> f32 {
    if x == 0.0 {
        return 0.0;
    }
    let t = cbrt_initial(x);
    // Halley step: t ← t · (2x + t³) / (2t³ + x).
    // Division-first form `t *= num / den` keeps the FMA shape and avoids
    // catastrophic underflow in `t * num` for very small x.
    let r = t * t * t;
    let t = t * x.mul_add(2.0, r) / r.mul_add(2.0, x);
    let r = t * t * t;
    let t = t * x.mul_add(2.0, r) / r.mul_add(2.0, x);
    debug_assert!(t < 1.001);
    debug_assert!(x < 216. / 24389. || t >= 16. / 116.);
    t
}

/// Convert image to L\*a\*b\* planar
///
/// It should return 1 (gray) or 3 (color) planes.
pub trait ToLABBitmap {
    fn to_lab(&self) -> Vec<GBitmap>;
}

impl ToLABBitmap for ImgVec<RGBAPLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}

impl ToLABBitmap for ImgVec<RGBLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}
impl ToLABBitmap for GBitmap {
    fn to_lab(&self) -> Vec<GBitmap> {
        debug_assert!(self.width() > 0);
        let f = |fy| {
            if fy > EPSILON { (cbrt_poly(fy) - 16. / 116.) * 1.16 } else { (K * 1.16) * fy }
        };

        #[cfg(feature = "threads")]
        let out = (0..self.height()).into_par_iter().flat_map_iter(|y| {
            self[y].iter().map(|&fy| f(fy))
        }).collect();

        #[cfg(not(feature = "threads"))]
        let out = self.pixels().map(f).collect();

        vec![Self::new(out, self.width(), self.height())]
    }
}

#[inline(never)]
fn rgb_to_lab<T: Copy + Sync + Send + 'static, F>(img: ImgRef<'_, T>, cb: F) -> Vec<GBitmap>
    where F: Fn(T, usize) -> (f32, f32, f32) + Sync + Send + 'static
{
    let width = img.width();
    assert!(width > 0);
    let height = img.height();
    let area = width * height;

    let mut out_l = Vec::with_capacity(area);
    let mut out_a = Vec::with_capacity(area);
    let mut out_b = Vec::with_capacity(area);

    // For output width == stride
    out_l.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
        out_a.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
            out_b.spare_capacity_mut().par_chunks_exact_mut(width).take(height))
    ).enumerate()
    .for_each(|(y, (l_row, (a_row, b_row)))| {
        let in_row = &img.rows().nth(y).unwrap()[0..width];
        let l_row = &mut l_row[0..width];
        let a_row = &mut a_row[0..width];
        let b_row = &mut b_row[0..width];
        for x in 0..width {
            let n = (x+11) ^ (y+11);
            let (l,a,b) = cb(in_row[x], n);
            l_row[x].write(l);
            a_row[x].write(a);
            b_row[x].write(b);
        }
    });

    unsafe { out_l.set_len(area) };
    unsafe { out_a.set_len(area) };
    unsafe { out_b.set_len(area) };

    vec![
        Img::new(out_l, width, height),
        Img::new(out_a, width, height),
        Img::new(out_b, width, height),
    ]
}

impl ToLABBitmap for ImgRef<'_, RGBAPLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        #[cfg(target_arch = "x86_64")]
        if simd_x86::has_avx2_fma() {
            // SAFETY: capability gate above guarantees AVX2+FMA at runtime.
            return unsafe { simd_x86::rgbaplu_to_lab(*self) };
        }
        rgb_to_lab(*self, |px, n| px.to_rgb(n).to_lab())
    }
}

impl ToLABBitmap for ImgRef<'_, RGBLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        #[cfg(target_arch = "x86_64")]
        if simd_x86::has_avx2_fma() {
            // SAFETY: capability gate above guarantees AVX2+FMA at runtime.
            return unsafe { simd_x86::rgblu_to_lab(*self) };
        }
        rgb_to_lab(*self, |px, _n| px.to_lab())
    }
}

// ── AVX2+FMA SIMD path (runtime-dispatched) ───────────────────────────────
#[cfg(target_arch = "x86_64")]
mod simd_x86 {
    use super::{GBitmap, EPSILON, K, RGBAPLU, RGBLU, D65x, D65y, D65z};
    #[cfg(not(feature = "threads"))]
    use crate::lieon as rayon;
    use core::arch::x86_64::*;
    use imgref::*;
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    // 0 = unknown, 1 = avx2+fma supported, 2 = not supported.
    static CAP: AtomicU8 = AtomicU8::new(0);

    pub(super) fn has_avx2_fma() -> bool {
        match CAP.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => {
                let yes = is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
                CAP.store(if yes { 1 } else { 2 }, Ordering::Relaxed);
                yes
            }
        }
    }

    /// 8-wide cube root: same polynomial seed + 2 Halley iterations as the
    /// scalar `cbrt_poly`, lifted onto __m256. Result is within 1 ULP of
    /// `f32::cbrt` over [0, 1].
    #[inline]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn cbrt_x8(x: __m256) -> __m256 {
        // Polynomial seed: y = -0.5·x² + 1.51·x + 0.2
        let c0 = _mm256_set1_ps(0.2);
        let c1 = _mm256_set1_ps(1.51);
        let c2 = _mm256_set1_ps(-0.5);
        let y = _mm256_fmadd_ps(c2, x, c1);
        let y = _mm256_fmadd_ps(y, x, c0);

        let two = _mm256_set1_ps(2.0);

        // Halley step: y ← y · (2x + y³) / (2y³ + x)
        let y3 = _mm256_mul_ps(_mm256_mul_ps(y, y), y);
        let num = _mm256_fmadd_ps(two, x, y3);
        let den = _mm256_fmadd_ps(two, y3, x);
        let y = _mm256_mul_ps(y, _mm256_div_ps(num, den));

        let y3 = _mm256_mul_ps(_mm256_mul_ps(y, y), y);
        let num = _mm256_fmadd_ps(two, x, y3);
        let den = _mm256_fmadd_ps(two, y3, x);
        _mm256_mul_ps(y, _mm256_div_ps(num, den))
    }

    /// 8-wide RGB-linear → Lab using the same matrix coefficients,
    /// epsilon-clamp, and final Lab transform as scalar `RGBLU::to_lab`.
    #[inline]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn to_lab_x8(r: __m256, g: __m256, b: __m256) -> (__m256, __m256, __m256) { unsafe {
        let m00 = _mm256_set1_ps(0.4124 / D65x);
        let m01 = _mm256_set1_ps(0.3576 / D65x);
        let m02 = _mm256_set1_ps(0.1805 / D65x);
        let m10 = _mm256_set1_ps(0.2126 / D65y);
        let m11 = _mm256_set1_ps(0.7152 / D65y);
        let m12 = _mm256_set1_ps(0.0722 / D65y);
        let m20 = _mm256_set1_ps(0.0193 / D65z);
        let m21 = _mm256_set1_ps(0.1192 / D65z);
        let m22 = _mm256_set1_ps(0.9505 / D65z);

        // fx = m00·r + m01·g + m02·b ; same for fy, fz.
        let fx = _mm256_fmadd_ps(m00, r, _mm256_fmadd_ps(m01, g, _mm256_mul_ps(m02, b)));
        let fy = _mm256_fmadd_ps(m10, r, _mm256_fmadd_ps(m11, g, _mm256_mul_ps(m12, b)));
        let fz = _mm256_fmadd_ps(m20, r, _mm256_fmadd_ps(m21, g, _mm256_mul_ps(m22, b)));

        let eps = _mm256_set1_ps(EPSILON);
        let bias = _mm256_set1_ps(16.0 / 116.0);
        let k = _mm256_set1_ps(K);

        // X = fx > EPSILON ? cbrt(fx) - 16/116 : K · fx
        let cbrt_x = _mm256_sub_ps(cbrt_x8(fx), bias);
        let lin_x = _mm256_mul_ps(k, fx);
        let mask_x = _mm256_cmp_ps::<_CMP_GT_OQ>(fx, eps);
        let x_v = _mm256_blendv_ps(lin_x, cbrt_x, mask_x);

        let cbrt_y = _mm256_sub_ps(cbrt_x8(fy), bias);
        let lin_y = _mm256_mul_ps(k, fy);
        let mask_y = _mm256_cmp_ps::<_CMP_GT_OQ>(fy, eps);
        let y_v = _mm256_blendv_ps(lin_y, cbrt_y, mask_y);

        let cbrt_z = _mm256_sub_ps(cbrt_x8(fz), bias);
        let lin_z = _mm256_mul_ps(k, fz);
        let mask_z = _mm256_cmp_ps::<_CMP_GT_OQ>(fz, eps);
        let z_v = _mm256_blendv_ps(lin_z, cbrt_z, mask_z);

        // L = Y · 1.05;  a = (500/220)·(X-Y) + 86.2/220;  b = (200/220)·(Y-Z) + 107.9/220.
        let one_oh_five = _mm256_set1_ps(1.05);
        let a_scale = _mm256_set1_ps(500.0 / 220.0);
        let a_bias = _mm256_set1_ps(86.2 / 220.0);
        let b_scale = _mm256_set1_ps(200.0 / 220.0);
        let b_bias = _mm256_set1_ps(107.9 / 220.0);

        let l = _mm256_mul_ps(y_v, one_oh_five);
        let a = _mm256_fmadd_ps(a_scale, _mm256_sub_ps(x_v, y_v), a_bias);
        let b_out = _mm256_fmadd_ps(b_scale, _mm256_sub_ps(y_v, z_v), b_bias);
        (l, a, b_out)
    }}

    /// Process one row of RGBLU pixels in 8-pixel chunks; scalar tail.
    /// `l_row`, `a_row`, `b_row` are uninitialized; every cell in `[..width]`
    /// is written before this returns.
    #[target_feature(enable = "avx2,fma")]
    unsafe fn rgblu_row(
        in_row: &[RGBLU],
        l_row: &mut [std::mem::MaybeUninit<f32>],
        a_row: &mut [std::mem::MaybeUninit<f32>],
        b_row: &mut [std::mem::MaybeUninit<f32>],
        width: usize,
    ) { unsafe {
        let chunks = width / 8;

        let mut r_arr = [0.0f32; 8];
        let mut g_arr = [0.0f32; 8];
        let mut b_arr = [0.0f32; 8];

        for c in 0..chunks {
            let base = c * 8;
            for i in 0..8 {
                let p = in_row[base + i];
                r_arr[i] = p.r;
                g_arr[i] = p.g;
                b_arr[i] = p.b;
            }
            let r = _mm256_loadu_ps(r_arr.as_ptr());
            let g = _mm256_loadu_ps(g_arr.as_ptr());
            let b = _mm256_loadu_ps(b_arr.as_ptr());
            let (l, a, b_out) = to_lab_x8(r, g, b);
            _mm256_storeu_ps(l_row.as_mut_ptr().add(base).cast::<f32>(), l);
            _mm256_storeu_ps(a_row.as_mut_ptr().add(base).cast::<f32>(), a);
            _mm256_storeu_ps(b_row.as_mut_ptr().add(base).cast::<f32>(), b_out);
        }

        // Scalar tail
        for i in (chunks * 8)..width {
            let p = in_row[i];
            let (l, a, b_out) = super::ToLAB::to_lab(&p);
            l_row[i].write(l);
            a_row[i].write(a);
            b_row[i].write(b_out);
        }
    }}

    /// Process one row of RGBAPLU pixels with dither in 8-pixel chunks.
    /// Mirrors `to_rgb(n).to_lab()`: composite premul-alpha onto a
    /// dither-checkered ~white background, then convert to Lab.
    /// `n_lane[i] = (x_base + i + 11) ^ (y + 11)` per pixel; channel masks
    /// test bits 16 (R), 8 (G), 32 (B) and add `(1 - a)` when set.
    #[target_feature(enable = "avx2,fma")]
    unsafe fn rgbaplu_row(
        in_row: &[RGBAPLU],
        l_row: &mut [std::mem::MaybeUninit<f32>],
        a_row: &mut [std::mem::MaybeUninit<f32>],
        b_row: &mut [std::mem::MaybeUninit<f32>],
        width: usize,
        y: usize,
    ) { unsafe {
        let chunks = width / 8;

        let one = _mm256_set1_ps(1.0);
        let bit_r = _mm256_set1_epi32(16);
        let bit_g = _mm256_set1_epi32(8);
        let bit_b = _mm256_set1_epi32(32);
        let zero_i = _mm256_setzero_si256();
        let y_xor = _mm256_set1_epi32(((y + 11) as i32) ^ 0);
        // Pixel-index increments for the 8 lanes: (i + 11) for i in 0..8.
        let lane_off = _mm256_setr_epi32(11, 12, 13, 14, 15, 16, 17, 18);

        let mut r_arr = [0.0f32; 8];
        let mut g_arr = [0.0f32; 8];
        let mut b_arr = [0.0f32; 8];
        let mut a_arr = [0.0f32; 8];

        for c in 0..chunks {
            let base = c * 8;
            for i in 0..8 {
                let p = in_row[base + i];
                r_arr[i] = p.r;
                g_arr[i] = p.g;
                b_arr[i] = p.b;
                a_arr[i] = p.a;
            }
            let r = _mm256_loadu_ps(r_arr.as_ptr());
            let g = _mm256_loadu_ps(g_arr.as_ptr());
            let b = _mm256_loadu_ps(b_arr.as_ptr());
            let a = _mm256_loadu_ps(a_arr.as_ptr());

            // n_i = ((x_base + i + 11) as i32) ^ y_xor
            let x_base_v = _mm256_set1_epi32(base as i32);
            let n = _mm256_xor_si256(_mm256_add_epi32(x_base_v, lane_off), y_xor);

            // mask_R = (n & 16) != 0  → ymm of all-1s when set, all-0s otherwise.
            let all_ones = _mm256_set1_epi32(-1);
            let mask_r = _mm256_xor_si256(
                _mm256_cmpeq_epi32(_mm256_and_si256(n, bit_r), zero_i),
                all_ones,
            );
            let mask_g = _mm256_xor_si256(
                _mm256_cmpeq_epi32(_mm256_and_si256(n, bit_g), zero_i),
                all_ones,
            );
            let mask_b = _mm256_xor_si256(
                _mm256_cmpeq_epi32(_mm256_and_si256(n, bit_b), zero_i),
                all_ones,
            );

            let one_minus_a = _mm256_sub_ps(one, a);
            let dither_r = _mm256_and_ps(_mm256_castsi256_ps(mask_r), one_minus_a);
            let dither_g = _mm256_and_ps(_mm256_castsi256_ps(mask_g), one_minus_a);
            let dither_b = _mm256_and_ps(_mm256_castsi256_ps(mask_b), one_minus_a);

            let r = _mm256_add_ps(r, dither_r);
            let g = _mm256_add_ps(g, dither_g);
            let b = _mm256_add_ps(b, dither_b);

            let (l, a_lab, b_lab) = to_lab_x8(r, g, b);
            _mm256_storeu_ps(l_row.as_mut_ptr().add(base).cast::<f32>(), l);
            _mm256_storeu_ps(a_row.as_mut_ptr().add(base).cast::<f32>(), a_lab);
            _mm256_storeu_ps(b_row.as_mut_ptr().add(base).cast::<f32>(), b_lab);
        }

        // Scalar tail: fall back to scalar to_rgb + to_lab for the last <8 pixels.
        for i in (chunks * 8)..width {
            let n = (i + 11) ^ (y + 11);
            let (l, a_lab, b_lab) = super::ToLAB::to_lab(&super::ToRGB::to_rgb(in_row[i], n));
            l_row[i].write(l);
            a_row[i].write(a_lab);
            b_row[i].write(b_lab);
        }
    }}

    /// SAFETY: caller must guarantee AVX2+FMA at runtime.
    pub(super) unsafe fn rgblu_to_lab(img: ImgRef<'_, RGBLU>) -> Vec<GBitmap> {
        let width = img.width();
        let height = img.height();
        assert!(width > 0);
        let area = width * height;

        let mut out_l: Vec<f32> = Vec::with_capacity(area);
        let mut out_a: Vec<f32> = Vec::with_capacity(area);
        let mut out_b: Vec<f32> = Vec::with_capacity(area);

        out_l.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
            out_a.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
                out_b.spare_capacity_mut().par_chunks_exact_mut(width).take(height))
        ).enumerate().for_each(|(y, (l_row, (a_row, b_row)))| {
            let in_row = &img.rows().nth(y).unwrap()[0..width];
            // SAFETY: capability checked by caller.
            unsafe { rgblu_row(in_row, &mut l_row[..width], &mut a_row[..width], &mut b_row[..width], width); }
        });

        // SAFETY: each per-row call wrote every cell in [..width] of its three
        // output rows; combined that's all `area` cells of each Vec.
        unsafe {
            out_l.set_len(area);
            out_a.set_len(area);
            out_b.set_len(area);
        }

        vec![
            Img::new(out_l, width, height),
            Img::new(out_a, width, height),
            Img::new(out_b, width, height),
        ]
    }

    /// SAFETY: caller must guarantee AVX2+FMA at runtime.
    pub(super) unsafe fn rgbaplu_to_lab(img: ImgRef<'_, RGBAPLU>) -> Vec<GBitmap> {
        let width = img.width();
        let height = img.height();
        assert!(width > 0);
        let area = width * height;

        let mut out_l: Vec<f32> = Vec::with_capacity(area);
        let mut out_a: Vec<f32> = Vec::with_capacity(area);
        let mut out_b: Vec<f32> = Vec::with_capacity(area);

        out_l.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
            out_a.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
                out_b.spare_capacity_mut().par_chunks_exact_mut(width).take(height))
        ).enumerate().for_each(|(y, (l_row, (a_row, b_row)))| {
            let in_row = &img.rows().nth(y).unwrap()[0..width];
            // SAFETY: capability checked by caller.
            unsafe { rgbaplu_row(in_row, &mut l_row[..width], &mut a_row[..width], &mut b_row[..width], width, y); }
        });

        // SAFETY: see analogous comment in `rgblu_to_lab`.
        unsafe {
            out_l.set_len(area);
            out_a.set_len(area);
            out_b.set_len(area);
        }

        vec![
            Img::new(out_l, width, height),
            Img::new(out_a, width, height),
            Img::new(out_b, width, height),
        ]
    }
}

#[test]
fn cbrts1() {
    let mut totaldiff = 0.;
    let mut maxdiff: f64 = 0.;
    for i in (0..=10001).rev() {
        let x = (f64::from(i) / 10001.) as f32;
        let a = cbrt_poly(x);
        let actual = a * a * a;
        let expected = x;
        let absdiff = (f64::from(expected) - f64::from(actual)).abs();
        assert!(absdiff < 0.0002, "{expected} - {actual} = {} @ {x}", expected - actual);
        if i % 400 == 0 {
            println!("{:+0.3}", (expected - actual) * 255.);
        }
        totaldiff += absdiff;
        maxdiff = maxdiff.max(absdiff);
    }
    println!("1={totaldiff:0.6}; {maxdiff:0.8}");
    assert!(totaldiff < 0.0025, "{totaldiff}");
}

#[test]
fn cbrts2() {
    let mut totaldiff = 0.;
    let mut maxdiff: f64 = 0.;
    for i in (2000..=10001).rev() {
        let x = f64::from(i) / 10001.;
        let actual = f64::from(cbrt_poly(x as f32));
        let expected = x.cbrt();
        let absdiff = (expected - actual).abs();
        totaldiff += absdiff;
        maxdiff = maxdiff.max(absdiff);
        assert!(absdiff < 0.0000005, "{expected} - {actual} = {} @ {x}", expected - actual);
    }
    println!("2={totaldiff:0.6}; {maxdiff:0.8}");
    assert!(totaldiff < 0.0025, "{totaldiff}");
}
