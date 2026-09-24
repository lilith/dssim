// 1D kernel from separable decomposition of the original 3×3 Gaussian
// (KERNEL = [0.095332, 0.118095, 0.095332, …, 0.146293, …]).
// Symmetric 1D form: K1D = [K_SIDE, K_CENTER, K_SIDE].
const K_SIDE: f32 = 0.308_758_86;
const K_CENTER: f32 = 0.382_482_8;

// Fused double-blur 5-tap kernel: convolving K1D with itself.
// K5 = [K5_OUTER, K5_INNER, K5_MID, K5_INNER, K5_OUTER]
// This makes H→V→H→V (two 3-tap blurs) equivalent to a single H5→V5 pass,
// halving memory traffic.
const K5_OUTER: f32 = K_SIDE * K_SIDE;
const K5_INNER: f32 = 2.0 * K_SIDE * K_CENTER;
const K5_MID: f32 = 2.0 * K_SIDE * K_SIDE + K_CENTER * K_CENTER;

// Edge-pixel coefficients chosen to make this pass *bit-equivalent* to two
// successive 1D 3-tap clamped passes (the upstream double-3×3 form). Derived
// by composing two H1·H1 clamped operations at j=0:
//
//   pass1 at 0: (K_SIDE+K_CENTER)·p[0] + K_SIDE·p[1]
//   pass1 at 1: K_SIDE·p[0] + K_CENTER·p[1] + K_SIDE·p[2]
//   pass2 at 0 = (K_SIDE+K_CENTER)·pass1[0] + K_SIDE·pass1[1]
//              = (K_M + K_I)·p[0] + (K_O + K_I)·p[1] + K_O·p[2]
//
// The single 5-tap with replicated clamps would over-weight p[0] by K_O. The
// inner pixels (j ∈ {1, w-2}) still match the upstream double-3×3 with the
// plain 5-tap form.
const K5_EDGE_CENTER: f32 = K5_MID + K5_INNER;
const K5_EDGE_NEAR: f32 = K5_OUTER + K5_INNER;
const K5_EDGE_FAR: f32 = K5_OUTER;

mod portable {
    use super::{K5_EDGE_CENTER, K5_EDGE_FAR, K5_EDGE_NEAR, K5_INNER, K5_MID, K5_OUTER};
    use imgref::*;
    use std::mem::MaybeUninit;

    /// Interior row pass shared by `blur_h5` and `blur_v5`: plain 5-tap,
    /// `r` ordered [-2, -1, 0, +1, +2] relative to the output element.
    /// `#[inline(always)]` so the AVX2+FMA wrapper re-vectorizes this same
    /// body under its target features instead of duplicating it.
    #[inline(always)]
    fn blur5_inner_inline(r: [&[f32]; 5], out: &mut [MaybeUninit<f32>]) {
        let [m2, m1, c, p1, p2] = r;
        for j in 0..out.len() {
            out[j].write(
                (m2[j] + p2[j]) * K5_OUTER
                + (m1[j] + p1[j]) * K5_INNER
                + c[j] * K5_MID,
            );
        }
    }

    #[inline(never)]
    fn blur5_inner_base(r: [&[f32]; 5], out: &mut [MaybeUninit<f32>]) {
        blur5_inner_inline(r, out);
    }

    /// AVX2+FMA clone of `blur5_inner_base`; same source, vectorized wider.
    /// SAFETY: call only when `caps::has_avx2_fma()` has confirmed support.
    #[cfg(target_arch = "x86_64")]
    #[inline(never)]
    #[target_feature(enable = "avx2,fma")]
    fn blur5_inner_avx2(r: [&[f32]; 5], out: &mut [MaybeUninit<f32>]) {
        blur5_inner_inline(r, out);
    }

    /// x86-64-v4 clone of `blur5_inner_base` (AVX-512 F/BW/DQ/VL —
    /// `avx512cd` unused by these kernels).
    /// SAFETY: call only when `caps::has_avx512_v4()` has confirmed support.
    #[cfg(target_arch = "x86_64")]
    #[inline(never)]
    #[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
    fn blur5_inner_avx512(r: [&[f32]; 5], out: &mut [MaybeUninit<f32>]) {
        blur5_inner_inline(r, out);
    }

    /// Runtime dispatch, resolved once per row — a cached atomic load is
    /// free next to a full row of work, and on statically-enabled builds
    /// the check is a constant `true`.
    #[inline]
    fn blur5_inner(r: [&[f32]; 5], out: &mut [MaybeUninit<f32>]) {
        #[cfg(target_arch = "x86_64")]
        if crate::caps::has_avx512_v4() {
            // SAFETY: has_avx512_v4() confirmed the AVX-512 v4 set.
            unsafe { blur5_inner_avx512(r, out) };
            return;
        }
        #[cfg(target_arch = "x86_64")]
        if crate::caps::has_avx2_fma() {
            // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
            unsafe { blur5_inner_avx2(r, out) };
            return;
        }
        blur5_inner_base(r, out);
    }

    /// Horizontal 5-tap blur, bit-equivalent to two sequential clamped 1D
    /// 3-tap blurs. Edge columns use `h5_edges` (the same legacy-equivalent
    /// clamped-tap math the fused moments pass applies per product).
    fn blur_h5(src: &[f32], dst: &mut [MaybeUninit<f32>], width: usize, height: usize, src_stride: usize) {
        debug_assert!(width >= 1);
        for y in 0..height {
            let row = &src[y * src_stride..][..width];
            let out = &mut dst[y * width..][..width];

            h5_edges(row, row, |a, _b, i| a[i], out);

            // Interior: j ∈ [2, w-2). Five aligned sub-slices so LLVM hoists
            // bounds checks once per row and emits AVX2/NEON SIMD over the body.
            if width >= 5 {
                let inner_len = width - 4;
                let r = [
                    &row[..inner_len],
                    &row[1..=inner_len],
                    &row[2..2 + inner_len],
                    &row[3..3 + inner_len],
                    &row[4..4 + inner_len],
                ];
                blur5_inner(r, &mut out[2..2 + inner_len]);
            }
        }
    }

    /// Clamped vertical tap indices [-2,-1,0,+1,+2] for output row `y` and
    /// the matching edge kind — the single source of truth for `blur_v5`'s
    /// row selection, shared with the fused moments path.
    pub(crate) fn v5_window(y: usize, height: usize) -> ([usize; 5], V5Edge) {
        let last = height - 1;
        let taps = [
            y.saturating_sub(2),
            y.saturating_sub(1),
            y,
            (y + 1).min(last),
            (y + 2).min(last),
        ];
        let edge = if y == 0 {
            V5Edge::Top
        } else if y == last {
            V5Edge::Bottom
        } else {
            V5Edge::None
        };
        (taps, edge)
    }

    /// One output row of the vertical 5-tap combine. `taps` are the five
    /// source rows at clamped indices [-2,-1,0,+1,+2] around the output
    /// row; `edge` picks the H1·H1-derived 3-coefficient form at y=0 and
    /// y=height-1, plain 5-tap everywhere else.
    ///
    /// The `match` sits inside the loop: `edge` is loop-invariant, so
    /// LLVM unswitches it — one source loop, three specialized codegen
    /// paths (the interior one vectorized under AVX2+FMA).
    #[inline(always)]
    fn v5_combine_row_inline(taps: [&[f32]; 5], edge: V5Edge, out: &mut [MaybeUninit<f32>]) {
        let [m2, m1, c, p1, p2] = taps;
        for (x, o) in out.iter_mut().enumerate() {
            o.write(match edge {
                V5Edge::None => {
                    (m2[x] + p2[x]) * K5_OUTER + (m1[x] + p1[x]) * K5_INNER + c[x] * K5_MID
                }
                V5Edge::Top => {
                    K5_EDGE_CENTER * c[x] + K5_EDGE_NEAR * p1[x] + K5_EDGE_FAR * p2[x]
                }
                V5Edge::Bottom => {
                    K5_EDGE_FAR * m2[x] + K5_EDGE_NEAR * m1[x] + K5_EDGE_CENTER * c[x]
                }
            });
        }
    }

    #[inline(never)]
    fn v5_combine_row_base(taps: [&[f32]; 5], edge: V5Edge, out: &mut [MaybeUninit<f32>]) {
        v5_combine_row_inline(taps, edge, out);
    }

    /// AVX2+FMA clone of `v5_combine_row_base`; same source, vectorized
    /// wider. SAFETY: call only when `caps::has_avx2_fma()` has confirmed
    /// support.
    #[cfg(target_arch = "x86_64")]
    #[inline(never)]
    #[target_feature(enable = "avx2,fma")]
    fn v5_combine_row_avx2(taps: [&[f32]; 5], edge: V5Edge, out: &mut [MaybeUninit<f32>]) {
        v5_combine_row_inline(taps, edge, out);
    }

    /// x86-64-v4 clone of `v5_combine_row_base` (AVX-512 F/BW/DQ/VL —
    /// `avx512cd` unused by these kernels).
    /// SAFETY: call only when `caps::has_avx512_v4()` has confirmed support.
    #[cfg(target_arch = "x86_64")]
    #[inline(never)]
    #[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
    fn v5_combine_row_avx512(taps: [&[f32]; 5], edge: V5Edge, out: &mut [MaybeUninit<f32>]) {
        v5_combine_row_inline(taps, edge, out);
    }

    /// Runtime dispatch, resolved once per row.
    #[inline]
    fn v5_combine_row(taps: [&[f32]; 5], edge: V5Edge, out: &mut [MaybeUninit<f32>]) {
        #[cfg(target_arch = "x86_64")]
        if crate::caps::has_avx512_v4() {
            // SAFETY: has_avx512_v4() confirmed the AVX-512 v4 set.
            unsafe { v5_combine_row_avx512(taps, edge, out) };
            return;
        }
        #[cfg(target_arch = "x86_64")]
        if crate::caps::has_avx2_fma() {
            // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
            unsafe { v5_combine_row_avx2(taps, edge, out) };
            return;
        }
        v5_combine_row_base(taps, edge, out);
    }

    /// Vertical 5-tap blur, bit-equivalent to two sequential clamped 1D
    /// 3-tap blurs. `src` must be tightly packed (stride == width).
    fn blur_v5(src: &[f32], dst: &mut [MaybeUninit<f32>], width: usize, height: usize, dst_stride: usize) {
        debug_assert!(height >= 1);
        for y in 0..height {
            let (idx, edge) = v5_window(y, height);
            let taps = idx.map(|i| &src[i * width..][..width]);
            v5_combine_row(taps, edge, &mut dst[y * dst_stride..][..width]);
        }
    }

    /// Scalar edge columns shared by all moment outputs: the same
    /// clamped-tap math as `blur_h5`, applied to an arbitrary per-pixel
    /// product of the two input rows. Writes j ∈ {0, 1, w-2, w-1}
    /// (deduplicated for tiny widths) and leaves the interior untouched.
    #[inline(always)]
    fn h5_edges(
        r1: &[f32],
        r2: &[f32],
        prod: impl Fn(&[f32], &[f32], usize) -> f32,
        out: &mut [MaybeUninit<f32>],
    ) {
        let width = out.len();
        let last = width - 1;
        let p = |i: usize| prod(r1, r2, i);
        out[0].write(K5_EDGE_CENTER * p(0) + K5_EDGE_NEAR * p(1.min(last)) + K5_EDGE_FAR * p(2.min(last)));
        if width >= 2 {
            out[last].write(K5_EDGE_FAR * p(last.saturating_sub(2)) + K5_EDGE_NEAR * p(last - 1) + K5_EDGE_CENTER * p(last));
        }
        if width >= 3 {
            out[1].write((p(0) + p(3.min(last))) * K5_OUTER + (p(0) + p(2.min(last))) * K5_INNER + p(1) * K5_MID);
        }
        if width >= 4 {
            let i = last - 1;
            out[i].write((p(i - 2) + p((i + 2).min(last))) * K5_OUTER + (p(i - 1) + p(i + 1)) * K5_INNER + p(i) * K5_MID);
        }
    }

    /// One row of the fused horizontal moments pass: writes h5(i1), h5(i2),
    /// h5(i1*i1), h5(i2*i2), h5(i1*i2) for a single source row pair.
    /// Six row-reads become two. `rows[p]` must have `r1.len()` cells.
    ///
    /// `#[inline(always)]` so the AVX2+FMA wrapper re-vectorizes this same
    /// body under its target features — five shifted sub-slices per input
    /// row give LLVM the same unit-stride stencil it vectorizes in
    /// `blur5_inner_inline`.
    #[inline(always)]
    fn blur_h5_moments_row_inline(
        r1: &[f32],
        r2: &[f32],
        rows: [&mut [MaybeUninit<f32>]; 5],
    ) {
        let width = r1.len();
        debug_assert!(width >= 1);
        let inner = width.saturating_sub(4);
        let mut rows = rows;

        // Edges: same clamped-tap math as blur_h5, one per product.
        h5_edges(r1, r2, |a, _b, i| a[i], rows[0]);
        h5_edges(r1, r2, |_a, b, i| b[i], rows[1]);
        h5_edges(r1, r2, |a, _b, i| a[i] * a[i], rows[2]);
        h5_edges(r1, r2, |_a, b, i| b[i] * b[i], rows[3]);
        h5_edges(r1, r2, |a, b, i| a[i] * b[i], rows[4]);

        if inner > 0 {
            // Interior x ∈ [2, w-2): tap k of output x+k... i.e. output
            // index k+2 reads input offsets k..k+4 — five aligned
            // sub-slices like `blur_h5` builds for `blur5_inner`.
            let a = [
                &r1[..inner],
                &r1[1..=inner],
                &r1[2..2 + inner],
                &r1[3..3 + inner],
                &r1[4..4 + inner],
            ];
            let b = [
                &r2[..inner],
                &r2[1..=inner],
                &r2[2..2 + inner],
                &r2[3..3 + inner],
                &r2[4..4 + inner],
            ];
            let [d0, d1, d2, d3, d4] = rows.each_mut().map(|r| &mut r[2..2 + inner]);
            for k in 0..inner {
                let (x0, x1, x2, x3, x4) = (a[0][k], a[1][k], a[2][k], a[3][k], a[4][k]);
                let (y0, y1, y2, y3, y4) = (b[0][k], b[1][k], b[2][k], b[3][k], b[4][k]);
                d0[k].write((x0 + x4) * K5_OUTER + (x1 + x3) * K5_INNER + x2 * K5_MID);
                d1[k].write((y0 + y4) * K5_OUTER + (y1 + y3) * K5_INNER + y2 * K5_MID);
                let (p0, p1, p2, p3, p4) = (x0 * x0, x1 * x1, x2 * x2, x3 * x3, x4 * x4);
                d2[k].write((p0 + p4) * K5_OUTER + (p1 + p3) * K5_INNER + p2 * K5_MID);
                let (q0, q1, q2, q3, q4) = (y0 * y0, y1 * y1, y2 * y2, y3 * y3, y4 * y4);
                d3[k].write((q0 + q4) * K5_OUTER + (q1 + q3) * K5_INNER + q2 * K5_MID);
                let (c0, c1, c2, c3, c4) = (x0 * y0, x1 * y1, x2 * y2, x3 * y3, x4 * y4);
                d4[k].write((c0 + c4) * K5_OUTER + (c1 + c3) * K5_INNER + c2 * K5_MID);
            }
        }
    }

    #[inline(never)]
    fn blur_h5_moments_row_base(r1: &[f32], r2: &[f32], rows: [&mut [MaybeUninit<f32>]; 5]) {
        blur_h5_moments_row_inline(r1, r2, rows);
    }

    /// AVX2+FMA clone of `blur_h5_moments_row_base`; same source, vectorized
    /// wider. SAFETY: call only when `caps::has_avx2_fma()` has confirmed
    /// support.
    #[cfg(target_arch = "x86_64")]
    #[inline(never)]
    #[target_feature(enable = "avx2,fma")]
    fn blur_h5_moments_row_avx2(r1: &[f32], r2: &[f32], rows: [&mut [MaybeUninit<f32>]; 5]) {
        blur_h5_moments_row_inline(r1, r2, rows);
    }

    /// x86-64-v4 clone of `blur_h5_moments_row_base` (AVX-512 F/BW/DQ/VL
    /// — `avx512cd` unused by these kernels).
    /// SAFETY: call only when `caps::has_avx512_v4()` has confirmed support.
    #[cfg(target_arch = "x86_64")]
    #[inline(never)]
    #[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
    fn blur_h5_moments_row_avx512(r1: &[f32], r2: &[f32], rows: [&mut [MaybeUninit<f32>]; 5]) {
        blur_h5_moments_row_inline(r1, r2, rows);
    }

    /// Row-level dispatch for the fused horizontal moments pass.
    /// `rows[p]` gets the horizontal blur of `r1`, `r2`, `r1*r1`,
    /// `r2*r2`, `r1*r2` respectively.
    pub fn blur_moments_row(r1: &[f32], r2: &[f32], rows: [&mut [MaybeUninit<f32>]; 5]) {
        #[cfg(target_arch = "x86_64")]
        if crate::caps::has_avx512_v4() {
            // SAFETY: has_avx512_v4() confirmed the AVX-512 v4 set.
            unsafe { blur_h5_moments_row_avx512(r1, r2, rows) };
            return;
        }
        #[cfg(target_arch = "x86_64")]
        if crate::caps::has_avx2_fma() {
            // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
            unsafe { blur_h5_moments_row_avx2(r1, r2, rows) };
            return;
        }
        blur_h5_moments_row_base(r1, r2, rows);
    }

    /// Border kind for `blur_moments_v5_row`.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum V5Edge {
        /// Output row 0: 3-coefficient edge form over the last three taps.
        Top,
        /// Output row height-1: mirrored 3-coefficient edge form.
        Bottom,
        /// Interior or near-edge row: plain clamped 5-tap.
        None,
    }

    /// Vertical 5-tap combine producing one output row for all five moment
    /// planes at once. `taps[p]` = the five H-filtered rows for product `p`
    /// at clamped indices [-2,-1,0,+1,+2] around the output row — i.e. the
    /// same row selection `v5_window` produces for `blur_v5`.
    pub fn blur_moments_v5_row(
        taps: [[&[f32]; 5]; 5],
        edge: V5Edge,
        mut out: [&mut [MaybeUninit<f32>]; 5],
    ) {
        for (p, o) in out.iter_mut().enumerate() {
            v5_combine_row(taps[p], edge, o);
        }
    }

    /// Promote `&mut [MaybeUninit<f32>]` to `&[f32]` once every cell is written.
    /// SAFETY: every cell of `slice` must have been initialized.
    pub(crate) unsafe fn assume_init_ref(slice: &[MaybeUninit<f32>]) -> &[f32] {
        // SAFETY: f32 and MaybeUninit<f32> have identical layout; caller guarantees init.
        unsafe { std::slice::from_raw_parts(slice.as_ptr().cast::<f32>(), slice.len()) }
    }

    #[cfg(test)]
    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [MaybeUninit<f32>]) -> ImgVec<f32> {
        let width = src.width();
        let height = src.height();
        assert!(width > 0 && width < 1 << 24);
        assert!(height > 0 && height < 1 << 24);
        debug_assert!(src.pixels().all(|p| p.is_finite()));

        let pixels = width * height;
        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];

        let mut dst_vec: Vec<f32> = Vec::with_capacity(pixels);
        let dst_uninit: &mut [MaybeUninit<f32>] = &mut dst_vec.spare_capacity_mut()[..pixels];

        blur_h5(src.buf(), tmp, width, height, src.stride());
        // SAFETY: blur_h5 wrote every cell of tmp[..pixels].
        let tmp_init: &[f32] = unsafe { assume_init_ref(tmp) };
        blur_v5(tmp_init, dst_uninit, width, height, width);

        // SAFETY: blur_v5 wrote every cell of dst_vec.spare_capacity_mut().
        unsafe { dst_vec.set_len(pixels); }
        ImgVec::new(dst_vec, width, height)
    }

    pub fn blur_in_place(mut srcdst: ImgRefMut<'_, f32>, tmp: &mut [MaybeUninit<f32>]) {
        let width = srcdst.width();
        let height = srcdst.height();
        let stride = srcdst.stride();
        assert!(width > 0 && width < 1 << 24);
        assert!(height > 0 && height < 1 << 24);

        let pixels = width * height;
        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];

        blur_h5(srcdst.buf(), tmp, width, height, stride);
        // SAFETY: blur_h5 wrote every cell of tmp[..pixels].
        let tmp_init: &[f32] = unsafe { assume_init_ref(tmp) };

        // Reinterpret the (initialized) destination buffer as MaybeUninit so blur_v5
        // can reuse its `&mut [MaybeUninit<f32>]` write path. Every pixel inside the
        // (width,height) window will be overwritten before any further read.
        let dst_buf = srcdst.buf_mut();
        // SAFETY: f32 and MaybeUninit<f32> have the same layout; we overwrite every cell.
        let dst_uninit: &mut [MaybeUninit<f32>] = unsafe {
            std::slice::from_raw_parts_mut(
                dst_buf.as_mut_ptr().cast::<MaybeUninit<f32>>(),
                dst_buf.len(),
            )
        };

        blur_v5(tmp_init, dst_uninit, width, height, stride);
    }

    /// Scalar vs AVX2 parity for the dispatched interior kernel, over an
    /// odd-length range so the SIMD tail is exercised.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn blur5_inner_dispatch_parity() {
        if !crate::caps::has_avx2_fma() {
            return;
        }
        let n = 253;
        let src: Vec<f32> = (0..n + 4)
            .map(|i| ((i as u32).wrapping_mul(747_796_405) >> 8) as f32 / 16_777_216.0)
            .collect();
        let r = [
            &src[..n], &src[1..=n], &src[2..2 + n], &src[3..3 + n], &src[4..4 + n],
        ];
        let mut base = vec![MaybeUninit::<f32>::uninit(); n];
        let mut avx2 = vec![MaybeUninit::<f32>::uninit(); n];
        blur5_inner_base(r, &mut base);
        // SAFETY: has_avx2_fma() confirmed support above.
        unsafe {
            blur5_inner_avx2(r, &mut avx2);
        }
        for j in 0..n {
            let (a, b) = unsafe { (base[j].assume_init(), avx2[j].assume_init()) };
            assert!((f64::from(a) - f64::from(b)).abs() < 1e-6,
                "blur5_inner diverged at {j}: base={a} avx2={b}");
        }
    }
}

pub use self::portable::*;

#[cfg(test)]
use imgref::*;

#[test]
fn blur_zero() {
    use std::mem::MaybeUninit;
    let src = vec![0.25];
    let mut src2 = src.clone();

    let mut tmp = [MaybeUninit::uninit(); 1];
    let dst = blur(ImgRef::new(&src[..], 1, 1), &mut tmp[..]);
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp[..]);

    assert_eq!(&src2, dst.buf());
    assert!((0.25 - dst.buf()[0]).abs() < 0.00001);
}

#[test]
fn blur_one() {
    blur_one_compare(Img::new(vec![
        0.,0.,0.,0.,0.,
        0.,0.,0.,0.,0.,
        0.,0.,1.,0.,0.,
        0.,0.,0.,0.,0.,
        0.,0.,0.,0.,0.,
    ], 5, 5));
}

#[test]
fn blur_one_stride() {
    let nan = 1./0.;
    blur_one_compare(Img::new_stride(vec![
        0.,0.,0.,0.,0., nan, -11.,
        0.,0.,0.,0.,0., 333., nan,
        0.,0.,1.,0.,0., nan, -11.,
        0.,0.,0.,0.,0., 333., nan,
        0.,0.,0.,0.,0., nan,
    ], 5, 5, 7));
}

#[cfg(test)]
fn blur_one_compare(src: ImgVec<f32>) {
    use std::mem::MaybeUninit;
    let mut src2 = src.clone();

    let mut tmp = [MaybeUninit::uninit(); 5 * 5];
    let dst = blur(src.as_ref(), &mut tmp[..]);
    blur_in_place(src2.as_mut(), &mut tmp[..]);

    assert_eq!(&src2.pixels().collect::<Vec<_>>(), dst.buf());

    assert!((1. / 110. - dst.buf()[0]).abs() < 0.0001, "{dst:?}");
    assert!((1. / 110. - dst.buf()[5 * 5 - 1]).abs() < 0.0001, "{dst:?}");
    assert!((0.11354011 - dst.buf()[2 * 5 + 2]).abs() < 0.0001);
}

#[test]
fn blur_1x1() {
    use std::mem::MaybeUninit;
    let src = vec![1.];
    let mut src2 = src.clone();

    let mut tmp = [MaybeUninit::uninit(); 1];
    let dst = blur(ImgRef::new(&src[..], 1, 1), &mut tmp[..]);
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp[..]);

    assert!((dst.buf()[0] - 1.).abs() < 0.00001);
    assert!((src2[0] - 1.).abs() < 0.00001);
}

#[test]
fn blur_two() {
    use std::mem::MaybeUninit;
    // 4×4 image with a 0 at corner (0,0) and 1s elsewhere. The blur should:
    //   - keep the far corners at 1 (kernel sums to 1, all neighbors are 1);
    //   - pull corner (0,0) up toward 1 by exactly the amount the legacy
    //     double-3×3 blur would (this branch's blur is bit-equivalent to it).
    let src = vec![
        0., 1., 1., 1.,
        1., 1., 1., 1.,
        1., 1., 1., 1.,
        1., 1., 1., 1.,
    ];
    let mut src2 = src.clone();

    let mut tmp = [MaybeUninit::uninit(); 4 * 4];
    let dst = blur(ImgRef::new(&src[..], 4, 4), &mut tmp[..]);
    blur_in_place(ImgRefMut::new(&mut src2[..], 4, 4), &mut tmp[..]);

    assert_eq!(&src2, dst.buf());

    // All-1 corners should remain 1.0 (kernel is normalized).
    assert!((1. - dst.buf()[3]).abs() < 0.0001, "{}", dst.buf()[3]);
    assert!((1. - dst.buf()[3 * 4]).abs() < 0.0001, "{}", dst.buf()[3 * 4]);
    assert!((1. - dst.buf()[4 * 4 - 1]).abs() < 0.0001, "{}", dst.buf()[4 * 4 - 1]);

    // Locked corner-(0,0) value: this is exactly what two clamped 1D 3-tap
    // passes (i.e. the upstream double-3×3 blur) produce on this fixture, to
    // four decimal places. The new fused 5-tap with the H1·H1-derived edge
    // weights matches it bit-for-bit modulo FP reordering. The
    // `blur_equiv` tests further down validate the equivalence over a
    // wider battery of inputs.
    let expected = 0.671_504_5_f32;
    assert!(
        (f64::from(expected) - f64::from(dst.buf()[0])).abs() < 0.0001,
        "expected {expected}, got {}",
        dst.buf()[0]
    );
}

// Equivalence test against the upstream double-3×3 blur lives in a separate
// file to keep this module focused on the algorithm. See
// `blur/equiv_tests.rs` for the legacy reference impl and per-pixel parity
// sweep.
#[cfg(test)]
mod equiv_tests;
