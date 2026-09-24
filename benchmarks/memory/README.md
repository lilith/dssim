# Memory reductions above PR #197

Two independently reviewable commits above the exact dispatch-only PR head. Results collected 2026-09-23. The current revisions use a private adapter and do not add public integer-image trait implementations. Historical timing tables below remain explicitly tied to the earlier revisions; parity, tests, compatibility and one-worker allocation measurements were rerun on the private-adapter revisions.

| Version | Commit | Scope |
| --- | --- | --- |
| base | eb41ae307bfda358e41c022df2e2956fe5fd868c | PR #197; parent is upstream main 0e44c9b7fe91a5265c8e463436c28512186fe9cd |
| fused | 27a478073263aef6fad9b095c466dc77a1b85033 | Fuse RGB8/RGBA8 conversion through a private adapter |
| stream | 22e9ab033d5e5ac41ee961de00e7ae9bd959b634 | On fused: retain image planes and stream comparison moments |

## Review boundaries

**Fusion** removes the full-resolution linear pixel buffer from the existing create_image_rgb/create_image_rgba methods and RGB8/RGBA8 decoded image loading. A crate-private GammaImage view implements ToLABBitmap and Downsample internally. There are no new implementations for public integer-pixel ImgRef/ImgVec types. The separate loader crate uses the existing public methods; its 16-bit and grayscale paths retain the original materialized conversion. The existing private ToRGB trait receives an associated Context: Sync: () for linear pixels, the existing lookup table for integer pixels. This passes the table through the shared scalar row body and its existing feature-dispatched clones, without a LabConv abstraction or duplicated arithmetic. Downsampling converts integer pixels to linear before averaging. Existing alpha behavior, dither and cube-root polynomial stay unchanged. Audit size: 7 files, 192 insertions, 35 deletions including tests.

**Streaming** retains only image planes instead of three cached planes per channel/scale. Comparison computes five moments using initialized row buffers: a five-row horizontal ring and vertical scratch, in 16-output-row blocks. Chroma preblur and the original sequential score reductions remain. Full output maps still exist; this is not a fully streaming public API. Six new unsafe call sites only enter runtime/static-feature-checked target-feature functions. There are no new raw-pointer memory operations, MaybeUninit, set_len or row-writer abstractions. Independent original full-plane blur routines remain under cfg(test) as a numerical oracle. Audit size: 7 files, 661 insertions, 177 deletions including tests.

Fusion alone reduces peak transient memory but shows no consistent one-worker time improvement. Streaming cuts retained memory by two thirds and makes preparation much cheaper, but recomputes moments on every comparison. Repeated comparisons regress on Mac and on one-worker Linux. These are separate review decisions; the memory reduction is not a universal speed improvement.

## Method

Rust 1.90.0; portable generic builds with opt-level=3, fat LTO, 16 codegen units, panic=abort and the same lockfile. No target-cpu=native or PGO. Linux: Ryzen 9 7900X, one worker pinned to CPU 2 or six workers pinned to CPUs 0-5. macOS: Apple M4 Pro, normal scheduling. Linux allocator settings: MALLOC_MMAP_THRESHOLD_=67108864 and MALLOC_TRIM_THRESHOLD_=2147483647. The Mac timing script also sets those variables, which do not tune Darwin's allocator.

All pixels are generated before timing; no decoding, file I/O or codecs are timed. Each case warms for 30 ms, then measures nine batches of at least 60 ms. Tables show the range of the two run medians, with variant order reversed for the second round. The raw CSVs include within-run min/max. Linux six-worker preparation results are noisy; do not treat their ratios as precise.

`rgba8_pair_compare` prepares one RGBA8 image and one RGB8 image and compares them. `prepare_pair_compare` uses an already-linear RGBLU/RGBAPLU pair. `compare` borrows an already-prepared linear pair, with no deep clone or preparation charged. Do not add separately measured phase times to estimate repeated-comparison break-even: allocation reuse differs between these cases.

## Current shared-reference comparison timings

Fresh measurements on #197 eb41ae3 versus the current private-adapter + streaming revision 22e9ab0. This isolates `compare(&original, &modified)` for the same already-prepared RGBLU/RGBAPLU pair, 2049 × 1024. Both images are borrowed; no deep clone, preparation, decoding or input generation is timed. SSIM maps are not requested. This is repeated comparison of a fixed pair, not a rotating collection of candidate images.

Rust 1.90, generic portable build and allocator/affinity settings as above. The harness is the original bench.rs with a filter in measure() to time only compare_2049x1024. Three runs, alternating version order; each run retains the nine-batch measurement scheme. Table values are medians of the three run medians. Parentheses show the range of those run medians. Positive change means slower. Linux six-worker baseline timing is noisy; its precise percentage is less certain than the direction.

| Host | Workers | #197, ms | Streaming, ms | Time change |
| --- | ---: | ---: | ---: | ---: |
| linux | 1 | 12.66 (12.65–13.47) | 19.40 (19.14–19.52) | +53.3% |
| linux | 6 | 10.37 (9.38–10.95) | 5.56 (5.51–5.61) | -46.4% |
| mac | 1 | 10.94 (10.90–10.98) | 24.73 (24.71–24.75) | +126.1% |
| mac | 6 | 4.91 (4.90–4.92) | 6.75 (6.73–6.82) | +37.5% |

The cost comes from discarding cached means and squared moments in the streaming commit, not from passing references or from the private input adapter. The first fusion commit does not change the comparison algorithm. For workloads dominated by repeated comparisons of prepared images, retain that distinction when reviewing the two commits; the memory tradeoff in the second commit is substantial.

Reproduce with the current revisions prepared by prepare.py:

```sh
for version in base stream; do
  cargo +1.90.0 build --release --locked --manifest-path /tmp/memory-review/$version/Cargo.toml --bin borrowed
done
python3 benchmarks/memory/run-borrowed.py /tmp/memory-review
```

Raw observations: results/borrowed-*.csv. Earlier whole-pipeline timing tables remain historical; they were not rerun during this focused comparison check.

## Historical performance revisions

The timing and full memory tables below were measured on fused bc028852b1c6ba0c192ddbb77b924eda1e530f9b and stream 04a3f2b6803948cce6769d49a0481841c2087a15. They are retained at fork branch bench/memory-197-public-impls. These are not fresh timing measurements of the private-adapter revision. The measured RGB8/RGBA8 and linear workloads retain the same arithmetic and allocation strategy; the broader old fusion for 16-bit/grayscale loader inputs has been removed. Fresh memory validation for the new revisions is listed below.

## Timings, 2049 × 1024

All numbers are milliseconds, lower is better.

| Host | Workers | Version | RGBA8 prepare | Integer pair + compare | Prepared-pair compare | Linear pair + compare |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| linux | 1 | base | 35.72–36.78 | 105.89–108.31 | 12.59–12.65 | 104.18–105.26 |
| linux | 1 | fused | 34.71–38.76 | 106.41–110.26 | 12.91–15.30 | 103.81–107.08 |
| linux | 1 | stream | 8.14–8.16 | 43.47–43.49 | 20.12–20.74 | 41.20–41.29 |
| linux | 6 | base | 11.87–14.83 | 38.59–44.44 | 8.60–8.69 | 28.88–40.63 |
| linux | 6 | fused | 14.21–14.31 | 42.58–43.70 | 8.69–9.70 | 42.84–43.88 |
| linux | 6 | stream | 3.44–3.46 | 12.70–12.88 | 5.61–5.81 | 14.01–14.24 |
| mac | 1 | base | 16.10–16.62 | 41.43–42.63 | 11.13–11.36 | 39.46–39.92 |
| mac | 1 | fused | 16.19–16.33 | 41.36–43.44 | 10.85–10.90 | 40.29–40.72 |
| mac | 1 | stream | 9.23–9.23 | 42.61–43.41 | 24.69–25.41 | 40.06–40.06 |
| mac | 6 | base | 5.38–5.38 | 15.45–15.50 | 4.95–4.97 | 13.03–13.38 |
| mac | 6 | fused | 4.26–4.42 | 13.19–13.24 | 4.91–5.16 | 13.04–13.59 |
| mac | 6 | stream | 2.22–2.24 | 10.97–11.08 | 6.75–6.86 | 10.36–10.61 |

## Memory

The separate memory executable uses a forwarding System allocator to count requested live heap bytes. These exclude allocator metadata, stack allocations and thread stacks. Both RGBA8 source buffers stay alive through preparation and comparison; this input pair differs from the mixed-format timing pair. Rayon is initialized before the input baseline. The prepared-pair count includes approximately 1 KiB of output-buffer overhead. Peaks are fresh-process observations, not distributions.

Prepared pair is net retained heap above the input baseline. Prepare peak is extra peak heap above that baseline. Compare scratch is extra peak heap above the already-prepared pair plus inputs. Process peak RSS includes inputs and allocator overhead; GNU time reports KiB, Darwin time bytes, converted here to MiB. All columns below are MiB.

At 2049 × 1024, fusion removes exactly 16 bytes/pixel from the one-worker Linux preparation peak (32.02 MiB). Streaming reduces the retained prepared pair from about 192 MiB to 64 MiB. RSS falls from 281 to 99 MiB on Linux and 285 to 93 MiB on Mac, with one worker.

| Host | Size | Workers | Version | Prepared pair | Prepare peak | Compare scratch | Process peak RSS |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: |
| linux | 2049x1024 | 1 | base | 192.03 | 228.07 | 32.02 | 281.18 |
| linux | 2049x1024 | 1 | fused | 192.03 | 196.05 | 32.02 | 249.07 |
| linux | 2049x1024 | 1 | stream | 64.01 | 72.02 | 8.71 | 99.13 |
| linux | 2049x1024 | 6 | base | 192.03 | 248.06 | 61.52 | 317.52 |
| linux | 2049x1024 | 6 | fused | 192.03 | 216.04 | 61.90 | 285.83 |
| linux | 2049x1024 | 6 | stream | 64.01 | 84.02 | 14.37 | 129.19 |
| linux | 4097x2048 | 1 | base | 767.96 | 912.08 | 128.03 | 1033.14 |
| linux | 4097x2048 | 1 | fused | 767.96 | 784.05 | 128.03 | 1000.98 |
| linux | 4097x2048 | 1 | stream | 255.99 | 288.02 | 33.41 | 395.15 |
| linux | 4097x2048 | 6 | base | 767.96 | 992.01 | 247.55 | 1191.08 |
| linux | 4097x2048 | 6 | fused | 767.96 | 863.98 | 246.05 | 1148.56 |
| linux | 4097x2048 | 6 | stream | 255.99 | 352.04 | 40.45 | 460.22 |
| mac | 2049x1024 | 1 | base | 192.03 | 228.07 | 32.02 | 284.97 |
| mac | 2049x1024 | 1 | fused | 192.03 | 196.05 | 32.02 | 252.95 |
| mac | 2049x1024 | 1 | stream | 64.01 | 72.02 | 8.71 | 93.00 |
| mac | 2049x1024 | 6 | base | 192.03 | 248.06 | 62.02 | 315.67 |
| mac | 2049x1024 | 6 | fused | 192.03 | 216.04 | 60.40 | 284.39 |
| mac | 2049x1024 | 6 | stream | 64.01 | 90.78 | 14.50 | 115.75 |
| mac | 4097x2048 | 1 | base | 767.96 | 912.08 | 128.03 | 1133.02 |
| mac | 4097x2048 | 1 | fused | 767.96 | 784.05 | 128.03 | 1004.92 |
| mac | 4097x2048 | 1 | stream | 255.99 | 288.02 | 33.41 | 365.02 |
| mac | 4097x2048 | 6 | base | 767.96 | 992.01 | 246.05 | 1247.78 |
| mac | 4097x2048 | 6 | fused | 767.96 | 879.98 | 238.05 | 1122.30 |
| mac | 4097x2048 | 6 | stream | 255.99 | 368.03 | 49.74 | 454.73 |

## Private adapter follow-up validation

The public integer-image ToLABBitmap and Downsample implementations have been removed. GammaImage is crate-private, and the added ToRGB::Context associated type and parameter remain crate-private. Existing public function and trait signatures are unchanged. The source-level change also eliminates the concrete-format downsampling macro: one generic implementation on the private adapter is sufficient.

Both current production revisions passed cargo-semver-checks 0.49.0 with rustc 1.98.1, comparing dssim and dssim-core against #197 eb41ae3 with default features and with no enabled features: 223 checks passed and 30 skipped per crate/configuration. This detects known compatibility violations; it is not by itself proof of API equality. Logs: results/private-*-semver-*.log. The original public-implementation audit logs remain as historical evidence.

Commands, run from each current production checkout:

```sh
cargo semver-checks -p dssim-core -p dssim --baseline-rev eb41ae307bfda358e41c022df2e2956fe5fd868c --default-features
cargo semver-checks -p dssim-core -p dssim --baseline-rev eb41ae307bfda358e41c022df2e2956fe5fd868c --only-explicit-features
```

Rust 1.90 tests were rerun on both revisions with and without threads: Linux workspace tests (four root tests and 26/30 core tests), Mac core tests (26/30). The full 220,370,288-byte parity corpus and scalar score outputs were compared with #197 again on each host; both revisions match byte-for-byte and retain the per-host hashes below. Locked score probes still report 0.338437.

Fresh one-worker, 2049 × 1024 allocation runs on Linux and Mac confirm the exact same prepared-pair, preparation-peak and comparison-scratch heap counts as the historical measurements. Fusion still saves 33,570,816 bytes of preparation peak heap, and streaming retains about 64 MiB instead of 192 MiB per prepared pair. Raw fresh data: results/private-*-memory-*.csv and .rss. Repeated-comparison costs and historical timing caveats still apply; the subsequent focused shared-reference rerun is documented above.

## Correctness and safety checks

- Rust 1.90 release core tests with default features and with --no-default-features pass on Linux and Mac: 26 tests at fused, 30 at stream.
- Linux workspace release tests pass, including four root tests. The core package builds and verifies with cargo package.
- Full-output parity includes linear Lab, integer image creation, downsampling, saved maps and scalar scores, 11 sizes through 2049 × 1024, padded rows, odd dimensions and partial/zero alpha. Each variant writes 220,370,288 bytes of pixel/map bit patterns. All three variants are byte-identical on Linux and hash-identical on Mac; score text also matches per host.
- Linux pixel/map SHA-256, all three versions: `99dcf2e16049bd1d0d9b5e14b1324286c3d9ea0bedc7f6f4a17fe7f2fddf4c98`.
- Mac pixel/map SHA-256, all three versions: `fe3ca30264e9d16b2218236d464110c825b424cede2bd800eca2183a7825a8e2`.
- The broader corpus differs across hosts already at the #197 baseline. This establishes per-host parity with #197, not cross-platform bit identity for the entire corpus. Large binary dumps are omitted from Git; the generator and score outputs are included.
- The locked 2049 × 1024 probe reports `ssim=0.338437` for all three versions. Its one-shot timing/RSS output is not used in the performance tables.
- Ring tests independently compare the original full-plane blur and product-blur results, including tiny images, block boundaries and padded rows. Dispatch tests compare available baseline/AVX2/AVX512 tiers.
- Mac assembly inspection confirms NEON vectorization in the new horizontal/vertical moment kernels and SSIM body. The one-worker regression remains despite vectorization; it is consistent with doing previously cached work again. No ARM dispatch tier was added.

## Reproduction

This directory is a standalone Cargo workspace with a relative dependency on this checkout's dssim-core. For the current private-adapter revisions, prepare separate worktrees/harnesses, then build each. To reproduce the historical tables, add --historical to prepare.py:

```sh
python3 benchmarks/memory/prepare.py /tmp/memory-review
for version in base fused stream; do
  cargo +1.90.0 build --release --locked --manifest-path /tmp/memory-review/$version/Cargo.toml --bins
done
python3 benchmarks/memory/run-linux.py /tmp/memory-review
# Or on Mac:
python3 benchmarks/memory/run-mac.py /tmp/memory-review
python3 benchmarks/memory/mac-memory.py /tmp/memory-review
```

The Linux runner includes memory measurements. CPU affinity may need adjustment for another host. Clear RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS and target-specific Rust flags for comparable generic builds. Run parity separately with RAYON_NUM_THREADS=1, redirecting stdout to a .bits file and stderr to a .scores file, then compare both. Each binary dump is about 220 MB. Run `score 2049 1024 1` on Linux for the historical locked-score probe (its RSS reporting uses /proc).

Raw measurements live in results/. No benchmark harness or measurement files are included in the two production commits.
