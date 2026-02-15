#[cfg(any(test, all(target_os = "macos", not(feature = "no-macos-vimage"))))]
const KERNEL: [f32; 9] = [
    0.095332, 0.118095, 0.095332, 0.118095, 0.146293, 0.118095, 0.095332, 0.118095, 0.095332,
];

#[cfg(all(target_os = "macos", not(feature = "no-macos-vimage")))]
mod mac {
    use super::KERNEL;
    use crate::ffi::vImageConvolve_PlanarF;
    use crate::ffi::vImagePixelCount;
    use crate::ffi::vImage_Buffer;
    use crate::ffi::vImage_Flags::kvImageEdgeExtend;
    use imgref::*;

    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [f32]) -> ImgVec<f32> {
        let width = src.width();
        let height = src.height();

        let srcbuf = vImage_Buffer {
            width: width as vImagePixelCount,
            height: height as vImagePixelCount,
            rowBytes: src.stride() * std::mem::size_of::<f32>(),
            data: src.buf().as_ptr(),
        };
        let mut dst_vec = vec![0f32; width * height];
        let mut dstbuf = vImage_Buffer {
            width: width as vImagePixelCount,
            height: height as vImagePixelCount,
            rowBytes: width * std::mem::size_of::<f32>(),
            data: dst_vec.as_mut_ptr(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, width, height);
        ImgVec::new(dst_vec, width, height)
    }

    pub fn blur_in_place(mut srcdst: ImgRefMut<'_, f32>, tmp: &mut [f32]) {
        let srcbuf = vImage_Buffer {
            width: srcdst.width() as vImagePixelCount,
            height: srcdst.height() as vImagePixelCount,
            rowBytes: srcdst.stride() * std::mem::size_of::<f32>(),
            data: srcdst.buf().as_ptr(),
        };
        let mut dstbuf = vImage_Buffer {
            width: srcdst.width() as vImagePixelCount,
            height: srcdst.height() as vImagePixelCount,
            rowBytes: srcdst.stride() * std::mem::size_of::<f32>(),
            data: srcdst.buf_mut().as_mut_ptr(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, srcdst.width(), srcdst.height());
    }

    /// Blur the element-wise product of two images. On macOS, falls back to
    /// multiply then blur since vImage has no fused variant.
    pub fn blur_mul(src1: ImgRef<'_, f32>, src2: ImgRef<'_, f32>, tmp: &mut [f32]) -> Vec<f32> {
        let width = src1.width();
        let height = src1.height();
        let mut product: Vec<f32> = src1
            .pixels()
            .zip(src2.pixels())
            .map(|(a, b)| a * b)
            .collect();
        blur_in_place(ImgRefMut::new(&mut product, width, height), tmp);
        product
    }

    fn do_blur(
        srcbuf: &vImage_Buffer<*const f32>,
        tmp: &mut [f32],
        dstbuf: &mut vImage_Buffer<*mut f32>,
        width: usize,
        height: usize,
    ) {
        assert_eq!(tmp.len(), width * height);

        unsafe {
            let mut tmpwrbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_mut_ptr(),
            };
            let res = vImageConvolve_PlanarF(
                srcbuf,
                &mut tmpwrbuf,
                std::ptr::null_mut(),
                0,
                0,
                KERNEL.as_ptr(),
                3,
                3,
                0.,
                kvImageEdgeExtend,
            );
            assert_eq!(0, res);

            let tmprbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_ptr(),
            };
            let res = vImageConvolve_PlanarF(
                &tmprbuf,
                dstbuf,
                std::ptr::null_mut(),
                0,
                0,
                KERNEL.as_ptr(),
                3,
                3,
                0.,
                kvImageEdgeExtend,
            );
            assert_eq!(0, res);
        }
    }
}

#[cfg(not(all(target_os = "macos", not(feature = "no-macos-vimage"))))]
mod portable {
    use imgref::*;

    // 1D kernel from separable decomposition of the 3×3 kernel.
    // The original 2D kernel K[r][c] ≈ K1D[r] * K1D[c] (exact to f32 precision).
    // Symmetric: K1D[0] = K1D[2] = K_SIDE, K1D[1] = K_CENTER.
    const K_SIDE: f32 = 0.308_758_86;
    const K_CENTER: f32 = 0.382_482_8;

    /// Horizontal 1D blur. Reads rows with `src_stride`, writes packed rows (stride = width).
    #[inline(never)]
    fn blur_h(src: &[f32], dst: &mut [f32], width: usize, height: usize, src_stride: usize) {
        for y in 0..height {
            let row = &src[y * src_stride..][..width];
            let out = &mut dst[y * width..][..width];

            // Left edge: clamp left neighbor to position 0
            let right = if width > 1 { row[1] } else { row[0] };
            out[0] = (row[0] + right) * K_SIDE + row[0] * K_CENTER;

            // Inner pixels
            for i in 1..width.saturating_sub(1) {
                out[i] = (row[i - 1] + row[i + 1]) * K_SIDE + row[i] * K_CENTER;
            }

            // Right edge: clamp right neighbor to last position
            if width > 1 {
                let i = width - 1;
                out[i] = (row[i - 1] + row[i]) * K_SIDE + row[i] * K_CENTER;
            }
        }
    }

    /// Vertical 1D blur. Reads packed rows (stride = width), writes rows with `dst_stride`.
    #[inline(never)]
    fn blur_v(src: &[f32], dst: &mut [f32], width: usize, height: usize, dst_stride: usize) {
        let mut prev = &src[0..width];
        let mut curr = prev;
        let mut next = prev;

        for y in 0..height {
            prev = curr;
            curr = next;
            next = if y + 1 < height {
                &src[(y + 1) * width..][..width]
            } else {
                curr
            };

            let out = &mut dst[y * dst_stride..][..width];
            for x in 0..width {
                out[x] = (prev[x] + next[x]) * K_SIDE + curr[x] * K_CENTER;
            }
        }
    }

    /// Horizontal blur with fused element-wise multiply.
    /// Computes blur(src1 * src2) by integrating the multiply into the first H pass.
    #[inline(never)]
    fn blur_h_mul(
        src1: &[f32],
        src2: &[f32],
        dst: &mut [f32],
        width: usize,
        height: usize,
        stride1: usize,
        stride2: usize,
    ) {
        for y in 0..height {
            let r1 = &src1[y * stride1..][..width];
            let r2 = &src2[y * stride2..][..width];
            let out = &mut dst[y * width..][..width];

            // Sliding window of products to avoid redundant multiplies
            let mut p_prev = r1[0] * r2[0];
            let mut p_curr = p_prev;
            let mut p_next = if width > 1 { r1[1] * r2[1] } else { p_curr };

            // Left edge: clamp left neighbor to position 0
            out[0] = (p_curr + p_next) * K_SIDE + p_curr * K_CENTER;

            // Inner pixels
            for i in 1..width.saturating_sub(1) {
                p_prev = p_curr;
                p_curr = p_next;
                p_next = r1[i + 1] * r2[i + 1];
                out[i] = (p_prev + p_next) * K_SIDE + p_curr * K_CENTER;
            }

            // Right edge: clamp right neighbor to last position
            if width > 1 {
                let i = width - 1;
                p_prev = p_curr;
                p_curr = p_next;
                out[i] = (p_prev + p_curr) * K_SIDE + p_curr * K_CENTER;
            }
        }
    }

    // ── AVX2+FMA SIMD path ──────────────────────────────────────────────
    #[cfg(all(feature = "fma", target_arch = "x86_64"))]
    mod simd {
        use super::{K_CENTER, K_SIDE};
        use archmage::prelude::*;
        use magetypes::simd::f32x8;

        #[arcane]
        pub fn blur_avx2(
            t: Desktop64,
            buf: &[f32],
            tmp: &mut [f32],
            dst: &mut [f32],
            w: usize,
            h: usize,
            stride: usize,
        ) {
            blur_h_simd(t, buf, tmp, w, h, stride);
            blur_v_simd(t, tmp, dst, w, h, w);
            blur_h_simd(t, dst, tmp, w, h, w);
            blur_v_simd(t, tmp, dst, w, h, w);
        }

        #[arcane]
        pub fn blur_in_place_avx2(
            t: Desktop64,
            buf: &mut [f32],
            tmp: &mut [f32],
            w: usize,
            h: usize,
            stride: usize,
        ) {
            blur_h_simd(t, buf, tmp, w, h, stride);
            blur_v_simd(t, tmp, buf, w, h, stride);
            blur_h_simd(t, buf, tmp, w, h, stride);
            blur_v_simd(t, tmp, buf, w, h, stride);
        }

        #[allow(clippy::too_many_arguments)]
        #[arcane]
        pub fn blur_mul_avx2(
            t: Desktop64,
            src1: &[f32],
            src2: &[f32],
            tmp: &mut [f32],
            dst: &mut [f32],
            w: usize,
            h: usize,
            stride1: usize,
            stride2: usize,
        ) {
            blur_h_mul_simd(t, src1, src2, tmp, w, h, stride1, stride2);
            blur_v_simd(t, tmp, dst, w, h, w);
            blur_h_simd(t, dst, tmp, w, h, w);
            blur_v_simd(t, tmp, dst, w, h, w);
        }

        #[rite]
        fn blur_h_simd(
            t: Desktop64,
            src: &[f32],
            dst: &mut [f32],
            width: usize,
            height: usize,
            src_stride: usize,
        ) {
            let vk_side = f32x8::splat(t, K_SIDE);
            let vk_center = f32x8::splat(t, K_CENTER);

            for y in 0..height {
                let row = &src[y * src_stride..][..width];
                let out = &mut dst[y * width..][..width];

                // Left edge: scalar (1px)
                let right = if width > 1 { row[1] } else { row[0] };
                out[0] = (row[0] + right) * K_SIDE + row[0] * K_CENTER;

                // SIMD loop: process 8 pixels at a time
                // Needs row[i-1..i+7], row[i..i+8], row[i+1..i+9] all valid
                let mut i = 1;
                while i + 9 <= width {
                    let left = f32x8::load(t, (&row[i - 1..i + 7]).try_into().unwrap());
                    let center = f32x8::load(t, (&row[i..i + 8]).try_into().unwrap());
                    let right = f32x8::load(t, (&row[i + 1..i + 9]).try_into().unwrap());
                    let sides = left + right;
                    let result = sides.mul_add(vk_side, center * vk_center);
                    result.store((&mut out[i..i + 8]).try_into().unwrap());
                    i += 8;
                }

                // Scalar tail
                while i < width.saturating_sub(1) {
                    out[i] = (row[i - 1] + row[i + 1]) * K_SIDE + row[i] * K_CENTER;
                    i += 1;
                }

                // Right edge
                if width > 1 {
                    let last = width - 1;
                    out[last] = (row[last - 1] + row[last]) * K_SIDE + row[last] * K_CENTER;
                }
            }
        }

        #[rite]
        fn blur_v_simd(
            t: Desktop64,
            src: &[f32],
            dst: &mut [f32],
            width: usize,
            height: usize,
            dst_stride: usize,
        ) {
            let vk_side = f32x8::splat(t, K_SIDE);
            let vk_center = f32x8::splat(t, K_CENTER);

            for y in 0..height {
                let prev = if y > 0 {
                    &src[(y - 1) * width..][..width]
                } else {
                    &src[0..width]
                };
                let curr = &src[y * width..][..width];
                let next = if y + 1 < height {
                    &src[(y + 1) * width..][..width]
                } else {
                    curr
                };

                let out = &mut dst[y * dst_stride..][..width];

                let mut x = 0;
                while x + 8 <= width {
                    let vprev = f32x8::load(t, (&prev[x..x + 8]).try_into().unwrap());
                    let vcurr = f32x8::load(t, (&curr[x..x + 8]).try_into().unwrap());
                    let vnext = f32x8::load(t, (&next[x..x + 8]).try_into().unwrap());
                    let sides = vprev + vnext;
                    let result = sides.mul_add(vk_side, vcurr * vk_center);
                    result.store((&mut out[x..x + 8]).try_into().unwrap());
                    x += 8;
                }

                // Scalar tail
                while x < width {
                    out[x] = (prev[x] + next[x]) * K_SIDE + curr[x] * K_CENTER;
                    x += 1;
                }
            }
        }

        #[allow(clippy::too_many_arguments)]
        #[rite]
        fn blur_h_mul_simd(
            t: Desktop64,
            src1: &[f32],
            src2: &[f32],
            dst: &mut [f32],
            width: usize,
            height: usize,
            stride1: usize,
            stride2: usize,
        ) {
            let vk_side = f32x8::splat(t, K_SIDE);
            let vk_center = f32x8::splat(t, K_CENTER);

            for y in 0..height {
                let r1 = &src1[y * stride1..][..width];
                let r2 = &src2[y * stride2..][..width];
                let out = &mut dst[y * width..][..width];

                // Left edge: scalar
                let p_curr = r1[0] * r2[0];
                let p_right = if width > 1 { r1[1] * r2[1] } else { p_curr };
                out[0] = (p_curr + p_right) * K_SIDE + p_curr * K_CENTER;

                // SIMD loop: 6 loads per 8px (3 pairs from src1, src2)
                let mut i = 1;
                while i + 9 <= width {
                    let l1 = f32x8::load(t, (&r1[i - 1..i + 7]).try_into().unwrap());
                    let l2 = f32x8::load(t, (&r2[i - 1..i + 7]).try_into().unwrap());
                    let c1 = f32x8::load(t, (&r1[i..i + 8]).try_into().unwrap());
                    let c2 = f32x8::load(t, (&r2[i..i + 8]).try_into().unwrap());
                    let ri1 = f32x8::load(t, (&r1[i + 1..i + 9]).try_into().unwrap());
                    let ri2 = f32x8::load(t, (&r2[i + 1..i + 9]).try_into().unwrap());

                    let p_left = l1 * l2;
                    let p_center = c1 * c2;
                    let p_right = ri1 * ri2;
                    let sides = p_left + p_right;
                    let result = sides.mul_add(vk_side, p_center * vk_center);
                    result.store((&mut out[i..i + 8]).try_into().unwrap());
                    i += 8;
                }

                // Scalar tail
                while i < width.saturating_sub(1) {
                    let pl = r1[i - 1] * r2[i - 1];
                    let pc = r1[i] * r2[i];
                    let pr = r1[i + 1] * r2[i + 1];
                    out[i] = (pl + pr) * K_SIDE + pc * K_CENTER;
                    i += 1;
                }

                // Right edge
                if width > 1 {
                    let last = width - 1;
                    let pl = r1[last - 1] * r2[last - 1];
                    let pc = r1[last] * r2[last];
                    out[last] = (pl + pc) * K_SIDE + pc * K_CENTER;
                }
            }
        }
    }

    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [f32]) -> ImgVec<f32> {
        let width = src.width();
        let height = src.height();
        assert!(width > 0 && width < 1 << 24);
        assert!(height > 0 && height < 1 << 24);
        debug_assert!(src.pixels().all(|p| p.is_finite()));

        let pixels = width * height;
        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];
        let mut dst = vec![0.0f32; pixels];

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::Desktop64::summon() {
                simd::blur_avx2(token, src.buf(), tmp, &mut dst, width, height, src.stride());
                return ImgVec::new(dst, width, height);
            }
        }

        blur_h(src.buf(), tmp, width, height, src.stride());
        blur_v(tmp, &mut dst, width, height, width);
        blur_h(&dst, tmp, width, height, width);
        blur_v(tmp, &mut dst, width, height, width);

        ImgVec::new(dst, width, height)
    }

    pub fn blur_in_place(mut srcdst: ImgRefMut<'_, f32>, tmp: &mut [f32]) {
        let width = srcdst.width();
        let height = srcdst.height();
        let stride = srcdst.stride();
        let pixels = width * height;

        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];
        let buf = srcdst.buf_mut();

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::Desktop64::summon() {
                simd::blur_in_place_avx2(token, buf, tmp, width, height, stride);
                return;
            }
        }

        blur_h(buf, tmp, width, height, stride);
        blur_v(tmp, buf, width, height, stride);
        blur_h(buf, tmp, width, height, stride);
        blur_v(tmp, buf, width, height, stride);
    }

    /// Blur the element-wise product of two images: blur(src1 * src2).
    /// Fuses the multiply into the first horizontal pass to save a full memory
    /// pass and avoid allocating an intermediate product buffer.
    pub fn blur_mul(src1: ImgRef<'_, f32>, src2: ImgRef<'_, f32>, tmp: &mut [f32]) -> Vec<f32> {
        let width = src1.width();
        let height = src1.height();
        debug_assert_eq!(width, src2.width());
        debug_assert_eq!(height, src2.height());
        assert!(width > 0 && width < 1 << 24);
        assert!(height > 0 && height < 1 << 24);

        let pixels = width * height;
        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];
        let mut dst = vec![0.0f32; pixels];

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::Desktop64::summon() {
                simd::blur_mul_avx2(
                    token,
                    src1.buf(),
                    src2.buf(),
                    tmp,
                    &mut dst,
                    width,
                    height,
                    src1.stride(),
                    src2.stride(),
                );
                return dst;
            }
        }

        blur_h_mul(
            src1.buf(),
            src2.buf(),
            tmp,
            width,
            height,
            src1.stride(),
            src2.stride(),
        );
        blur_v(tmp, &mut dst, width, height, width);
        blur_h(&dst, tmp, width, height, width);
        blur_v(tmp, &mut dst, width, height, width);

        dst
    }
}

#[cfg(all(target_os = "macos", not(feature = "no-macos-vimage")))]
pub use self::mac::*;

#[cfg(not(all(target_os = "macos", not(feature = "no-macos-vimage"))))]
pub use self::portable::*;

#[cfg(test)]
use imgref::*;

#[test]
fn blur_zero() {
    let src = vec![0.25];
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 1];
    let dst = blur(ImgRef::new(&src[..], 1, 1), &mut tmp);
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp);

    assert_eq!(&src2, dst.buf());
    assert!((0.25 - dst.buf()[0]).abs() < 0.00001);
}

#[test]
fn blur_one() {
    blur_one_compare(Img::new(
        vec![
            0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0.,
            0., 0., 0.,
        ],
        5,
        5,
    ));
}

#[test]
fn blur_one_stride() {
    let nan = 1. / 0.;
    blur_one_compare(Img::new_stride(
        vec![
            0., 0., 0., 0., 0., nan, -11., 0., 0., 0., 0., 0., 333., nan, 0., 0., 1., 0., 0., nan,
            -11., 0., 0., 0., 0., 0., 333., nan, 0., 0., 0., 0., 0., nan,
        ],
        5,
        5,
        7,
    ));
}

#[cfg(test)]
fn blur_one_compare(src: ImgVec<f32>) {
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 5 * 5];
    let dst = blur(src.as_ref(), &mut tmp);
    blur_in_place(src2.as_mut(), &mut tmp);

    assert_eq!(&src2.pixels().collect::<Vec<_>>(), dst.buf());

    assert!((1. / 110. - dst.buf()[0]).abs() < 0.0001, "{dst:?}");
    assert!((1. / 110. - dst.buf()[5 * 5 - 1]).abs() < 0.0001, "{dst:?}");
    assert!((0.11354011 - dst.buf()[2 * 5 + 2]).abs() < 0.0001);
}

#[test]
fn blur_1x1() {
    let src = vec![1.];
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 1];
    let dst = blur(ImgRef::new(&src[..], 1, 1), &mut tmp);
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp);

    assert!((dst.buf()[0] - 1.).abs() < 0.00001);
    assert!((src2[0] - 1.).abs() < 0.00001);
}

#[test]
fn blur_two() {
    let src = vec![
        0., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1.,
    ];
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 4 * 4];
    let dst = blur(ImgRef::new(&src[..], 4, 4), &mut tmp);
    blur_in_place(ImgRefMut::new(&mut src2[..], 4, 4), &mut tmp);

    assert_eq!(&src2, dst.buf());

    let z00 = 0. * KERNEL[0]
        + 0. * KERNEL[1]
        + 1. * KERNEL[2]
        + 0. * KERNEL[3]
        + 0. * KERNEL[4]
        + 1. * KERNEL[5]
        + 1. * KERNEL[6]
        + 1. * KERNEL[7]
        + 1. * KERNEL[8];
    let z01 = 0. * KERNEL[0]
        + 1. * KERNEL[1]
        + 1. * KERNEL[2]
        + 0. * KERNEL[3]
        + 1. * KERNEL[4]
        + 1. * KERNEL[5]
        + 1. * KERNEL[6]
        + 1. * KERNEL[7]
        + 1. * KERNEL[8];

    let z10 = 0. * KERNEL[0]
        + 0. * KERNEL[1]
        + 1. * KERNEL[2]
        + 1. * KERNEL[3]
        + 1. * KERNEL[4]
        + 1. * KERNEL[5]
        + 1. * KERNEL[6]
        + 1. * KERNEL[7]
        + 1. * KERNEL[8];
    let z11 = 0. * KERNEL[0]
        + 1. * KERNEL[1]
        + 1. * KERNEL[2]
        + 1. * KERNEL[3]
        + 1. * KERNEL[4]
        + 1. * KERNEL[5]
        + 1. * KERNEL[6]
        + 1. * KERNEL[7]
        + 1. * KERNEL[8];
    let exp = z00 * KERNEL[0]
        + z00 * KERNEL[1]
        + z01 * KERNEL[2]
        + z00 * KERNEL[3]
        + z00 * KERNEL[4]
        + z01 * KERNEL[5]
        + z10 * KERNEL[6]
        + z10 * KERNEL[7]
        + z11 * KERNEL[8];

    assert!((1. - dst.buf()[3]).abs() < 0.0001, "{}", dst.buf()[3]);
    assert!(
        (1. - dst.buf()[3 * 4]).abs() < 0.0001,
        "{}",
        dst.buf()[3 * 4]
    );
    assert!(
        (1. - dst.buf()[4 * 4 - 1]).abs() < 0.0001,
        "{}",
        dst.buf()[4 * 4 - 1]
    );
    assert!((f64::from(exp) - f64::from(dst.buf()[0])).abs() < 0.0001);
}
