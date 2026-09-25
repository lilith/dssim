use dssim_core::{Dssim, DssimImage};
use rgb::RGBA;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn prepare(d: &Dssim, pixels: &[RGBA<u8>], w: usize, h: usize, cached: bool) -> DssimImage<f32> {
    #[cfg(feature = "options")]
    { d.create_image_rgba_with_options(pixels, w, h,
        dssim_core::ImageOptions::default().cache_for_reuse(cached)).unwrap() }
    #[cfg(not(feature = "options"))]
    { assert!(cached, "historical versions always cache"); d.create_image_rgba(pixels, w, h).unwrap() }
}

// One operation, with preparation, comparison, and destruction measured together.
// Four rotating candidates represent comparing a reference with different images.
#[derive(Clone, Copy, Default)]
struct Sample { total: f64, reference: f64, candidate: f64, compare: f64, drop: f64 }
fn ns(t: Duration) -> f64 { t.as_secs_f64() * 1e9 }
fn measure(mut f: impl FnMut(usize) -> Sample) -> (Sample, Vec<Sample>) {
    let mut index = 0;
    let warm = Instant::now();
    while warm.elapsed() < Duration::from_millis(100) { black_box(f(index % 4)); index += 1; }
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        let mut n = 0;
        let mut sum = Sample::default();
        // Complete candidate cycles so every batch has an equal input mix.
        while n == 0 || n % 4 != 0 || start.elapsed() < Duration::from_millis(100) {
            let s = black_box(f(index % 4)); index += 1;
            sum.reference += s.reference; sum.candidate += s.candidate;
            sum.compare += s.compare; sum.drop += s.drop;
            n += 1;
        }
        sum.total = ns(start.elapsed()) / n as f64;
        sum.reference /= n as f64; sum.candidate /= n as f64;
        sum.compare /= n as f64; sum.drop /= n as f64;
        samples.push(sum);
    }
    let mut sorted = samples.clone();
    sorted.sort_by(|a,b| a.total.total_cmp(&b.total));
    (sorted[3], samples)
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let w: usize = args[1].parse().unwrap();
    let h: usize = args[2].parse().unwrap();
    let mode = args[3].as_str();
    let scenario = args[4].as_str();
    assert!(matches!(mode, "cc" | "cs" | "ss"));
    let mut state = 123456789u32;
    let mut next = || {
        state ^= state << 13; state ^= state >> 17; state ^= state << 5;
        (state >> 8) as f32 / 16777216.0
    };
    let src: Vec<_> = (0..w*h).map(|_| RGBA::new(
        (next()*256.) as u8, (next()*256.) as u8, (next()*256.) as u8, 255,
    )).collect();
    let candidates: Vec<Vec<_>> = (0..4).map(|k| src.iter().enumerate().map(|(i,p)| RGBA::new(
        p.r.wrapping_add(3 + 4*k), p.g, p.b,
        if i%7 == 0 { (next()*256.) as u8 } else { 255 },
    )).collect()).collect();
    let d = Dssim::new();
    let a = || prepare(&d, black_box(&src), w, h, mode.as_bytes()[0] == b'c');
    let b = |i: usize| prepare(&d, black_box(&candidates[i]), w, h, mode.as_bytes()[1] == b'c');
    // Verify all four scores and initialize Rayon outside timing, then drop images.
    let scores = {
        let x = a();
        (0..4).map(|i| { let y = b(i); format!("{:016x}", f64::from(d.compare(&x, &y).0).to_bits()) })
            .collect::<Vec<_>>().join(";")
    };
    let mut reference_once = 0.;
    let (median, batches) = match scenario {
        "pair" => measure(|i| {
            let start = Instant::now(); let x = a(); let reference_done = Instant::now();
            let y = b(i); let candidate_done = Instant::now();
            let result = black_box(d.compare(&x, &y)); let compare_done = Instant::now();
            drop((result, x, y)); let drop_done = Instant::now();
            Sample { total: 0., reference: ns(reference_done-start), candidate: ns(candidate_done-reference_done),
                compare: ns(compare_done-candidate_done), drop: ns(drop_done-compare_done) }
        }),
        "candidate" => {
            let start = Instant::now(); let x = a(); reference_once = ns(start.elapsed());
            measure(|i| {
                let start = Instant::now(); let y = b(i); let candidate_done = Instant::now();
                let result = black_box(d.compare(black_box(&x), &y)); let compare_done = Instant::now();
                drop((result, y)); let drop_done = Instant::now();
                Sample { total: 0., reference: 0., candidate: ns(candidate_done-start),
                    compare: ns(compare_done-candidate_done), drop: ns(drop_done-compare_done) }
            })
        }
        _ => panic!("scenario must be pair or candidate"),
    };
    // Each phase comes from the same batch as median total. All batches are retained.
    println!("median_ns,reference_prep_ns,candidate_prep_ns,compare_ns,drop_ns,reference_once_ns,score_bits,batch_ns,batch_reference_ns,batch_candidate_ns,batch_compare_ns,batch_drop_ns");
    let list = |f: fn(&Sample)->f64| batches.iter().map(|s| format!("{:.1}",f(s))).collect::<Vec<_>>().join(";");
    println!("{:.1},{:.1},{:.1},{:.1},{:.1},{reference_once:.1},{scores},{},{},{},{},{}",
        median.total,median.reference,median.candidate,median.compare,median.drop,
        list(|s|s.total),list(|s|s.reference),list(|s|s.candidate),list(|s|s.compare),list(|s|s.drop));
}
