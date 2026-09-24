use dssim_core::{Dssim, RGBAPLU, RGBLU, ToLABBitmap};
use imgref::Img;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn measure<T>(name: &str, mut f: impl FnMut() -> T) {
    let warm = Instant::now();
    while warm.elapsed() < Duration::from_millis(30) { black_box(f()); }
    let mut samples = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed() < Duration::from_millis(60) {
            black_box(f());
            n += 1;
        }
        samples.push(start.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
    samples.sort_by(f64::total_cmp);
    println!("{name},{:.1},{:.1},{:.1}", samples[4], samples[0], samples[8]);
}

fn main() {
    if std::env::args().any(|a| a == "--train") { return train(); }
    println!("case,median_ns,min_ns,max_ns");
    // Deterministic, predecoded linear pixels. Generation is outside every timer.
    for (w, h) in [(256, 256), (2049, 1024)] {
        let mut state = 123456789u32;
        let mut next = || {
            state ^= state << 13; state ^= state >> 17; state ^= state << 5;
            (state >> 8) as f32 / 16777216.0
        };
        let rgb: Vec<_> = (0..w*h).map(|_| RGBLU::new(next(), next(), next())).collect();
        let rgba: Vec<_> = rgb.iter().map(|p| {
            let a = next(); RGBAPLU::new(p.r*a, p.g*a, p.b*a, a)
        }).collect();
        let rgb = Img::new(rgb, w, h);
        let rgba = Img::new(rgba, w, h);
        let attr = Dssim::new();
        measure(&format!("rgb_lab_{w}x{h}"), || black_box(&rgb).to_lab());
        measure(&format!("rgba_lab_{w}x{h}"), || black_box(&rgba).to_lab());
        measure(&format!("rgb_create_{w}x{h}"), || attr.create_image(black_box(&rgb)));
        measure(&format!("rgba_create_{w}x{h}"), || attr.create_image(black_box(&rgba)));
        let original = attr.create_image(&rgb).unwrap();
        let modified = attr.create_image(&rgba).unwrap();
        // Borrowing avoids charging a deep clone to the comparison.
        measure(&format!("compare_{w}x{h}"), || attr.compare(black_box(&original), black_box(&modified)));
        measure(&format!("prepare_pair_compare_{w}x{h}"), || {
            let a = attr.create_image(black_box(&rgb)).unwrap();
            let b = attr.create_image(black_box(&rgba)).unwrap();
            attr.compare(&a, &b)
        });
    }
}

// PGO training uses different dimensions/seeds from the timed validation cases.
fn train() {
    for (w,h) in [(127,93), (512,511), (1600,900)] {
        let mut state = 987654321u32;
        let mut next = || { state ^= state << 13; state ^= state >> 17; state ^= state << 5; (state >> 8) as f32 / 16777216.0 };
        let rgb: Vec<_> = (0..w*h).map(|_| RGBLU::new(next(), next(), next())).collect();
        let rgba: Vec<_> = rgb.iter().enumerate().map(|(i,p)| { let a = match i % 4 { 0 => 0.0, 1 => 1.0, _ => next() }; RGBAPLU::new(p.r*a,p.g*a,p.b*a,a) }).collect();
        let gray: Vec<f32> = rgb.iter().map(|p| p.g).collect();
        let rgb=Img::new(rgb,w,h); let rgba=Img::new(rgba,w,h); let gray=Img::new(gray,w,h);
        for round in 0..3 {
            let mut attr=Dssim::new(); attr.set_save_ssim_maps(if round==0 {5} else {0});
            black_box(rgb.to_lab()); black_box(rgba.to_lab()); black_box(gray.to_lab());
            let a=attr.create_image(&rgb).unwrap(); let b=attr.create_image(&rgba).unwrap(); let g=attr.create_image(&gray).unwrap();
            black_box(attr.compare(&a,&b)); black_box(attr.compare(&g,&g));
        }
    }
}
