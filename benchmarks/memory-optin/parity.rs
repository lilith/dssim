use dssim_core::{Dssim, Downsample, RGBAPLU, RGBLU, ToLABBitmap};
use imgref::Img;
use std::io::{self, Write};

fn dump(out: &mut impl Write, values: impl IntoIterator<Item = f32>) {
    for value in values { out.write_all(&value.to_bits().to_le_bytes()).unwrap(); }
}

fn main() {
    let mode = std::env::var("DSSIM_MODE").unwrap_or_else(|_| "cc".into());
    let mut out = io::BufWriter::new(io::stdout().lock());
    for (w, h) in [(1, 1), (2, 3), (7, 9), (8, 8), (15, 17), (16, 15), (17, 19), (31, 33), (64, 49), (257, 65), (2049, 1024)] {
        for padding in [0, 3] {
            let stride = w + padding;
            let mut state = 0x9e3779b9_u32;
            let mut next = || {
                state ^= state << 13; state ^= state >> 17; state ^= state << 5;
                (state >> 8) as f32 / 16_777_216.0
            };
            let rgb: Vec<_> = (0..stride*h).map(|_| RGBLU::new(next(), next(), next())).collect();
            let rgba: Vec<_> = rgb.iter().enumerate().map(|(i, p)| {
                let a = match i % 7 { 0 => 0.0, 1 => 1.0, _ => next() };
                RGBAPLU::new(p.r*a, p.g*a, p.b*a, a)
            }).collect();
            let gray: Vec<_> = rgb.iter().map(|p| p.r).collect();
            let rgb = Img::new_stride(rgb, w, h, stride);
            let rgba = Img::new_stride(rgba, w, h, stride);
            let gray = Img::new_stride(gray, w, h, stride);
            for planes in [rgb.to_lab(), rgba.to_lab(), gray.to_lab()] {
                for plane in planes { dump(&mut out, plane.buf().iter().copied()); }
            }
            if let Some(scaled) = rgb.downsample() {
                dump(&mut out, scaled.buf().iter().flat_map(|p| [p.r, p.g, p.b]));
            }
            if let Some(scaled) = rgba.downsample() {
                dump(&mut out, scaled.buf().iter().flat_map(|p| [p.r, p.g, p.b, p.a]));
            }
            if let Some(scaled) = gray.downsample() { dump(&mut out, scaled.buf().iter().copied()); }
            if w >= 8 && h >= 8 {
                let mut d = Dssim::new();
                d.set_save_ssim_maps(5);

                let a = d.create_image_with_options(&rgb, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[0] == b'c')).unwrap();

                let b = d.create_image_with_options(&rgba, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[1] == b'c')).unwrap();
                let (score, mut maps) = d.compare(&a, &b);
                maps.sort_by_key(|m| std::cmp::Reverse((m.map.width(), m.map.height())));
                for map in maps { dump(&mut out, map.map.buf().iter().copied()); }
                eprintln!("{w}x{h}+{padding}: {:016x}", f64::from(score).to_bits());
                let rgb8: Vec<_>=rgb.pixels().map(|p| rgb::RGB::new((p.r*255.0) as u8,(p.g*255.0) as u8,(p.b*255.0) as u8)).collect();
                let rgba8: Vec<_>=rgba.pixels().map(|p| rgb::RGBA::new((p.r*255.0) as u8,(p.g*255.0) as u8,(p.b*255.0) as u8,(p.a*255.0) as u8)).collect();

                let a = d.create_image_rgb_with_options(&rgb8,w,h, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[0] == b'c')).unwrap();

                let b = d.create_image_rgba_with_options(&rgba8,w,h, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[1] == b'c')).unwrap();
                let (score,maps)=d.compare(&a,&b);
                for map in maps { dump(&mut out,map.map.buf().iter().copied()); }
                eprintln!("integer {w}x{h}+{padding}: {:016x}",f64::from(score).to_bits());
                let other_gray=Img::new(gray.pixels().map(|p|p*0.9).collect::<Vec<_>>(),w,h);

                let a = d.create_image_with_options(&gray, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[0] == b'c')).unwrap();
                let b = d.create_image_with_options(&other_gray, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[1] == b'c')).unwrap();
                let (score,maps)=d.compare(&a,&b);
                for map in maps { dump(&mut out,map.map.buf().iter().copied()); }
                eprintln!("gray {w}x{h}+{padding}: {:016x}",f64::from(score).to_bits());
            }
        }
    }
}
