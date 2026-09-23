//! AVX2 + FMA SIMD `tolab` path for x86_64. Runtime-dispatched on
//! `is_x86_feature_detected!("avx2") && ...!("fma")` cached in an
//! `AtomicU8`. Build-time shortcut when those features are statically
//! enabled (e.g. `-C target-feature=+avx2,+fma` or
//! `-C target-cpu=x86-64-v3`).

use super::{GBitmap, EPSILON, K, RGBAPLU, RGBLU, D65x, D65y, D65z};
use core::arch::x86_64::*;
use imgref::*;
use super::output::{lab_rows_simd, RowWriter};

#[cfg(not(all(target_feature = "avx2", target_feature = "fma")))]
use std::sync::atomic::{AtomicU8, Ordering};

// 0 = unknown, 1 = avx2+fma supported, 2 = not supported.
#[cfg(not(all(target_feature = "avx2", target_feature = "fma")))]
static CAP: AtomicU8 = AtomicU8::new(0);

pub(super) fn has_avx2_fma() -> bool {
    // Statically enabled features need neither runtime detection nor a cache.
    #[cfg(all(target_feature = "avx2", target_feature = "fma"))]
    {
        true
    }
    #[cfg(not(all(target_feature = "avx2", target_feature = "fma")))]
    {
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
}

/// Test helper: run `cbrt_x8` on 8 scalar inputs and return 8 scalar outputs.
/// SAFETY: caller must guarantee AVX2+FMA at runtime.
#[cfg(test)]
#[target_feature(enable = "avx2,fma")]
pub(super) fn cbrt_x8_test(input: [f32; 8]) -> [f32; 8] {
    // SAFETY: `input` is a fully-initialized stack array of 8 f32; the load
    // reads exactly 8 f32 in bounds.
    let v = load_f32(&input);
    let r = cbrt_x8(v);
    let mut out = [0.0f32; 8];
    // SAFETY: `out` is a stack array of 8 f32; the store writes exactly 8 in bounds.
    unsafe { _mm256_storeu_ps(out.as_mut_ptr(), r) };
    out
}

/// 8-wide cube root: same polynomial seed + 2 Halley iterations as the
/// scalar `cbrt_poly`, lifted onto __m256. Result is within 1 ULP of
/// `f32::cbrt` over [0, 1]. Pure value-compute — every intrinsic here is
/// safe-when-target-feature-enabled, so no `unsafe` blocks needed.
#[inline]
#[target_feature(enable = "avx2,fma")]
fn cbrt_x8(x: __m256) -> __m256 {
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
fn to_lab_x8(r: __m256, g: __m256, b: __m256) -> (__m256, __m256, __m256) {
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
}

/// AOS → SOA deinterleave for 8 RGB-f32 pixels (24 floats / 3 ymm vectors).
///
/// `RGB<f32>` is `#[repr(C)]` (verified in upstream `rgb-0.8.53/src/formats/rgb.rs`),
/// so 8 pixels load as three contiguous ymm vectors:
/// - `v0 = [R0 G0 B0 R1 G1 B1 R2 G2]` (lanes 0..7)
/// - `v1 = [B2 R3 G3 B3 R4 G4 B4 R5]`
/// - `v2 = [G5 B5 R6 G6 B6 R7 G7 B7]`
///
/// Per channel: three `vpermps` lift the wanted lanes into the matching
/// output positions, then two `vblendps` merge the three contributions.
/// LLVM further folds shared `vpermps` work and substitutes the cheaper
/// `vshufps` / `vpermpd` where possible, ending around 11 ops + 3 loads
/// per chunk — versus ~45 µops (24 `vmovss` + 21 `vinsertps` + 3
/// `vinsertf128`) emitted by LLVM's autovectorized scalar staging path.
/// Microbench (Zen 4, single-thread, min of 30): RGBLU `to_lab` drops from
/// ~24 cyc/px to ~22 cyc/px at 1024², and 28→27 cyc/px at 2048².
///
#[inline]
#[target_feature(enable = "avx2")]
fn deinterleave_rgb_f32_x8(input: &[RGBLU; 8]) -> (__m256, __m256, __m256) {
    const { assert!(std::mem::size_of::<RGBLU>() == 3 * std::mem::size_of::<f32>()); }
    let ptr = input.as_ptr().cast::<f32>();
    // SAFETY: RGB<f32> is repr(C), with initialized r/g/b fields and no padding.
    // The array borrow covers all 24 floats, and these unaligned loads stay within it.
    let (v0, v1, v2) = unsafe {
        (
            _mm256_loadu_ps(ptr),
            _mm256_loadu_ps(ptr.add(8)),
            _mm256_loadu_ps(ptr.add(16)),
        )
    };

    // R lives at: v0 lanes [0,3,6], v1 lanes [1,4,7], v2 lanes [2,5].
    let r_v0 = _mm256_permutevar8x32_ps(v0, _mm256_setr_epi32(0, 3, 6, 0, 0, 0, 0, 0));
    let r_v1 = _mm256_permutevar8x32_ps(v1, _mm256_setr_epi32(0, 0, 0, 1, 4, 7, 0, 0));
    let r_v2 = _mm256_permutevar8x32_ps(v2, _mm256_setr_epi32(0, 0, 0, 0, 0, 0, 2, 5));
    // 0b00_111_000 = lanes 3,4,5 from r_v1; rest from r_v0.
    let r = _mm256_blend_ps::<0b0011_1000>(r_v0, r_v1);
    // 0b11_000_000 = lanes 6,7 from r_v2; rest stays.
    let r = _mm256_blend_ps::<0b1100_0000>(r, r_v2);

    // G lives at: v0 lanes [1,4,7], v1 lanes [2,5], v2 lanes [0,3,6].
    let g_v0 = _mm256_permutevar8x32_ps(v0, _mm256_setr_epi32(1, 4, 7, 0, 0, 0, 0, 0));
    let g_v1 = _mm256_permutevar8x32_ps(v1, _mm256_setr_epi32(0, 0, 0, 2, 5, 0, 0, 0));
    let g_v2 = _mm256_permutevar8x32_ps(v2, _mm256_setr_epi32(0, 0, 0, 0, 0, 0, 3, 6));
    // 0b00_011_000 = lanes 3,4 from g_v1; rest from g_v0.
    let g = _mm256_blend_ps::<0b0001_1000>(g_v0, g_v1);
    // 0b11_100_000 = lanes 5,6,7 from g_v2.
    let g = _mm256_blend_ps::<0b1110_0000>(g, g_v2);

    // B lives at: v0 lanes [2,5], v1 lanes [0,3,6], v2 lanes [1,4,7].
    let b_v0 = _mm256_permutevar8x32_ps(v0, _mm256_setr_epi32(2, 5, 0, 0, 0, 0, 0, 0));
    let b_v1 = _mm256_permutevar8x32_ps(v1, _mm256_setr_epi32(0, 0, 0, 3, 6, 0, 0, 0));
    let b_v2 = _mm256_permutevar8x32_ps(v2, _mm256_setr_epi32(0, 0, 0, 0, 0, 1, 4, 7));
    // 0b00_011_100 = lanes 2,3,4 from b_v1; rest from b_v0.
    let b = _mm256_blend_ps::<0b0001_1100>(b_v0, b_v1);
    // 0b11_100_000 = lanes 5,6,7 from b_v2.
    let b = _mm256_blend_ps::<0b1110_0000>(b, b_v2);

    (r, g, b)
}

// Memory operations accept fixed-size borrows. Row slicing checks dynamic bounds.
#[inline]
#[target_feature(enable = "avx2,fma")]
fn load_f32(input: &[f32; 8]) -> __m256 {
    // SAFETY: the borrow covers exactly 8 initialized f32s; unaligned loads are allowed.
    unsafe { _mm256_loadu_ps(input.as_ptr()) }
}


/// Process one row of RGBLU pixels in 8-pixel chunks; scalar tail.
/// Writers record every initialized value; the caller checks completeness.
#[target_feature(enable = "avx2,fma")]
fn rgblu_row(
    in_row: &[RGBLU],
    l_row: &mut RowWriter<'_>,
    a_row: &mut RowWriter<'_>,
    b_row: &mut RowWriter<'_>,
    width: usize,
) {
    // Input bounds are checked here; writers check each output write.
    let in_row = &in_row[..width];
    let chunks = width / 8;

    for pixels in in_row.as_chunks::<8>().0 {
        let (r, g, b) = deinterleave_rgb_f32_x8(pixels);
        let (l, a, b) = to_lab_x8(r, g, b);
        l_row.write8(l);
        a_row.write8(a);
        b_row.write8(b);
    }

    // Scalar tail
    for i in (chunks * 8)..width {
        let p = in_row[i];
        let (l, a, b_out) = super::ToLAB::to_lab(&p);
        l_row.write(l);
        a_row.write(a);
        b_row.write(b_out);
    }
}

/// Process one row of RGBAPLU pixels with dither in 8-pixel chunks.
/// Mirrors `to_rgb(n).to_lab()`: composite premul-alpha onto a
/// dither-checkered ~white background, then convert to Lab.
/// `n_lane[i] = (x_base + i + 11) ^ (y + 11)` per pixel; channel masks
/// test bits 16 (R), 8 (G), 32 (B) and add `(1 - a)` when set.
#[target_feature(enable = "avx2,fma")]
fn rgbaplu_row(
    in_row: &[RGBAPLU],
    l_row: &mut RowWriter<'_>,
    a_row: &mut RowWriter<'_>,
    b_row: &mut RowWriter<'_>,
    width: usize,
    y: usize,
) {
    // Input bounds are checked here; writers check each output write.
    let in_row = &in_row[..width];
    let chunks = width / 8;

    let one = _mm256_set1_ps(1.0);
    let bit_r = _mm256_set1_epi32(16);
    let bit_g = _mm256_set1_epi32(8);
    let bit_b = _mm256_set1_epi32(32);
    let zero_i = _mm256_setzero_si256();
    let y_xor = _mm256_set1_epi32((y + 11) as i32);
    // Pixel-index increments for the 8 lanes: (i + 11) for i in 0..8.
    let lane_off = _mm256_setr_epi32(11, 12, 13, 14, 15, 16, 17, 18);

    let mut r_arr = [0.0f32; 8];
    let mut g_arr = [0.0f32; 8];
    let mut b_arr = [0.0f32; 8];
    let mut a_arr = [0.0f32; 8];

    for (c, pixels) in in_row.as_chunks::<8>().0.iter().enumerate() {
        let base = c * 8;
        for i in 0..8 {
            let p = pixels[i];
            r_arr[i] = p.r;
            g_arr[i] = p.g;
            b_arr[i] = p.b;
            a_arr[i] = p.a;
        }
        let (r, g, b, a) = (load_f32(&r_arr), load_f32(&g_arr), load_f32(&b_arr), load_f32(&a_arr));

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
        l_row.write8(l);
        a_row.write8(a_lab);
        b_row.write8(b_lab);
    }

    // Scalar tail: fall back to scalar to_rgb + to_lab for the last <8 pixels.
    for i in (chunks * 8)..width {
        let n = (i + 11) ^ (y + 11);
        let (l, a_lab, b_lab) = super::ToLAB::to_lab(&super::ToRGB::to_rgb(in_row[i], n));
        l_row.write(l);
        a_row.write(a_lab);
        b_row.write(b_lab);
    }
}

/// SAFETY: caller must guarantee AVX2+FMA at runtime.
#[target_feature(enable = "avx2,fma")]
pub(super) fn rgblu_to_lab(img: ImgRef<'_, RGBLU>) -> Vec<GBitmap> {
    lab_rows_simd(img, |row, _y, l, a, b| {
        rgblu_row(row, l, a, b, row.len());
    })
}

/// SAFETY: caller must guarantee AVX2+FMA at runtime.
#[target_feature(enable = "avx2,fma")]
pub(super) fn rgbaplu_to_lab(img: ImgRef<'_, RGBAPLU>) -> Vec<GBitmap> {
    lab_rows_simd(img, |row, y, l, a, b| {
        rgbaplu_row(row, l, a, b, row.len(), y);
    })
}
