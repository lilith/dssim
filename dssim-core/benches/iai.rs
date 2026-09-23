//! iai-callgrind instruction-count benchmarks for the compare pipeline.

use dssim_core::{Dssim, DssimImage, RGBLU, Val};
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use imgref::ImgVec;
use std::hint::black_box;

const W: usize = 2049;
const H: usize = 1024;

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn gen_image(seed: u64) -> ImgVec<RGBLU> {
    let mut s = seed;
    let px: Vec<RGBLU> = (0..W * H)
        .map(|i| {
            let x = (i % W) as f32;
            let y = (i / W) as f32;
            let n = (xorshift64(&mut s) >> 40) as f32 * (1.0 / 16_777_216.0);
            let base = (x * 0.013).sin() * (y * 0.017).cos() * 0.5 + 0.5;
            RGBLU::new(
                (base * 0.6 + n * 0.4).min(1.0),
                (base * 0.4 + n * 0.5).min(1.0),
                (base * 0.3 + n * 0.6).min(1.0),
            )
        })
        .collect();
    ImgVec::new(px, W, H)
}

#[library_benchmark]
fn sanity() -> u64 {
    let mut acc = 0u64;
    for i in 0..1_000_000u64 {
        acc = black_box(acc.wrapping_mul(6364136223846793005).wrapping_add(i));
    }
    acc
}

#[library_benchmark]
fn bench_create() -> Option<DssimImage<f32>> {
    let img = gen_image(0x9e37_79b9);
    let d = Dssim::new();
    black_box(d.create_image(black_box(&img)))
}

#[library_benchmark]
fn bench_compare() -> Val {
    let d = Dssim::new();
    let a = d.create_image(&gen_image(0x9e37_79b9)).unwrap();
    let b = d.create_image(&gen_image(0xdead_beef)).unwrap();
    black_box(d.compare(black_box(&a), black_box(&b)).0)
}

library_benchmark_group!(
    name = pipeline;
    benchmarks = sanity, bench_create, bench_compare
);

main!(library_benchmark_groups = pipeline);
