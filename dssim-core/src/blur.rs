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

mod portable {
    use super::{K5_INNER, K5_MID, K5_OUTER};
    use imgref::*;
    use std::mem::MaybeUninit;

    /// Horizontal 5-tap blur. Equivalent to two sequential 3-tap horizontal blurs.
    fn blur_h5(src: &[f32], dst: &mut [MaybeUninit<f32>], width: usize, height: usize, src_stride: usize) {
        let last = width - 1;
        for y in 0..height {
            let row = &src[y * src_stride..][..width];
            let out = &mut dst[y * width..][..width];

            // Left edge pixels (0, 1): clamp negative indices to 0
            for i in 0..2.min(width) {
                let m2 = row[0]; // i-2 clamps to 0; if i==1 then i-2 still clamps to 0
                let m1 = if i >= 1 { row[i - 1] } else { row[0] };
                let p1 = if i < last { row[i + 1] } else { row[last] };
                let p2 = if i + 2 <= last { row[i + 2] } else { row[last] };
                out[i].write((m2 + p2) * K5_OUTER + (m1 + p1) * K5_INNER + row[i] * K5_MID);
            }

            // Inner pixels: 2 <= i <= width-3 (no edge clamping)
            for i in 2..width.saturating_sub(2) {
                out[i].write(
                    (row[i - 2] + row[i + 2]) * K5_OUTER
                    + (row[i - 1] + row[i + 1]) * K5_INNER
                    + row[i] * K5_MID,
                );
            }

            // Right edge pixels: clamp beyond-end indices to last
            for i in width.saturating_sub(2).max(2)..width {
                let p1 = if i < last { row[i + 1] } else { row[last] };
                let p2 = if i + 2 <= last { row[i + 2] } else { row[last] };
                out[i].write(
                    (row[i - 2] + p2) * K5_OUTER + (row[i - 1] + p1) * K5_INNER + row[i] * K5_MID,
                );
            }
        }
    }

    /// Vertical 5-tap blur. Equivalent to two sequential 3-tap vertical blurs.
    /// `src` must be tightly packed (stride == width).
    fn blur_v5(src: &[f32], dst: &mut [MaybeUninit<f32>], width: usize, height: usize, dst_stride: usize) {
        let last_y = height - 1;

        for y in 0..height {
            let ym2 = y.saturating_sub(2);
            let ym1 = y.saturating_sub(1);
            let yp1 = (y + 1).min(last_y);
            let yp2 = (y + 2).min(last_y);

            let rm2 = &src[ym2 * width..][..width];
            let rm1 = &src[ym1 * width..][..width];
            let rc = &src[y * width..][..width];
            let rp1 = &src[yp1 * width..][..width];
            let rp2 = &src[yp2 * width..][..width];

            let out = &mut dst[y * dst_stride..][..width];

            for x in 0..width {
                out[x].write(
                    (rm2[x] + rp2[x]) * K5_OUTER + (rm1[x] + rp1[x]) * K5_INNER + rc[x] * K5_MID,
                );
            }
        }
    }

    /// Horizontal 5-tap blur with fused element-wise multiply.
    /// Computes the H5 pass over `src1 * src2` in a single pass, avoiding
    /// materializing the product as a separate buffer.
    #[allow(clippy::too_many_arguments)]
    fn blur_h5_mul(
        src1: &[f32],
        src2: &[f32],
        dst: &mut [MaybeUninit<f32>],
        width: usize,
        height: usize,
        stride1: usize,
        stride2: usize,
    ) {
        let last = width - 1;
        for y in 0..height {
            let r1 = &src1[y * stride1..][..width];
            let r2 = &src2[y * stride2..][..width];
            let out = &mut dst[y * width..][..width];

            // General clamped product access for edge pixels
            let prod = |i: isize| -> f32 {
                let ic = i.max(0).min(last as isize) as usize;
                r1[ic] * r2[ic]
            };

            // Left edge pixels
            for i in 0..2.min(width) {
                let ii = i as isize;
                out[i].write(
                    (prod(ii - 2) + prod(ii + 2)) * K5_OUTER
                    + (prod(ii - 1) + prod(ii + 1)) * K5_INNER
                    + (r1[i] * r2[i]) * K5_MID,
                );
            }

            // Inner pixels: no clamping needed
            for i in 2..width.saturating_sub(2) {
                let pm2 = r1[i - 2] * r2[i - 2];
                let pm1 = r1[i - 1] * r2[i - 1];
                let pc = r1[i] * r2[i];
                let pp1 = r1[i + 1] * r2[i + 1];
                let pp2 = r1[i + 2] * r2[i + 2];
                out[i].write((pm2 + pp2) * K5_OUTER + (pm1 + pp1) * K5_INNER + pc * K5_MID);
            }

            // Right edge pixels
            for i in width.saturating_sub(2).max(2)..width {
                let ii = i as isize;
                out[i].write(
                    (prod(ii - 2) + prod(ii + 2)) * K5_OUTER
                    + (prod(ii - 1) + prod(ii + 1)) * K5_INNER
                    + (r1[i] * r2[i]) * K5_MID,
                );
            }
        }
    }

    /// Promote `&mut [MaybeUninit<f32>]` to `&[f32]` once every cell is written.
    /// SAFETY: every cell of `slice` must have been initialized.
    unsafe fn assume_init_ref(slice: &[MaybeUninit<f32>]) -> &[f32] {
        // SAFETY: f32 and MaybeUninit<f32> have identical layout; caller guarantees init.
        unsafe { std::slice::from_raw_parts(slice.as_ptr().cast::<f32>(), slice.len()) }
    }

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

    /// Blur the element-wise product of two images: `blur(src1 * src2)`.
    /// Fuses the multiply into the horizontal pass, then does a single vertical pass.
    pub fn blur_mul(src1: ImgRef<'_, f32>, src2: ImgRef<'_, f32>, tmp: &mut [MaybeUninit<f32>]) -> Vec<f32> {
        let width = src1.width();
        let height = src1.height();
        debug_assert_eq!(width, src2.width());
        debug_assert_eq!(height, src2.height());
        assert!(width > 0 && width < 1 << 24);
        assert!(height > 0 && height < 1 << 24);

        let pixels = width * height;
        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];

        let mut dst_vec: Vec<f32> = Vec::with_capacity(pixels);
        let dst_uninit: &mut [MaybeUninit<f32>] = &mut dst_vec.spare_capacity_mut()[..pixels];

        blur_h5_mul(
            src1.buf(),
            src2.buf(),
            tmp,
            width,
            height,
            src1.stride(),
            src2.stride(),
        );
        // SAFETY: blur_h5_mul wrote every cell of tmp[..pixels].
        let tmp_init: &[f32] = unsafe { assume_init_ref(tmp) };
        blur_v5(tmp_init, dst_uninit, width, height, width);

        // SAFETY: blur_v5 wrote every cell.
        unsafe { dst_vec.set_len(pixels); }
        dst_vec
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

    let mut tmp = vec![MaybeUninit::uninit(); 1];
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

    let mut tmp = vec![MaybeUninit::uninit(); 5 * 5];
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

    let mut tmp = vec![MaybeUninit::uninit(); 1];
    let dst = blur(ImgRef::new(&src[..], 1, 1), &mut tmp[..]);
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp[..]);

    assert!((dst.buf()[0] - 1.).abs() < 0.00001);
    assert!((src2[0] - 1.).abs() < 0.00001);
}

#[test]
fn blur_two() {
    use std::mem::MaybeUninit;
    let src = vec![
        0., 1., 1., 1.,
        1., 1., 1., 1.,
        1., 1., 1., 1.,
        1., 1., 1., 1.,
    ];
    let mut src2 = src.clone();

    let mut tmp = vec![MaybeUninit::uninit(); 4 * 4];
    let dst = blur(ImgRef::new(&src[..], 4, 4), &mut tmp[..]);
    blur_in_place(ImgRefMut::new(&mut src2[..], 4, 4), &mut tmp[..]);

    assert_eq!(&src2, dst.buf());

    // All-1 corners should remain 1.0 (kernel is normalized)
    assert!((1. - dst.buf()[3]).abs() < 0.0001, "{}", dst.buf()[3]);
    assert!((1. - dst.buf()[3 * 4]).abs() < 0.0001, "{}", dst.buf()[3 * 4]);
    assert!((1. - dst.buf()[4 * 4 - 1]).abs() < 0.0001, "{}", dst.buf()[4 * 4 - 1]);

    // Reference 5-tap computation for corner [0][0]
    let k5o = K5_OUTER;
    let k5i = K5_INNER;
    let k5m = K5_MID;
    let cl = |i: isize, max: usize| i.max(0).min(max as isize) as usize;

    // H5 pass on 4×4
    let mut h = [0.0f32; 16];
    for y in 0..4 {
        for x in 0..4usize {
            let xi = x as isize;
            h[y * 4 + x] = (src[y * 4 + cl(xi - 2, 3)] + src[y * 4 + cl(xi + 2, 3)]) * k5o
                + (src[y * 4 + cl(xi - 1, 3)] + src[y * 4 + cl(xi + 1, 3)]) * k5i
                + src[y * 4 + x] * k5m;
        }
    }
    // V5 pass
    let mut exp_all = [0.0f32; 16];
    for y in 0..4usize {
        for x in 0..4 {
            let yi = y as isize;
            exp_all[y * 4 + x] = (h[cl(yi - 2, 3) * 4 + x] + h[cl(yi + 2, 3) * 4 + x]) * k5o
                + (h[cl(yi - 1, 3) * 4 + x] + h[cl(yi + 1, 3) * 4 + x]) * k5i
                + h[y * 4 + x] * k5m;
        }
    }
    let exp = exp_all[0];
    assert!(
        (f64::from(exp) - f64::from(dst.buf()[0])).abs() < 0.0001,
        "expected {exp}, got {}",
        dst.buf()[0]
    );

}
