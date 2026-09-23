//! Equivalence test: this branch's fused 5-tap blur vs. the upstream
//! double-3×3 blur. The legacy `do_blur` (single 3×3 pass, edge-clamped)
//! is ported inline as `legacy_do_blur` and run twice to mirror upstream's
//! `blur(src) = do_blur(do_blur(src))`. With the H1·H1-derived edge
//! weights from `blur.rs`, the new blur is bit-equivalent (modulo FP
//! reordering) to upstream across the entire image, including the 4-pixel
//! boundary ring. The tests below assert per-pixel diff, MAE, and
//! strided-vs-tight equivalence ≤ 5×10⁻⁶ over a battery of distortion
//! patterns: constants, gradients, random uniform (deterministic
//! xorshift32 — no `rand` dev-dep), step edges, single-pixel impulses,
//! strided sub-images, and a tiny-size sweep.

    use imgref::*;
    use std::mem::MaybeUninit;

    /// Upstream's 3×3 separable Gaussian (kept as a literal reference).
    const REF_KERNEL: [f32; 9] = [
        0.095332, 0.118095, 0.095332,
        0.118095, 0.146293, 0.118095,
        0.095332, 0.118095, 0.095332,
    ];

    /// Single 3×3 pass with edge-clamped reads — matches upstream `do_blur`.
    fn legacy_do_blur(src: &[f32], width: usize, height: usize, src_stride: usize) -> Vec<f32> {
        let mut dst = vec![0.0f32; width * height];
        for y in 0..height {
            let prev_y = y.saturating_sub(1);
            let next_y = (y + 1).min(height - 1);
            let prev = &src[prev_y * src_stride..][..width];
            let curr = &src[y * src_stride..][..width];
            let next = &src[next_y * src_stride..][..width];
            for x in 0..width {
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(width - 1);
                let v = prev[xm].mul_add(REF_KERNEL[0], prev[x] * REF_KERNEL[1])
                    + prev[xp].mul_add(REF_KERNEL[2], curr[xm] * REF_KERNEL[3])
                    + curr[x].mul_add(REF_KERNEL[4], curr[xp] * REF_KERNEL[5])
                    + next[xm].mul_add(REF_KERNEL[6], next[x] * REF_KERNEL[7])
                    + next[xp] * REF_KERNEL[8];
                dst[y * width + x] = v;
            }
        }
        dst
    }

    /// Reference blur: two sequential 3×3 passes, matching upstream `blur`.
    fn legacy_blur(src: ImgRef<'_, f32>) -> Vec<f32> {
        let w = src.width();
        let h = src.height();
        let stride = src.stride();
        // First pass operates on the (possibly strided) source.
        let pass1 = legacy_do_blur(src.buf(), w, h, stride);
        // Second pass operates on the (now tightly packed) intermediate.
        legacy_do_blur(&pass1, w, h, w)
    }

    /// Compute (`interior_max_abs_err`, `boundary_max_abs_err`, `mean_abs_err`)
    /// between this branch's blur and the legacy double-3×3 blur. "Interior"
    /// is anywhere ≥ 4 pixels from any edge.
    fn compare(src: &ImgVec<f32>) -> (f64, f64, f64) {
        let w = src.width();
        let h = src.height();
        let mut tmp: Vec<MaybeUninit<f32>> = (0..w * h).map(|_| MaybeUninit::uninit()).collect();
        let new_out = super::blur(src.as_ref(), &mut tmp);
        let legacy = legacy_blur(src.as_ref());

        let mut interior_max: f64 = 0.0;
        let mut boundary_max: f64 = 0.0;
        let mut sum_abs: f64 = 0.0;
        let mut count: usize = 0;
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let diff = (f64::from(new_out.buf()[idx]) - f64::from(legacy[idx])).abs();
                let interior = x >= 4 && y >= 4 && x + 4 < w && y + 4 < h;
                if interior {
                    interior_max = interior_max.max(diff);
                } else {
                    boundary_max = boundary_max.max(diff);
                }
                sum_abs += diff;
                count += 1;
            }
        }
        (interior_max, boundary_max, sum_abs / count.max(1) as f64)
    }

    /// Tiny xorshift PRNG so the tests stay deterministic without pulling
    /// `rand` into dev-deps.
    fn xorshift32(state: &mut u32) -> u32 {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        *state
    }

    fn random_image(w: usize, h: usize, seed: u32) -> ImgVec<f32> {
        let mut s = seed;
        let buf: Vec<f32> = (0..w * h)
            .map(|_| (xorshift32(&mut s) as f32) / (u32::MAX as f32))
            .collect();
        ImgVec::new(buf, w, h)
    }

    fn linear_gradient(w: usize, h: usize) -> ImgVec<f32> {
        let buf: Vec<f32> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x + y) as f32 / (w + h) as f32))
            .collect();
        ImgVec::new(buf, w, h)
    }

    fn step_edge(w: usize, h: usize) -> ImgVec<f32> {
        let buf: Vec<f32> = (0..h)
            .flat_map(|y| (0..w).map(move |x| if x < w / 2 || y < h / 2 { 0.0 } else { 1.0 }))
            .collect();
        ImgVec::new(buf, w, h)
    }

    fn impulse(w: usize, h: usize) -> ImgVec<f32> {
        let mut buf = vec![0.0f32; w * h];
        buf[(h / 2) * w + (w / 2)] = 1.0;
        ImgVec::new(buf, w, h)
    }

    /// Bit-equivalent (modulo FP reordering) bound: 5×10⁻⁶ everywhere.
    /// The fused 5-tap with the H1·H1-derived edge weights agrees with the
    /// upstream double-3×3 in every pixel; any larger divergence is a bug.
    fn assert_bounds(name: &str, (interior, boundary, mae): (f64, f64, f64)) {
        const TOL: f64 = 5e-6;
        eprintln!(
            "{name}: interior_max={interior:.3e}  boundary_max={boundary:.3e}  mae={mae:.3e}"
        );
        assert!(
            interior <= TOL,
            "{name}: interior diverged ({interior:.3e} > {TOL:.0e})"
        );
        assert!(
            boundary <= TOL,
            "{name}: boundary diverged ({boundary:.3e} > {TOL:.0e})"
        );
        assert!(
            mae <= TOL,
            "{name}: MAE diverged ({mae:.3e} > {TOL:.0e})"
        );
    }

    #[test]
    fn equiv_constant_50() {
        // Constant 0.5 — both blurs must return ≈ 0.5 everywhere (kernel sums to 1).
        let img = ImgVec::new(vec![0.5f32; 64 * 48], 64, 48);
        assert_bounds("constant_50", compare(&img));
    }

    #[test]
    fn equiv_constant_zero() {
        let img = ImgVec::new(vec![0.0f32; 32 * 32], 32, 32);
        assert_bounds("constant_zero", compare(&img));
    }

    #[test]
    fn equiv_linear_gradient() {
        // Linear ramps are preserved exactly by separable smoothing in the interior.
        assert_bounds("linear_gradient", compare(&linear_gradient(96, 64)));
    }

    #[test]
    fn equiv_random_uniform() {
        // Three different sizes, each with a different PRNG seed.
        for &(w, h, seed) in &[(64usize, 64usize, 0xCAFEBABE_u32), (97, 53, 0x1234_5678), (128, 96, 0xDEAD_BEEF)] {
            assert_bounds(
                &format!("random_uniform_{w}x{h}_seed{seed:08x}"),
                compare(&random_image(w, h, seed)),
            );
        }
    }

    #[test]
    fn equiv_step_edge() {
        // Sharp step — the boundary case is more aggressive but still bounded.
        assert_bounds("step_edge", compare(&step_edge(96, 96)));
    }

    #[test]
    fn equiv_impulse() {
        // Single-pixel impulse at center — interior must reproduce the exact
        // impulse response of the kernel.
        assert_bounds("impulse_64x64", compare(&impulse(64, 64)));
        assert_bounds("impulse_33x37", compare(&impulse(33, 37)));
    }

    #[test]
    fn equiv_strided_subimage() {
        // Allocate 96×64 with a 96-px stride, view the inner 80×48.
        let full = random_image(96, 64, 0xA5A5_A5A5);
        let sub = full.as_ref().sub_image(8, 4, 80, 48);
        // Materialize a tightly-packed copy as the reference so we compare
        // strided-vs-tight for THIS branch's blur (the legacy reference is
        // re-derived from the strided view internally).
        let w = sub.width();
        let h = sub.height();
        let tight: Vec<f32> = sub.pixels().collect();
        let tight = ImgVec::new(tight, w, h);

        let mut tmp: Vec<MaybeUninit<f32>> = (0..w * h).map(|_| MaybeUninit::uninit()).collect();
        let strided_out = super::blur(sub, &mut tmp);
        let mut tmp2: Vec<MaybeUninit<f32>> = (0..w * h).map(|_| MaybeUninit::uninit()).collect();
        let tight_out = super::blur(tight.as_ref(), &mut tmp2);

        // Strided and tight inputs must produce identical outputs.
        let mut max_diff: f64 = 0.0;
        for i in 0..w * h {
            let d = (f64::from(strided_out.buf()[i]) - f64::from(tight_out.buf()[i])).abs();
            max_diff = max_diff.max(d);
        }
        eprintln!("strided_subimage: max stride/tight diff={max_diff:.3e}");
        assert!(max_diff < 1e-6, "strided blur diverged from tight blur: {max_diff:.3e}");

        // And both must match the legacy 3×3-twice reference within the
        // interior+boundary bounds.
        assert_bounds("strided_subimage", compare(&tight));
    }

    #[test]
    fn equiv_tiny_sizes() {
        // Sweep tiny widths/heights, including the corners around the
        // 5-tap inner-loop kink (width=5 is the threshold). Now that
        // boundary handling matches H1·H1 exactly, even fully-boundary
        // images (w<5 or h<5) must agree to FP-reordering precision.
        for w in 1..=8 {
            for h in 1..=8 {
                let img = random_image(w, h, 0xBEEF_F00D_u32.wrapping_add((w * 99 + h) as u32));
                assert_bounds(&format!("tiny_{w}x{h}"), compare(&img));
            }
        }
    }

    /// `v5_window` produces the same clamped tap indices and edge kinds the
    /// old inline `blur_v5` code used — pinned as an explicit table so a
    /// regression in the shared row-selection logic fails here, not in
    /// downstream map diffs.
    #[test]
    fn v5_window_table() {
        use super::V5Edge::{Bottom, None as Mid, Top};
        for (h, y, exp_taps, exp_edge) in [
            (1usize, 0usize, [0, 0, 0, 0, 0], Top),
            (2, 0, [0, 0, 0, 1, 1], Top),
            (2, 1, [0, 0, 1, 1, 1], Bottom),
            (3, 0, [0, 0, 0, 1, 2], Top),
            (3, 1, [0, 0, 1, 2, 2], Mid),
            (3, 2, [0, 1, 2, 2, 2], Bottom),
            (4, 1, [0, 0, 1, 2, 3], Mid),
            (4, 2, [0, 1, 2, 3, 3], Mid),
            (5, 2, [0, 1, 2, 3, 4], Mid),
            (9, 0, [0, 0, 0, 1, 2], Top),
            (9, 1, [0, 0, 1, 2, 3], Mid),
            (9, 6, [4, 5, 6, 7, 8], Mid),
            (9, 7, [5, 6, 7, 8, 8], Mid),
            (9, 8, [6, 7, 8, 8, 8], Bottom),
        ] {
            let (taps, edge) = super::v5_window(y, h);
            assert_eq!(taps, exp_taps, "h={h} y={y} taps");
            assert_eq!(edge, exp_edge, "h={h} y={y} edge");
        }
    }

    /// `blur_moments_row` must equal an independent per-pixel clamped
    /// 5-tap reference for each of the five products — catches tap-offset
    /// and lane-indexing bugs in the fused kernel (bitwise: the reference
    /// uses the identical expression tree).
    #[test]
    fn moments_row_matches_scalar_reference() {
        use super::{K5_EDGE_CENTER, K5_EDGE_FAR, K5_EDGE_NEAR, K5_INNER, K5_MID, K5_OUTER};

        let ref_row = |prod: &dyn Fn(usize) -> f32, w: usize| -> Vec<f32> {
            let last = w - 1;
            let p = |i: isize| prod((i.max(0) as usize).min(last));
            (0..w).map(|x| {
                let x = x as isize;
                if x == 0 {
                    K5_EDGE_CENTER * p(0) + K5_EDGE_NEAR * p(1) + K5_EDGE_FAR * p(2)
                } else if x == last as isize && w >= 2 {
                    K5_EDGE_FAR * p(x - 2) + K5_EDGE_NEAR * p(x - 1) + K5_EDGE_CENTER * p(x)
                } else if x == 1 && w >= 3 {
                    (p(0) + p(3)) * K5_OUTER + (p(0) + p(2)) * K5_INNER + p(1) * K5_MID
                } else if x == last as isize - 1 && w >= 4 {
                    (p(x - 2) + p(x + 2)) * K5_OUTER + (p(x - 1) + p(x + 1)) * K5_INNER + p(x) * K5_MID
                } else {
                    (p(x - 2) + p(x + 2)) * K5_OUTER + (p(x - 1) + p(x + 1)) * K5_INNER + p(x) * K5_MID
                }
            }).collect()
        };

        for w in [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 16, 17, 31, 64] {
            let mut s = 0xABCD_1234u32;
            let r1: Vec<f32> = (0..w).map(|_| (xorshift32(&mut s) as f32) / (u32::MAX as f32)).collect();
            let r2: Vec<f32> = (0..w).map(|_| (xorshift32(&mut s) as f32) / (u32::MAX as f32)).collect();
            let mut bufs: [Vec<MaybeUninit<f32>>; 5] =
                core::array::from_fn(|_| vec![MaybeUninit::uninit(); w]);
            super::blur_moments_row(&r1, &r2, bufs.each_mut().map(|b| &mut b[..]));
            let prods: [Box<dyn Fn(usize) -> f32>; 5] = [
                Box::new(|i| r1[i]),
                Box::new(|i| r2[i]),
                Box::new(|i| r1[i] * r1[i]),
                Box::new(|i| r2[i] * r2[i]),
                Box::new(|i| r1[i] * r2[i]),
            ];
            for (p, prod) in prods.iter().enumerate() {
                let expected = ref_row(&**prod, w);
                // SAFETY: blur_moments_row wrote all w cells of each row.
                let got: &[f32] = unsafe { super::assume_init_ref(&bufs[p]) };
                assert_eq!(got, expected.as_slice(), "w={w} product {p}");
            }
        }
    }
