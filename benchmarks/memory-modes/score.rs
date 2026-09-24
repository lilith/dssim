use dssim_core::Dssim;
use rgb::RGBA;
use std::hint::black_box;
use std::time::Instant;

fn peak_rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status").unwrap()
        .lines().find(|l| l.starts_with("VmHWM")).unwrap()
        .split_whitespace().nth(1).unwrap().parse::<f64>().unwrap() / 1024.0
}

fn gen_px(w: usize, h: usize, seed: u32) -> Vec<RGBA<u8>> {
    let mut state = seed;
    (0..w * h).map(|_| {
        state ^= state << 13; state ^= state >> 17; state ^= state << 5;
        let v = state;
        RGBA::new(v as u8, (v >> 8) as u8, (v >> 16) as u8, 255)
    }).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let w: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(2049);
    let h: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(1024);
    let reps: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(3);
    let px1 = gen_px(w, h, 123456789);
    let px2 = gen_px(w, h, 987654321);
    let attr = Dssim::new();
    let (mut bc, mut bm, mut ssim) = (f64::MAX, f64::MAX, 0.0);
    for _ in 0..reps {
        let t0 = Instant::now();
        let img1 = attr.create_image_rgba(black_box(&px1), w, h).unwrap();
        let img2 = attr.create_image_rgba(black_box(&px2), w, h).unwrap();
        let tc = t0.elapsed().as_secs_f64() * 1e3;
        let t1 = Instant::now();
        let (score, _) = attr.compare(black_box(&img1), black_box(&img2));
        let tm = t1.elapsed().as_secs_f64() * 1e3;
        ssim = f64::from(score);
        bc = bc.min(tc);
        bm = bm.min(tm);
    }
    println!(
        "{w}x{h} t={}: create {bc:.2} ms, compare {bm:.2} ms, peak {:.0} MB, ssim={ssim:.6}",
        std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "all".into()),
        peak_rss_mb()
    );
}
