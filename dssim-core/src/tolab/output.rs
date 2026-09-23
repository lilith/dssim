//! Checked construction of three uninitialized f32 output planes.
use super::GBitmap;
use imgref::{Img, ImgRef};
use std::mem::MaybeUninit;
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use rayon::prelude::*;

/// All slots before `remaining` are initialized. Fields and construction stay
/// private: safe callers can initialize more values, but cannot skip a slot.
pub(super) struct RowWriter<'a> {
    remaining: &'a mut [MaybeUninit<f32>],
}

impl RowWriter<'_> {
    #[inline]
    pub(super) fn write(&mut self, value: f32) {
        self.remaining[0].write(value);
        // The write above checked that at least one slot exists. No fallible
        // operation occurs between taking the slice and restoring its tail.
        self.remaining = &mut std::mem::take(&mut self.remaining)[1..];
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    #[target_feature(enable = "avx")]
    pub(super) fn write8(&mut self, value: core::arch::x86_64::__m256) {
        let dst = self.remaining.first_chunk_mut::<8>()
            .expect("SIMD write exceeds output row");
        // SAFETY: the exclusive array borrow covers eight f32-sized slots;
        // unaligned stores are allowed. Advance only after all slots are written.
        unsafe { core::arch::x86_64::_mm256_storeu_ps(dst.as_mut_ptr().cast::<f32>(), value) };
        self.remaining = &mut std::mem::take(&mut self.remaining)[8..];
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    #[target_feature(enable = "neon")]
    pub(super) fn write4(&mut self, value: core::arch::aarch64::float32x4_t) {
        let dst = self.remaining.first_chunk_mut::<4>()
            .expect("SIMD write exceeds output row");
        // SAFETY: the exclusive array borrow covers four f32-sized slots;
        // unaligned stores are allowed. Advance only after all slots are written.
        unsafe { core::arch::aarch64::vst1q_f32(dst.as_mut_ptr().cast::<f32>(), value) };
        self.remaining = &mut std::mem::take(&mut self.remaining)[4..];
    }

    fn assert_complete(&self) {
        assert!(self.remaining.is_empty(), "incomplete output row");
    }
}

/// The callback only borrows writers. This function owns their completion
/// checks, so an early return from the callback cannot publish unwritten slots.
macro_rules! define_lab_rows {
    ($name:ident $(, $feature:literal)?) => {
        $(#[target_feature(enable = $feature)])?
pub(super) fn $name<T, F>(img: ImgRef<'_, T>, fill: F) -> Vec<GBitmap>
where
    T: Copy + Sync,
    F: Fn(&[T], usize, &mut RowWriter<'_>, &mut RowWriter<'_>, &mut RowWriter<'_>) + Sync + Send,
{
    let width = img.width();
    let height = img.height();
    assert!(width > 0);
    let area = width.checked_mul(height).expect("image area overflow");
    let mut planes: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::with_capacity(area));
    let [out_l, out_a, out_b] = &mut planes;
    out_l.spare_capacity_mut()[..area].par_chunks_exact_mut(width)
        .zip(out_a.spare_capacity_mut()[..area].par_chunks_exact_mut(width))
        .zip(out_b.spare_capacity_mut()[..area].par_chunks_exact_mut(width))
        .enumerate().for_each(|(y, ((l, a), b))| {
            let input = &img.rows().nth(y).unwrap()[..width];
            let mut l = RowWriter { remaining: &mut l[..width] };
            let mut a = RowWriter { remaining: &mut a[..width] };
            let mut b = RowWriter { remaining: &mut b[..width] };
            fill(input, y, &mut l, &mut a, &mut b);
            l.assert_complete();
            a.assert_complete();
            b.assert_complete();
        });

    for plane in &mut planes {
        // SAFETY: equal-length row partitions cover exactly area slots in each
        // plane. Every writer passed its completion check. If any callback or
        // check panics, execution never reaches here. f32 needs no drop handling
        // for partially initialized buffers on the panic path.
        unsafe { plane.set_len(area) };
    }
    planes.into_iter().map(|plane| Img::new(plane, width, height)).collect()
}
    };
}

define_lab_rows!(lab_rows);
#[cfg(target_arch = "x86_64")]
define_lab_rows!(lab_rows_simd, "avx2,fma");
#[cfg(target_arch = "aarch64")]
define_lab_rows!(lab_rows_simd, "neon");

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[test]
    fn all_rows_and_planes_are_written() {
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let out = lab_rows(ImgRef::new(&input, 3, 2), |row, _, l, a, b| {
            for &v in row { l.write(v); a.write(v + 10.0); b.write(v + 20.0); }
        });
        assert_eq!(out[0].buf(), &input);
        assert_eq!(out[1].buf(), &[11.0, 12.0, 13.0, 14.0, 15.0, 16.0]);
        assert_eq!(out[2].buf(), &[21.0, 22.0, 23.0, 24.0, 25.0, 26.0]);
    }

    #[test]
    fn incomplete_callback_cannot_publish_output() {
        for missing in 0..3 {
            let result = catch_unwind(|| lab_rows(ImgRef::new(&[1.0; 9], 3, 3), |row, y, l, a, b| {
                for (x, &v) in row.iter().enumerate() {
                    if y != 1 || x != 2 || missing != 0 { l.write(v); }
                    if y != 1 || x != 2 || missing != 1 { a.write(v); }
                    if y != 1 || x != 2 || missing != 2 { b.write(v); }
                }
            }));
            assert!(result.is_err());
        }
    }

    #[test]
    fn scalar_overrun_does_not_advance_cursor() {
        let mut buf = [MaybeUninit::uninit(); 1];
        let mut writer = RowWriter { remaining: &mut buf };
        writer.write(7.0);
        assert!(catch_unwind(AssertUnwindSafe(|| writer.write(9.0))).is_err());
        assert_eq!(writer.remaining.len(), 0);
        writer.assert_complete();
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn simd_overrun_does_not_advance_cursor() {
        if !is_x86_feature_detected!("avx") { return; }
        let mut buf = [MaybeUninit::uninit(); 7];
        let mut writer = RowWriter { remaining: &mut buf };
        // SAFETY: AVX checked above, including when the inner panic is caught.
        assert!(catch_unwind(AssertUnwindSafe(|| unsafe {
            writer.write8(core::arch::x86_64::_mm256_set1_ps(7.0));
        })).is_err());
        assert_eq!(writer.remaining.len(), 7);
        assert!(catch_unwind(AssertUnwindSafe(|| writer.assert_complete())).is_err());
        for _ in 0..7 { writer.write(3.0); }
        writer.assert_complete();
    }
}
