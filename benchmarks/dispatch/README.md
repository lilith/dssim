Codec-free dispatch benchmarks and output checks for the runtime-autovectorization PR.

Current source anchors: upstream kornelski/main 0e44c9b7fe91a5265c8e463436c28512186fe9cd; dispatch eb41ae3. Upstream merged the no-threads fix (#196) and deterministic scale ordering (#195) during this work, so #197 was rebased to one performance commit directly on that new main. Fresh benchmark and parity runs use these updated sources. The production PR contains kernel changes and tests only; this fork branch holds the reproduction harness and raw measurements.

Historical runs below are explicitly labeled: upstream 6e4579882f97907a75fd84133a4c4993a06d56ea versus dispatch b6ebed5 (before the rebase).

All runs use Rust 1.90, opt-level 3, fat LTO, 16 codegen units, panic=abort, and the same benchmark lockfile. Linux: Ryzen 9 7900X; one worker pinned to CPU 2, six workers to CPUs 0–5. Linux sets MALLOC_MMAP_THRESHOLD_=67108864 and MALLOC_TRIM_THRESHOLD_=2147483647 for every variant to reduce allocator threshold noise. macOS: Apple M4 Pro, normal OS scheduling and default allocator. Machines are not isolated; turbo is enabled.

bench.rs generates deterministic linear RGB and premultiplied RGBA BEFORE timers. There are no codecs, file I/O or gamma conversion inside timed operations. Lab/preparation includes allocations and destruction. Comparison borrows prepared images; the full pipeline prepares both and compares. Each operation warms for 30 ms and reports the median of nine batches lasting at least 60 ms. Two rounds reverse variant order. These are workload measurements, not statistical confidence intervals. Do not sum separately measured phases to infer whole-pipeline time: allocation reuse differs.

Reproduction: copy this entire standalone benchmark directory into each source worktree, preserving its relative dependency, then:

```sh
cargo +1.90.0 build --release --locked --manifest-path benchmarks/dispatch/Cargo.toml --bins
RAYON_NUM_THREADS=1 benchmarks/dispatch/target/release/review-bench
RAYON_NUM_THREADS=6 benchmarks/dispatch/target/release/review-bench
```

On Linux apply the allocator variables and taskset affinity above. For native controls build in a separate target directory with RUSTFLAGS='-C target-cpu=native'. Compare the exact upstream commit and dispatch commit, not an earlier fork baseline. The workspace's old lockfile is irrelevant to this isolated harness; its checked-in lock fixes all benchmark dependencies.

parity.rs emits every f32 bit pattern, in order, for RGB/RGBA/grayscale Lab, downsampled planes and saved SSIM maps. It covers 11 sizes (1x1 through 2049x1024), both packed and stride+3 layouts, vector tails, and alpha endpoints/intermediate values. Maps are sorted by dimensions to permit comparisons with historical upstream 6e45798, whose parallel scale collection was unordered; current upstream has fixed that ordering. Scores are emitted separately on stderr. Run with one worker and compare outputs byte-for-byte:

```sh
RAYON_NUM_THREADS=1 benchmarks/dispatch/target/release/parity > pixels.bits 2> scores.txt
```

Linux and Mac upstream/dispatch outputs matched: 175,199,056 bytes, SHA256 b69c0b14b6ff25441a5a89c35461e93dcbaad52becd64fc267a9a0d89883ceb0. One-worker score bit patterns matched too. This is empirical coverage, not a promise about every f32 input or future compiler. The separate standard RGBA probe (`score`, Linux RSS reporting) retained the locked displayed score 0.338437 at 2049x1024.

Validation: 25 release library tests with and without default features on Linux x86-64 and macOS AArch64; tests call baseline and each supported feature clone directly, compare bits, and include padded grayscale. Rust 1.90 packaged-crate verification passed. Existing upstream unused-shim-trait warnings remain when threads are disabled.

Generated-code checks found vector loops in both RGB and RGBA feature clones, without callback calls or software fmaf calls. Globally enabling AVX2/FMA removes that detection/cache; enabling all requested AVX-512 features removes both caches. An AVX2-only build intentionally retains the wider-tier probe. Full-native benchmark binaries also had no dispatch caches or RGB baseline/AVX2 clones remaining.

LabConv was unnecessary: existing ToRGB handles RGBA and now has a trivial identity implementation for RGB. Inline-always ToRGB/ToLAB implementations feed one inline row body. Function-item callbacks were tested but one Fn::call remained out of line. A single generic closure dispatcher was also tested; despite explicit inline attributes it inhibited vectorization in SSIM/downsample. Explicit typed wrappers retain LLVM's useful argument information. Baseline blur/downsample wrappers may inline on non-x86; forcing those boundaries on ARM regressed preparation. No new row-writer, hand intrinsics, reassociated reductions, cube-root arithmetic changes, fused conversion, or image-plane storage changes are included.

The controlled AVX2-only experiment disables the AVX-512 gate in a separate worktree. Its x86 kernel source otherwise matches the final implementation (the later baseline-inline refinement applies only to non-x86). At one worker, RGB Lab was 2.613–2.624 ms and RGBA 3.802–3.814 ms versus AVX-512's 2.143–2.146 and 2.604–2.607 ms. Whole pipeline was 104.612–104.843 versus 103.317–103.448 ms: the wider tier helps these kernels, but the overall benefit is modest on this workload. Corresponding controlled CSVs are included separately from final publication runs.

Current-main measurements (`0e44c9b` versus `eb41ae3`), 2049×1024. Values are milliseconds, shown as the range of two rounds’ batch medians. Raw files are `results/rebased-*.csv` and `results/mac-rebased-*.csv`.

Linux, 1 worker(s):

| Operation | Main generic | Dispatch generic | Main native | Dispatch native |
|---|---:|---:|---:|---:|
| rgb_lab | 89.02–114.83 | 2.14–2.14 | 2.19–2.19 | 2.15–2.15 |
| rgba_lab | 92.09–113.79 | 2.60–2.60 | 2.66–2.67 | 2.60–2.61 |
| rgb_create | 142.42–142.53 | 28.84–28.97 | 25.63–28.00 | 28.47–28.67 |
| rgba_create | 143.49–146.02 | 30.55–30.86 | 26.98–29.56 | 30.13–30.30 |
| compare | 31.85–31.95 | 12.65–12.66 | 21.00–21.92 | 12.32–13.03 |
| prepare_pair_compare | 343.45–344.46 | 103.77–103.93 | 106.49–111.97 | 103.71–103.79 |

Linux, 6 worker(s):

| Operation | Main generic | Dispatch generic | Main native | Dispatch native |
|---|---:|---:|---:|---:|
| rgb_lab | 15.32–15.59 | 0.51–0.52 | 0.51–0.52 | 0.52–0.52 |
| rgba_lab | 15.60–15.61 | 0.70–0.70 | 0.70–0.70 | 0.70–0.70 |
| rgb_create | 28.81–28.94 | 9.21–9.34 | 9.06–9.09 | 9.26–9.27 |
| rgba_create | 28.18–28.71 | 9.74–9.79 | 9.47–9.49 | 9.72–9.75 |
| compare | 10.39–11.28 | 8.63–10.59 | 8.83–9.52 | 8.70–9.69 |
| prepare_pair_compare | 83.76–84.85 | 42.15–42.37 | 43.75–45.66 | 40.80–41.26 |

Mac, 1 worker(s):

| Operation | Main | Dispatch |
|---|---:|---:|
| rgb_lab | 3.09–3.13 | 2.78–2.79 |
| rgba_lab | 4.59–4.62 | 3.64–3.70 |
| rgb_create | 14.57–15.06 | 13.85–14.42 |
| rgba_create | 16.83–16.84 | 15.20–15.77 |
| compare | 37.78–38.45 | 11.15–12.02 |
| prepare_pair_compare | 65.62–67.45 | 38.19–38.34 |

Mac, 6 worker(s):

| Operation | Main | Dispatch |
|---|---:|---:|
| rgb_lab | 0.62–0.62 | 0.56–0.56 |
| rgba_lab | 0.91–0.92 | 0.74–0.74 |
| rgb_create | 3.95–3.99 | 3.86–3.92 |
| rgba_create | 5.00–5.14 | 4.18–4.22 |
| compare | 9.44–9.59 | 4.84–4.99 |
| prepare_pair_compare | 18.14–18.79 | 13.01–13.07 |

Historical pre-rebase measurements follow below; values are milliseconds, shown as the range of the two rounds' medians.

Linux, 1 worker(s):

| Operation | Main generic | Dispatch generic | Main native | Dispatch native |
|---|---:|---:|---:|---:|
| rgb_lab | 118.84–119.78 | 2.14–2.14 | 2.19–2.19 | 2.15–2.15 |
| rgba_lab | 115.27–119.91 | 2.60–2.60 | 2.66–2.67 | 2.59–2.60 |
| rgb_create | 176.63–177.11 | 27.02–29.04 | 28.43–28.84 | 27.38–27.52 |
| rgba_create | 177.97–179.15 | 28.44–30.82 | 30.13–30.41 | 28.93–29.14 |
| compare | 31.66–31.77 | 12.31–12.31 | 21.84–21.87 | 12.00–12.44 |
| prepare_pair_compare | 415.48–416.15 | 98.87–103.71 | 111.41–111.90 | 102.69–103.07 |

Linux, 6 worker(s):

| Operation | Main generic | Dispatch generic | Main native | Dispatch native |
|---|---:|---:|---:|---:|
| rgb_lab | 15.69–15.81 | 0.52–0.52 | 0.51–0.52 | 0.51–0.52 |
| rgba_lab | 15.25–15.70 | 0.70–0.71 | 0.70–0.70 | 0.70–0.71 |
| rgb_create | 29.33–29.44 | 9.18–9.26 | 9.05–9.08 | 9.12–9.16 |
| rgba_create | 28.47–28.65 | 9.74–9.78 | 9.45–9.52 | 9.69–9.74 |
| compare | 10.61–10.81 | 9.30–10.45 | 8.65–8.90 | 8.54–10.26 |
| prepare_pair_compare | 79.20–81.78 | 33.75–45.00 | 33.19–39.97 | 33.31–46.91 |

Mac, 1 worker(s):

| Operation | Main | Dispatch |
|---|---:|---:|
| rgb_lab | 3.09–3.33 | 2.78–2.98 |
| rgba_lab | 4.59–4.90 | 3.65–3.93 |
| rgb_create | 15.06–15.86 | 14.29–15.06 |
| rgba_create | 16.44–17.86 | 15.57–16.40 |
| compare | 37.88–39.84 | 10.94–11.91 |
| prepare_pair_compare | 65.60–70.90 | 39.37–40.21 |

Mac, 6 worker(s):

| Operation | Main | Dispatch |
|---|---:|---:|
| rgb_lab | 0.62–0.62 | 0.55–0.56 |
| rgba_lab | 0.91–0.92 | 0.73–0.73 |
| rgb_create | 4.04–4.09 | 3.87–3.91 |
| rgba_create | 4.96–5.05 | 4.19–4.22 |
| compare | 9.48–9.77 | 5.39–5.39 |
| prepare_pair_compare | 18.14–18.69 | 12.70–12.87 |

The earlier controlled Linux series measured the same main pipeline at 339–345 ms (one worker), versus 103–104 ms for dispatch. The final series was 415–416 ms versus 99–104 ms. Both sets are retained; this host variability rules out treating a single speedup ratio as a guarantee. Six-worker and native comparisons are especially noisy.

[Follow-up Mac native controls and SSIM ablation](mac-native-check.md) compare both revisions with and without `-C target-cpu=native` and identify the source of the ARM improvement.
