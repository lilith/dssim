# Lab conversion without hand-written SIMD

This experiment uses a worktree separate from the shared dssim directory. The
comparison is upstream main `6e45798`, a Rust 1.90 scalar conversion placed
behind the PR's feature gate, and the exact PR #188 head `d0e6eea`. The control
has no reachable hand-written SIMD code. `rgb_to_lab_auto` uses the same
allocation, row traversal, output stores, and alpha composition as the scalar
fallback. Its small pixel helpers are forced inline within an AVX2/FMA or NEON
target-feature function so LLVM can see and vectorize the arithmetic.

Two control branches were measured. `experiment/tolab-autovec-control` at
`ee65798` uses the PR scalar path's bit-based cube-root seed. This branch at
`7ce472d` changes only that seed to the polynomial used by the hand-written
SIMD path. The polynomial control is the closer arithmetic comparison; the
bit-seed control quantifies the separate effect of that choice. Rust 1.90
assembly confirms packed FMA/divide in the control kernels on both hosts.

Each native release build used opt-level 3, fat LTO, 16 codegen units, panic
abort, the same harness and locked dependencies, and no global CPU flags.
Inputs are deterministic linear RGB and premultiplied RGBA pixels generated
before timing. No codec or I/O is timed. Each case has 30 ms warmup and nine
batches of at least 60 ms; the table shows the median per round. Rounds reverse
variant order. Images are 2049×1024. Linux used a Ryzen 9 7900X with one
worker on CPU 2 or six workers on CPUs 0–5, and glibc allocation reuse
controlled using `MALLOC_MMAP_THRESHOLD_=67108864` and
`MALLOC_TRIM_THRESHOLD_=2147483647`. Mac used an Apple M4 Pro with normal
scheduling. The hosts were not isolated, so close results are inconclusive.

| Host / workers | Operation | Auto polynomial, rounds 1 / 2 | PR intrinsics, rounds 1 / 2 |
|---|---|---:|---:|
| Linux / 1 | RGB → Lab | 2.827 / 2.626 ms | 2.634 / 2.628 ms |
| Linux / 1 | RGBA → Lab | 4.019 / 4.697 ms | 3.621 / 3.608 ms |
| Linux / 1 | Prepare pair + compare | 111.415 / 111.358 ms | 110.729 / 111.231 ms |
| Linux / 6 | RGB → Lab | 0.541 / 0.541 ms | 0.544 / 0.542 ms |
| Linux / 6 | RGBA → Lab | 0.770 / 0.772 ms | 0.735 / 0.733 ms |
| Linux / 6 | Prepare pair + compare | 47.004 / 37.366 ms | 44.751 / 37.595 ms |
| Mac / 1 | RGB → Lab | 3.173 / 3.298 ms | 3.036 / 3.176 ms |
| Mac / 1 | RGBA → Lab | 4.754 / 4.880 ms | 3.552 / 3.671 ms |
| Mac / 1 | Prepare pair + compare | 70.961 / 69.560 ms | 68.228 / 67.619 ms |
| Mac / 6 | RGB → Lab | 0.624 / 0.622 ms | 0.596 / 0.595 ms |
| Mac / 6 | RGBA → Lab | 0.915 / 0.921 ms | 0.689 / 0.687 ms |
| Mac / 6 | Prepare pair + compare | 18.344 / 18.423 ms | 17.740 / 17.655 ms |

The hand-written RGB path has no demonstrated conversion advantage on this
Zen 4 host once the compiler sees the feature-enabled polynomial scalar loop.
Its RGBA path is consistently faster: about 10% in the first Linux single
worker round and about 5% with six workers. The second Linux single-worker
control RGBA measurement was slower, making its exact advantage uncertain.
On M4 Pro, hand-written RGB is about 4–5% faster and hand-written RGBA about
25% faster than the polynomial autovectorized control. Whole-pipeline changes
are small relative to the conversion-specific gains, especially with six
workers; compare-only timings also vary.

The bit-seed control took about 2.79–3.05 ms RGB and 4.12–4.14 ms RGBA on
Linux with one worker, and 3.18–3.30 ms RGB and 5.06–5.24 ms RGBA on Mac.
Thus the polynomial seed explains part of the gap in a compiler-only design.
The large generic-x86 improvement versus upstream main comes chiefly from
running the scalar arithmetic in an AVX2/FMA feature region where LLVM can
vectorize it. The hand-written intrinsics add the narrower gains above.

The core library tests pass on Rust 1.90: 22 on Linux and 21 on Mac. The
benchmark harness is `bench.rs`; the locked standalone manifest is adjacent.
`results/` contains the raw medians and batch minima/maxima for both rounds,
including the bit-seed and exact current-PR comparisons. Build this benchmark
crate with `cargo +1.90.0 build --release --locked --manifest-path
benchmarks/autovec/Cargo.toml`, then run its binary with the worker count and
allocator settings above.
