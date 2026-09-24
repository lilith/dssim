# Optional cached and low-memory images

Production branch: `perf/memory-modes-197`, commit **f300435f330056a7707e39aa53bad014c34373e8**. It adds one commit above private input fusion 27a4780 and streaming comparison 22e9ab0, both based on PR #197 **eb41ae307bfda358e41c022df2e2956fe5fd868c** (whose parent is upstream main 0e44c9b7fe91a5265c8e463436c28512186fe9cd). Measurements use that exact #197 baseline.

## API and behavior

`Dssim::new()` prepares cached images by default, retaining the mean and squared-moment planes used for repeated comparisons. `set_low_memory(true)` changes how subsequent images are prepared: only image planes are retained. Existing images are unaffected. `compare` keeps its existing signature and uses the caches actually present on its two inputs. All four cached/streaming combinations work, including a cached reusable reference with a streaming candidate.

This is one intentional public API addition, the setter. No public image type, integer-image trait implementation, new dependency, lock, or interior mutation is added. Images continue to be usable through shared references. The Rust setter is not exposed as a new CLI option or C API entry point in this prototype.

```rust
let mut d = dssim_core::Dssim::new();
let reference = d.create_image_rgb(&reference_pixels, width, height).unwrap();
d.set_low_memory(true);
let candidate = d.create_image_rgb(&candidate_pixels, width, height).unwrap();
let (score, maps) = d.compare(&reference, &candidate);
```

Here `reference_pixels` and `candidate_pixels` are RGB<u8> slices of matching dimensions. The reference keeps its moments; setting low-memory mode does not alter it.

## Implementation and review cost

The additional commit changes six files: 202 insertions, 70 deletions including tests. Each private channel has `Option<CachedMoments<T>>`. Cached preparation and cached-pair cross blur reuse #197's full-plane routines, restored from test-only to production. The existing SSIM arithmetic and ordered score reductions are shared unchanged.

If either image lacks moments, the row driver reads any retained planes directly and computes only the missing moments plus the cross product. Two const-bool parameters specialize the existing canonical horizontal body and row driver. Selection happens once per block; no cache-policy branches remain in pixel arithmetic. Existing AVX2/AVX512 feature checks and static elision are preserved. Fully cached pairs retain the full-plane comparison path because forcing them through a row ring was slower on Mac.

No new unsafe memory operation is introduced by this commit. Restoring the cached path makes the original full-plane blur allocation/initialization helpers production code again; their unsafe bodies are unchanged. Streaming scratch remains initialized, bounds-checked slices. The different policies are immutable properties of the prepared images; no automatic cache allocation occurs during comparison.

## Method

Rust 1.90.0, generic portable builds, opt-level=3, fat LTO, 16 codegen units, panic=abort, identical lockfiles. No target-cpu=native or PGO. Linux Ryzen 9 7900X: one worker pinned to CPU 2, six workers pinned to CPUs 0–5; MALLOC_MMAP_THRESHOLD_=67108864 and MALLOC_TRIM_THRESHOLD_=2147483647. Apple M4 Pro: normal scheduling and default allocator.

All inputs are generated before timing; codecs and file I/O are excluded. Deterministic 2049 × 1024 RGBLU reference and RGBAPLU candidate. `compare` repeatedly borrows the same two prepared images with no deep clone or preparation timed. `candidate_create_compare` prepares the candidate inside each timed iteration and compares it against an already-prepared reference; the candidate is dropped inside that iteration. It models one-use candidates without charging reference preparation. It does not measure a rotating dataset or memory pressure from many concurrent comparisons.

Each case warms for 30 ms and measures nine batches of at least 60 ms. Tables show the range of two run medians; the second run reverses version order. Linux multi-worker timing is noisy; percentages should not be treated as precise. Raw CSVs include each run's min/max. C = cached, S = streaming; first letter is reference policy, second is candidate policy.

## Timings

Milliseconds, lower is better. The baseline is #197's cached behavior; CC is the new default. CS is the intended reusable-reference/one-use-candidate mix. SC is included for symmetry and correctness, but caching a one-use candidate while repeatedly recomputing the reference is generally a poor choice.

| Host | Workers | Mode | Prepared comparison | Candidate creation + comparison |
| --- | ---: | --- | ---: | ---: |
| linux | 1 | BASE | 12.75–13.86 | 59.58–59.89 |
| linux | 1 | CC | 12.45–12.48 | 39.04–39.09 |
| linux | 1 | CS | 15.90–16.31 | 24.20–24.54 |
| linux | 1 | SC | 15.91–16.16 | 49.74–49.94 |
| linux | 1 | SS | 19.06–19.21 | 27.36–27.40 |
| linux | 6 | BASE | 8.64–8.73 | 25.06–25.35 |
| linux | 6 | CC | 8.71–8.71 | 23.37–25.10 |
| linux | 6 | CS | 5.38–5.46 | 12.00–12.08 |
| linux | 6 | SC | 5.45–5.46 | 16.88–21.40 |
| linux | 6 | SS | 5.52–5.82 | 10.21–12.24 |
| mac | 1 | BASE | 10.50–10.90 | 26.00–26.99 |
| mac | 1 | CC | 10.52–10.62 | 25.76–27.67 |
| mac | 1 | CS | 18.71–18.81 | 26.90–27.27 |
| mac | 1 | SC | 18.75–18.75 | 33.77–33.94 |
| mac | 1 | SS | 24.53–24.54 | 32.79–33.01 |
| mac | 6 | BASE | 4.64–4.87 | 8.98–9.32 |
| mac | 6 | CC | 4.72–4.86 | 9.07–9.10 |
| mac | 6 | CS | 5.67–5.72 | 7.73–7.82 |
| mac | 6 | SC | 5.71–5.72 | 9.78–9.91 |
| mac | 6 | SS | 6.77–6.79 | 8.72–8.84 |

Cached comparison speed is preserved in this probe. Mixed mode is still slower for repeated comparisons on Mac: roughly 18.8 ms versus 10.6 ms with one worker, and 5.7 ms versus 4.8 ms with six. When the candidate must be prepared for each comparison, the six-worker Mac case improves from about 9.1 ms in CC to 7.8 ms in CS; one-worker Mac remains roughly level at 26–28 ms. Streaming both images remains a larger repeated-comparison tradeoff. Favor cached preparation for images reused often; use streaming where reduced retention and cheaper preparation justify recomputation.

## Memory

Fresh-process, one-worker, 2049 × 1024 pair of RGBA8 inputs. The forwarding System allocator counts requested heap bytes, excluding allocator metadata and stacks. Both source buffers stay alive through comparison; this pair differs from the linear timing inputs. Rayon is initialized before measurement. Prepared pair = retained heap above input baseline (including about 1 KiB of output-buffer overhead); prepare peak is above that same baseline; comparison scratch is above the already-prepared pair and inputs. RSS is the process high-water mark from /usr/bin/time. Values are MiB, single observations.

| Host | Mode | Prepared pair | Prepare peak | Comparison scratch | Peak RSS |
| --- | --- | ---: | ---: | ---: | ---: |
| linux | CC | 192.03 | 196.05 | 32.02 | 249.14 |
| linux | CS | 128.02 | 136.03 | 8.71 | 162.96 |
| linux | SC | 128.02 | 132.04 | 8.71 | 160.91 |
| linux | SS | 64.01 | 72.02 | 8.71 | 99.12 |
| mac | CC | 192.03 | 196.05 | 32.02 | 252.95 |
| mac | CS | 128.02 | 136.03 | 8.71 | 157.28 |
| mac | SC | 128.02 | 132.04 | 8.71 | 158.27 |
| mac | SS | 64.01 | 72.02 | 8.71 | 93.05 |

## Validation

- Rust 1.90: 31 core release tests pass on Linux and Mac, with and without threads. Linux workspace tests also pass (four root tests). The mode tests exercise cache-policy changes, mixed inputs, scalar scores, saved maps, tiny/odd/padded images and all available x86 feature tiers.
- All four policies reproduce #197's full-output parity corpus byte-for-byte on each host: 220,370,288 bytes per mode, plus matching scalar-score text. Linux SHA-256: `99dcf2e16049bd1d0d9b5e14b1324286c3d9ea0bedc7f6f4a17fe7f2fddf4c98`. Mac: `fe3ca30264e9d16b2218236d464110c825b424cede2bd800eca2183a7825a8e2`. These establish same-host parity; the full baseline corpus already differs across hosts. Binary dumps are not stored in Git.
- cargo-semver-checks 0.49.0, rustc 1.98.1: dssim and dssim-core pass against exact #197, with default features and with none enabled (223 passed, 30 skipped per crate/configuration). This is a compatibility check, not a claim of unchanged API: set_low_memory is intentionally added.

## Reproduction

The harness in this directory builds against the current checkout. To recreate the exact baseline and proposal in separate worktrees:

```sh
python3 benchmarks/memory-modes/prepare.py /tmp/modes-review
for version in base modes; do
  cargo +1.90.0 build --release --locked --manifest-path /tmp/modes-review/$version/Cargo.toml --bins
done
python3 benchmarks/memory-modes/run.py /tmp/modes-review
python3 benchmarks/memory-modes/check.py /tmp/modes-review
```

Use a fresh directory and clear RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS and target-specific compiler flags for portable builds. Adjust the Linux CPU affinity in run.py/check.py for another machine. check.py writes about 1.1 GB of parity dumps, compares them against the baseline, records hashes and measures allocation/RSS. Run the scripts separately on Linux and Mac; each detects the host for its filenames and time/affinity commands. The score binary is a historical Linux-only RSS/locked-score probe and is not used in these timing tables.
