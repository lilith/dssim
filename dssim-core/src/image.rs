#![allow(dead_code)]

use crate::linear::{GammaComponent, GammaPixel};
use imgref::*;
use rgb::alt::*;
use rgb::*;

/// RGBA, but: premultiplied alpha, linear (using sRGB primaries, but not its gamma curve), f32 unit scale 0..1
pub type RGBAPLU = RGBA<f32>;
/// RGB, but: linear (using sRGB primaries, but not its gamma curve), f32 unit scale 0..1
pub type RGBLU = RGB<f32>;

/// L\*a\*b\*b, but using float units (values are 100× smaller than in usual integer representation)
#[derive(Debug, Copy, Clone)]
pub struct LAB {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

impl std::ops::Mul<Self> for LAB {
    type Output = Self;

    fn mul(self, other: Self) -> Self::Output {
        Self {
            l: self.l * other.l,
            a: self.a * other.a,
            b: self.b * other.b,
        }
    }
}

impl std::ops::Mul<LAB> for f32 {
    type Output = LAB;

    fn mul(self, other: LAB) -> Self::Output {
        LAB {
            l: self * other.l,
            a: self * other.a,
            b: self * other.b,
        }
    }
}

impl std::ops::Mul<f32> for LAB {
    type Output = Self;

    fn mul(self, other: f32) -> Self::Output {
        Self {
            l: self.l * other,
            a: self.a * other,
            b: self.b * other,
        }
    }
}

impl std::ops::Add<Self> for LAB {
    type Output = Self;

    fn add(self, other: Self::Output) -> Self::Output {
        Self {
            l: self.l + other.l,
            a: self.a + other.a,
            b: self.b + other.b,
        }
    }
}

impl std::ops::Add<f32> for LAB {
    type Output = Self;

    fn add(self, other: f32) -> Self::Output {
        Self {
            l: self.l + other,
            a: self.a + other,
            b: self.b + other,
        }
    }
}

impl std::ops::Sub<Self> for LAB {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self {
            l: self.l - other.l,
            a: self.a - other.a,
            b: self.b - other.b,
        }
    }
}

impl LAB {
    pub(crate) fn avg(self) -> f32 {
        (self.l + self.a + self.b) * (1. / 3.)
    }
}

impl From<LAB> for f64 {
    fn from(other: LAB) -> Self {
        (Self::from(other.l) + Self::from(other.a) + Self::from(other.b)) * (1. / 3.)
    }
}

impl From<LAB> for f32 {
    fn from(other: LAB) -> Self {
        other.avg()
    }
}

impl std::ops::Div<Self> for LAB {
    type Output = Self;

    fn div(self, other: Self::Output) -> Self::Output {
        Self {
            l: self.l / other.l,
            a: self.a / other.a,
            b: self.b / other.b,
        }
    }
}

/// Component-wise averaging of pixel values used by `Downsample` to support arbitrary pixel types
///
/// Used to naively resample 4 high-res pixels into one low-res pixel
#[doc(hidden)]
pub trait Average4 {
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self;
}

impl Average4 for f32 {
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self {
        (a + b + c + d) * 0.25
    }
}

impl Average4 for RGBAPLU {
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBAPLU {
            r: Average4::average4(a.r, b.r, c.r, d.r),
            g: Average4::average4(a.g, b.g, c.g, d.g),
            b: Average4::average4(a.b, b.b, c.b, d.b),
            a: Average4::average4(a.a, b.a, c.a, d.a),
        }
    }
}

impl Average4 for RGBLU {
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBLU {
            r: Average4::average4(a.r, b.r, c.r, d.r),
            g: Average4::average4(a.g, b.g, c.g, d.g),
            b: Average4::average4(a.b, b.b, c.b, d.b),
        }
    }
}

pub(crate) trait ToRGB {
    fn to_rgb(self, n: usize) -> RGBLU;
}

impl ToRGB for RGBAPLU {
    #[inline(always)]
    fn to_rgb(self, n: usize) -> RGBLU {
        // Bit tests only read bits <32; u32 keeps vectorized compares in
        // 32-bit lanes instead of usize-wide ones.
        let n = n as u32;
        let mut r = self.r;
        let mut g = self.g;
        let mut b = self.b;
        let a = self.a;
        let dither = if a < 255.0 { 1.0 - a } else { 0.0 }; // assumes premultiplied alpha
        if (n & 16) != 0 {
            r += dither;
        }
        if (n & 8) != 0 {
            g += dither;
        }
        if (n & 32) != 0 {
            b += dither;
        }

        RGBLU { r, g, b }
    }
}

/// You can customize how images are downsampled
///
/// Multi-scale DSSIM needs to scale images down. This is it. It's supposed to return the same type of image, but half the size.
///
/// There is a default implementation that just averages 4 neighboring pixels.
#[doc(hidden)]
pub trait Downsample {
    type Output;
    fn downsample(&self) -> Option<Self::Output>;
}

/// 2x2→1 average of a source row-pair:
/// `out[i] = T::average4(top[2i], top[2i+1], bot[2i], bot[2i+1])`.
/// `out.len()` is the output (half) width.
#[inline(always)]
fn downsample_row_pair_inline<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [std::mem::MaybeUninit<T>]) {
    for (i, o) in out.iter_mut().enumerate() {
        o.write(T::average4(top[2 * i], top[2 * i + 1], bot[2 * i], bot[2 * i + 1]));
    }
}

#[inline(never)]
fn downsample_row_pair_base<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [std::mem::MaybeUninit<T>]) {
    downsample_row_pair_inline(top, bot, out)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn downsample_row_pair_avx2<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [std::mem::MaybeUninit<T>]) {
    downsample_row_pair_inline(top, bot, out)
}

fn downsample_row_pair<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [std::mem::MaybeUninit<T>]) {
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
        return unsafe { downsample_row_pair_avx2(top, bot, out) };
    }
    downsample_row_pair_base(top, bot, out)
}

/// Same row-pair shape, but each source component is linearized through
/// `lut` before averaging (gamma-encoded integer pixels → `RGBAPLU`).
#[inline(always)]
fn downsample_gamma_row_pair_inline<P>(top: &[P], bot: &[P], lut: &<P::Component as GammaComponent>::Lut, out: &mut [std::mem::MaybeUninit<RGBAPLU>])
where
    P: GammaPixel<Output = RGBAPLU> + Copy,
{
    for (i, o) in out.iter_mut().enumerate() {
        o.write(Average4::average4(
            top[2 * i].to_linear(lut),
            top[2 * i + 1].to_linear(lut),
            bot[2 * i].to_linear(lut),
            bot[2 * i + 1].to_linear(lut),
        ));
    }
}

#[inline(never)]
fn downsample_gamma_row_pair_base<P>(top: &[P], bot: &[P], lut: &<P::Component as GammaComponent>::Lut, out: &mut [std::mem::MaybeUninit<RGBAPLU>])
where
    P: GammaPixel<Output = RGBAPLU> + Copy,
{
    downsample_gamma_row_pair_inline(top, bot, lut, out)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn downsample_gamma_row_pair_avx2<P>(top: &[P], bot: &[P], lut: &<P::Component as GammaComponent>::Lut, out: &mut [std::mem::MaybeUninit<RGBAPLU>])
where
    P: GammaPixel<Output = RGBAPLU> + Copy,
{
    downsample_gamma_row_pair_inline(top, bot, lut, out)
}

fn downsample_gamma_row_pair<P>(top: &[P], bot: &[P], lut: &<P::Component as GammaComponent>::Lut, out: &mut [std::mem::MaybeUninit<RGBAPLU>])
where
    P: GammaPixel<Output = RGBAPLU> + Copy,
{
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
        return unsafe { downsample_gamma_row_pair_avx2(top, bot, lut, out) };
    }
    downsample_gamma_row_pair_base(top, bot, lut, out)
}

impl<T> Downsample for ImgVec<T> where T: Average4 + Copy + Sync + Send {
    type Output = Self;

    fn downsample(&self) -> Option<Self::Output> {
        self.as_ref().downsample()
    }
}

impl<T> Downsample for ImgRef<'_, T> where T: Average4 + Copy + Sync + Send {
    type Output = ImgVec<T>;

    fn downsample(&self) -> Option<Self::Output> {
        let stride = self.stride();
        let width = self.width();
        let height = self.height();

        if width < 8 || height < 8 {
            return None;
        }

        let half_height = height / 2;
        let half_width = width / 2;

        let mut scaled: Vec<T> = Vec::with_capacity(half_width * half_height);
        for y in 0..half_height {
            let row = y * 2 * stride;
            let top = &self.buf()[row..row + half_width * 2];
            let bot = &self.buf()[row + stride..row + stride + half_width * 2];
            downsample_row_pair(top, bot, &mut scaled.spare_capacity_mut()[y * half_width..][..half_width]);
        }
        // SAFETY: every row pair wrote all half_width slots of its output row.
        unsafe { scaled.set_len(half_width * half_height) };
        Some(Img::new(scaled, half_width, half_height))
    }
}

/// Downsampling for gamma-encoded integer pixels (`RGBA<u8>`, `RGB<u16>`,
/// `Gray`, `GrayAlpha`, …): linearizes each pixel and averages in linear
/// space — equivalent to `to_rgbaplu()` + `downsample()` without
/// materializing the intermediate buffer.
///
/// Written per concrete type (a blanket impl would overlap the `Average4`
/// impls above, since nothing rules out a type satisfying both bounds).
macro_rules! downsample_gamma_pixel {
    ($($t:ty),+ $(,)?) => {$(
        impl Downsample for ImgVec<$t> {
            type Output = ImgVec<RGBAPLU>;

            fn downsample(&self) -> Option<Self::Output> {
                self.as_ref().downsample()
            }
        }

        impl Downsample for ImgRef<'_, $t> {
            type Output = ImgVec<RGBAPLU>;

            fn downsample(&self) -> Option<Self::Output> {
                let stride = self.stride();
                let width = self.width();
                let height = self.height();

                if width < 8 || height < 8 {
                    return None;
                }

                let half_height = height / 2;
                let half_width = width / 2;
                let lut = <$t>::make_lut();

                let mut scaled: Vec<RGBAPLU> = Vec::with_capacity(half_width * half_height);
                for y in 0..half_height {
                    let row = y * 2 * stride;
                    let top = &self.buf()[row..row + half_width * 2];
                    let bot = &self.buf()[row + stride..row + stride + half_width * 2];
                    downsample_gamma_row_pair(top, bot, &lut, &mut scaled.spare_capacity_mut()[y * half_width..][..half_width]);
                }
                // SAFETY: every row pair wrote all half_width slots of its output row.
                unsafe { scaled.set_len(half_width * half_height) };
                Some(Img::new(scaled, half_width, half_height))
            }
        }
    )+}
}

downsample_gamma_pixel! {
    RGBA<u8>, RGBA<u16>,
    RGB<u8>, RGB<u16>,
    BGRA<u8>, BGRA<u16>,
    BGR<u8>, BGR<u16>,
    Gray<u8>, Gray<u16>,
    GrayAlpha<u8>, GrayAlpha<u16>,
}

#[allow(dead_code)]
pub(crate) fn worst(input: ImgRef<'_, f32>) -> ImgVec<f32> {
    let stride = input.stride();
    let half_height = input.height() / 2;
    let half_width = input.width() / 2;

    if half_height < 4 || half_width < 4 {
        return input.new_buf(input.buf().to_vec());
    }

    let mut scaled = Vec::with_capacity(half_width * half_height);
    scaled.extend(input.buf().chunks(stride * 2).take(half_height).flat_map(|pair| {
        let (top, bot) = pair.split_at(stride);
        let top = &top[0..half_width * 2];
        let bot = &bot[0..half_width * 2];

        top.as_chunks::<2>().0.iter().zip(bot.chunks_exact(2)).map(|(a,b)| {
            a[0].min(a[1]).min(b[0].min(b[1]))
        })
    }));

    assert_eq!(half_width * half_height, scaled.len());
    Img::new(scaled, half_width, half_height)
}

#[allow(dead_code)]
pub(crate) fn avgworst(input: ImgRef<'_, f32>) -> ImgVec<f32> {
    let stride = input.stride();
    let half_height = input.height() / 2;
    let half_width = input.width() / 2;

    if half_height < 4 || half_width < 4 {
        return input.new_buf(input.buf().to_vec());
    }

    let mut scaled = Vec::with_capacity(half_width * half_height);
    scaled.extend(input.buf().chunks(stride * 2).take(half_height).flat_map(|pair| {
        let (top, bot) = pair.split_at(stride);
        let top = &top[0..half_width * 2];
        let bot = &bot[0..half_width * 2];

        top.as_chunks::<2>().0.iter()
            .zip(bot.chunks_exact(2))
            .map(|(a, b)| (a[0] + a[1] + b[0] + b[1]).mul_add(0.25, a[0].min(a[1]).min(b[0].min(b[1]))) * 0.5)
    }));

    assert_eq!(half_width * half_height, scaled.len());
    Img::new(scaled, half_width, half_height)
}

#[allow(dead_code)]
pub(crate) fn avg(input: ImgRef<'_, f32>) -> ImgVec<f32> {
    let stride = input.stride();
    let half_height = input.height() / 2;
    let half_width = input.width() / 2;

    if half_height < 4 || half_width < 4 {
        return input.new_buf(input.buf().to_vec());
    }

    let mut scaled = Vec::with_capacity(half_width * half_height);
    scaled.extend(input.buf().chunks(stride * 2).take(half_height).flat_map(|pair| {
        let (top, bot) = pair.split_at(stride);
        let top = &top[0..half_width * 2];
        let bot = &bot[0..half_width * 2];

        top.as_chunks::<2>().0.iter().zip(bot.chunks_exact(2)).map(|(a,b)| {
            (a[0] + a[1] + b[0] + b[1]) * 0.25
        })
    }));

    assert_eq!(half_width * half_height, scaled.len());
    Img::new(scaled, half_width, half_height)
}

/// `ImgRef<RGBA<u8>>::downsample()` must equal `to_rgbaplu()` followed by
/// `Average4` downsampling on the materialized buffer — the two compute the
/// same values, so they must be bit-identical (not merely close).
#[test]
fn fused_downsample_parity() {
    use crate::linear::ToRGBAPLU;

    let (w, h) = (66, 34); // odd halves exercise the odd-row tail
    let px: Vec<RGBA<u8>> = (0..w * h).map(|i| {
        let v = (i as u32).wrapping_mul(747_796_405).rotate_left(9);
        RGBA::new(v as u8, (v >> 8) as u8, (v >> 16) as u8, (v >> 24) as u8)
    }).collect();

    let fused = ImgRef::new(&px[..], w, h).downsample().unwrap();
    let reference = Img::new(px.to_rgbaplu(), w, h).downsample().unwrap();
    assert_eq!(fused.buf(), reference.buf());
}
