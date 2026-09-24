use dssim_core::{Dssim, DssimImage, RGBAPLU, RGBLU};
use imgref::Img;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn measure<T>(mut f: impl FnMut() -> T) -> (f64, f64, f64, String) {
    let warm = Instant::now();
    while warm.elapsed() < Duration::from_millis(50) { black_box(f()); }
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed() < Duration::from_millis(60) {
            black_box(f());
            n += 1;
        }
        samples.push(start.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
    let raw = samples.iter().map(|s| format!("{s:.1}")).collect::<Vec<_>>().join(";");
    samples.sort_by(f64::total_cmp);
    (samples[3], samples[0], samples[6], raw)
}

fn run(
    a: impl Fn(&Dssim, dssim_core::ImageOptions) -> DssimImage<f32>,
    b: impl Fn(&Dssim, dssim_core::ImageOptions) -> DssimImage<f32>,
    mode: &str, scenario: &str,
) {
    let da = Dssim::new();
    let db = Dssim::new();
    let a = |d: &Dssim| a(d, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[0] == b'c'));
    let b = |d: &Dssim| b(d, dssim_core::ImageOptions::default().cache_moments(mode.as_bytes()[1] == b'c'));
    // Check the score outside timing. Drop these images before selecting the
    // measured scenario so one-shot runs don't retain unrelated prepared pairs.
    let score = {
        let x = a(&da);
        let y = b(&db);
        f64::from(da.compare(&x, &y).0).to_bits()
    };
    let result = match scenario {
        "compare" => {
            let x = a(&da);
            let y = b(&db);
            measure(|| da.compare(black_box(&x), black_box(&y)))
        }
        "candidate" => {
            let x = a(&da);
            measure(|| { let y = b(&db); da.compare(black_box(&x), &y) })
        }
        "pair" => measure(|| {
            let x = a(&da);
            let y = b(&db);
            da.compare(&x, &y)
        }),
        _ => panic!("scenario must be compare, candidate, or pair"),
    };
    println!("median_ns,min_ns,max_ns,score_bits,batch_ns");
    println!("{:.1},{:.1},{:.1},{score:016x},{}", result.0, result.1, result.2, result.3);
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let w: usize = args[1].parse().unwrap();
    let h: usize = args[2].parse().unwrap();
    let format = &args[3];
    let mode = &args[4];
    let scenario = &args[5];
    assert!(matches!(mode.as_str(), "cc" | "cs" | "ss"));
    let mut state = 123456789u32;
    let mut next = || {
        state ^= state << 13; state ^= state >> 17; state ^= state << 5;
        (state >> 8) as f32 / 16777216.0
    };
    if format == "linear" {
        let rgb: Vec<_> = (0..w*h).map(|_| RGBLU::new(next(), next(), next())).collect();
        let rgba: Vec<_> = rgb.iter().map(|p| {
            let a = next(); RGBAPLU::new(p.r*a, p.g*a, p.b*a, a)
        }).collect();
        let rgb = Img::new(rgb, w, h);
        let rgba = Img::new(rgba, w, h);
        run(|d, o| d.create_image_with_options(black_box(&rgb), o).unwrap(),
            |d, o| d.create_image_with_options(black_box(&rgba), o).unwrap(), mode, scenario);
    } else {
        assert_eq!(format, "rgba8");
        let src: Vec<_> = (0..w*h).map(|_| rgb::RGBA::new(
            (next()*256.) as u8, (next()*256.) as u8, (next()*256.) as u8, 255,
        )).collect();
        let dst: Vec<_> = src.iter().enumerate().map(|(i,p)| rgb::RGBA::new(
            p.r.wrapping_add(3), p.g, p.b,
            if i%7 == 0 { (next()*256.) as u8 } else { 255 },
        )).collect();
        run(|d, o| d.create_image_rgba_with_options(black_box(&src), w, h, o).unwrap(),
            |d, o| d.create_image_rgba_with_options(black_box(&dst), w, h, o).unwrap(), mode, scenario);
    }
}
