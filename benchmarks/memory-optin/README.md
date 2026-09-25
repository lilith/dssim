# Construction-time, per-image moment caching

> For current end-to-end results, including preparation costs, upstream main, and the current per-image caching API, use the [preparation and caching benchmark](https://github.com/lilith/dssim/blob/bench/e2e-cache-review/benchmarks/e2e/README.md). This report retains earlier experiments at their stated revisions and settings; its timings should not be mixed with the newer matrix.
> The context-wide flag and `cache_moments` API shown below are superseded by construction-time `ImageOptions::cache_for_reuse` in [#200](https://github.com/kornelski/dssim/pull/200).

Production: `perf/memory-optin-197`, **1b891ad2597d70f21d90ffb874bac53fd00a37c9**.
This is above private gamma-input fusion 27a4780, streaming moments 22e9ab0,
and mixed cache support f300435. The exact baseline is PR #197
**eb41ae307bfda358e41c022df2e2956fe5fd868c**, whose parent is upstream
main **0e44c9b7fe91a5265c8e463436c28512186fe9cd**.

## API and implementation

```rust
let d = dssim_core::Dssim::new();
let reference = d.create_image_rgb_with_options(
    &reference_pixels, width, height,
    dssim_core::ImageOptions::default().cache_moments(true),
).unwrap();
let candidate = d.create_image_rgb(&candidate_pixels, width, height).unwrap();
let (score, maps) = d.compare(&reference, &candidate);
```

Here the inputs are matching RGB<u8> slices. Existing constructors now retain
only image planes. A reusable reference can opt into retaining mean and
squared-moment planes at creation. There are `_with_options` variants for
RGB8, RGBA8, generic images, and the `dssim::load_image` file loader. There is
no method to add caches to a prepared image and no context-wide cache setter.
The setter removed from f300435 was an unreleased prototype, absent from #197.
The C API and CLI use the compact default; this change adds no C/CLI cache switch.

The option is a private-field, copyable value passed to the existing preparation
implementation. Caching shares the scratch buffer used for chroma preparation
and runs alongside preparation of other scales. This avoids the separate scratch
allocations and parallel phase of the discarded `image.cache_moments()` prototype.
The cache math, comparison implementations, dispatch checks, static feature
elision, and ordered reductions are unchanged from f300435. No new unsafe code,
locks, interior mutation, or comparison-time option checks are introduced.

Both prepared-image policies work through shared references. Comparison reads
retained moments where available and computes missing ones without mutating
images. All four combinations are supported. Fully cached pairs keep the
existing full-plane comparison path; mixed/compact pairs use the row pipeline.

## Measurement method

Rust 1.90.0, generic portable builds, opt-level=3, fat LTO, 16 codegen units,
panic=abort, identical lockfiles. No native targeting or PGO. Input generation,
file I/O, and codecs are outside timing. Each observation warms for 50 ms and
measures seven batches of at least 60 ms. Tables use the median of three
independent process medians; CSVs retain every batch and run range.

- `compare`: repeatedly borrow two prepared images, without cloning.
- `candidate`: prepare, compare and drop each candidate; reference preparation
  is outside timing.
- `pair`: prepare both images, compare and drop them inside timing.
- CC: cache both; CS: cache only reference; SS: cache neither (new default).

The main matrix compares #197 (`base`) with immutable f300435 using CC/CS/SS.
It covers 512x512 and 3840x2160, linear and RGBA8 inputs, and 1/6/12 workers on
Apple M4 Pro (8 performance + 4 efficiency cores, 24 GiB). Mac RGBA8 spot checks
also cover 513x511, 4097x2048 and 2049x1024 at six workers. Linux Ryzen 7900X
covers the two principal sizes with RGBA8 inputs at 1/6 workers. One Linux worker
is pinned to CPU 2; six to CPUs 0-5. Linux uses MALLOC_MMAP_THRESHOLD_=67108864
and MALLOC_TRIM_THRESHOLD_=2147483647. Mac uses normal OS scheduling/default
allocator. Neither machine is exclusively reserved.

The construction API A/B repeats RGBA8 at both sizes, all three scenarios and
all policies: `old` is f300435, `new` is 1b891ad. Runs shuffle group/version
order across three rounds. The main and API matrices are separate experiments;
prefer within-experiment comparisons. `post_creation` rows document the discarded
uncommitted API prototype and are historical diagnostics, not the current source.

Mac runs sample other-process CPU every 0.5 s, wait before starting when it
exceeds 60% of one core, and reject/retry runs exceeding 80%. Background CPU
activity delayed the construction run; rejected observations are excluded.
This is load screening, not CPU isolation. Linux's large multithreaded workloads
show substantial allocation/scheduling variation. A seven-run, alternating-order
follow-up of its fully cached 4K pair is included as `focused`, alongside the
original results rather than replacing them.

## Results

Tables are generated below from the saved CSVs. Small percentages without
separation between run ranges should be treated as noise, not speedups.

### API overhead: both images cached, complete pair preparation + comparison

Old = f300435 context setter; new = construction options. Milliseconds.

| Host | Size | Workers | Old | New | Change |
| --- | --- | ---: | ---: | ---: | ---: |
| mac | 512x512 | 1 | 6.032 | 6.021 | -0.2% |
| mac | 512x512 | 6 | 1.731 | 1.696 | -2.0% |
| mac | 512x512 | 12 | 1.928 | 1.919 | -0.5% |
| mac | 3840x2160 | 1 | 168.310 | 168.993 | +0.4% |
| mac | 3840x2160 | 6 | 50.004 | 49.653 | -0.7% |
| mac | 3840x2160 | 12 | 47.441 | 48.541 | +2.3% |
| linux | 512x512 | 1 | 5.011 | 5.022 | +0.2% |
| linux | 512x512 | 6 | 1.639 | 1.648 | +0.6% |
| linux | 3840x2160 | 1 | 385.853 | 385.907 | +0.0% |
| linux | 3840x2160 | 6 | 178.719 | 204.231 | +14.3% |

Stable single-worker and small-pair cases are close. Construction options remove
the separate post-preparation caching phase, but the measurements do not justify
a blanket zero-overhead claim:

- Linux six-worker 4K cached pairs initially measured 178.72 ms old versus
  204.23 ms new. Seven alternating follow-up runs gave medians 185.43 versus
  189.14 ms (+2.0%), with wide overlapping ranges: 169.73–202.38 and
  160.60–193.30 ms. Preserve the original observation; no precise cost estimate
  is defensible for this variable workload.
- Mac six-worker 512x512 **candidate creation + comparison, both cached**
  measured 1.104 versus 1.217 ms (+10.3%). Seven alternating follow-up runs
  gave 1.112 versus 1.205 ms (+8.3%), ranges 1.093–1.220 and 1.115–1.226 ms.
  This remains a measured regression for that configuration; its cause is
  unresolved. It is not evidence of an extra caching phase in the source.
- Mac twelve-worker cached-pair overhead from the discarded post-creation
  prototype was +14.8% at 512x512 and +9.5% at 4K in its own A/B. The current
  construction A/B gives -0.5% and +2.3%, respectively. These are separate
  experiments, so the numbers are not a direct paired comparison between
  the two prototypes.

### Choosing the default: current construction API

Milliseconds, RGBA8, twelve Mac workers or six Linux workers. These are current
construction runs, not the earlier main matrix. Retaining moments benefits
repeated prepared comparisons, while compact images favor one-shot processing
and lower retention. Reference-only caching often helps the one-use candidate
workload. The full results include single-worker cases where streaming is slower.

| Host | Size | Scenario | CC | CS | SS (default) |
| --- | --- | --- | ---: | ---: | ---: |
| mac | 512x512 | compare | 0.724 | 0.737 | 0.843 |
| mac | 512x512 | candidate | 1.339 | 1.140 | 1.266 |
| mac | 512x512 | pair | 1.919 | 1.729 | 1.694 |
| mac | 3840x2160 | compare | 18.586 | 17.688 | 21.180 |
| mac | 3840x2160 | candidate | 32.742 | 25.642 | 28.754 |
| mac | 3840x2160 | pair | 48.541 | 42.091 | 38.891 |
| linux | 512x512 | compare | 0.555 | 0.595 | 0.697 |
| linux | 512x512 | candidate | 1.117 | 0.931 | 1.016 |
| linux | 512x512 | pair | 1.648 | 1.370 | 1.334 |
| linux | 3840x2160 | compare | 51.771 | 22.634 | 26.490 |
| linux | 3840x2160 | candidate | 124.775 | 54.531 | 39.191 |
| linux | 3840x2160 | pair | 204.231 | 108.642 | 62.828 |

For context, the earlier main matrix's Mac 4K twelve-worker one-shot pipeline
was 57.09 ms for exact #197, 47.03 ms for CC, 39.95 ms for CS, and 36.70 ms for
SS. Those include the memory branch's input fusion and streaming changes; they
are not speedups attributable solely to the options API. At one worker,
prepared 4K comparisons were 42.03 ms CC, 70.05 ms CS and 93.65 ms SS: caching
is useful when reuse makes its preparation and memory costs worthwhile.

## Memory

Fresh one-worker processes; forwarding System allocator counts requested heap
bytes, not allocator metadata or stacks. Both RGBA8 source buffers remain live.
Prepared/prepare-peak values exclude the input baseline; comparison scratch is
additional to the prepared pair. RSS is the process high-water mark. All MiB.

| Host | Size | Mode | Prepared pair | Prepare peak | Compare scratch | Peak RSS |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| linux | 512x512 | CC | 24.00 | 24.50 | 4.00 | 33.77 |
| linux | 512x512 | CS | 16.00 | 17.00 | 1.18 | 23.14 |
| linux | 512x512 | SS | 8.00 | 9.00 | 1.18 | 15.02 |
| linux | 3840x2160 | CC | 759.19 | 775.10 | 126.56 | 975.41 |
| linux | 3840x2160 | CS | 506.13 | 537.80 | 32.96 | 643.43 |
| linux | 3840x2160 | SS | 253.07 | 284.74 | 32.96 | 389.73 |
| mac | 512x512 | CC | 24.00 | 24.50 | 4.00 | 34.22 |
| mac | 512x512 | CS | 16.00 | 17.00 | 1.18 | 21.59 |
| mac | 512x512 | SS | 8.00 | 9.00 | 1.18 | 13.53 |
| mac | 3840x2160 | CC | 759.19 | 775.10 | 126.56 | 993.91 |
| mac | 3840x2160 | CS | 506.13 | 537.80 | 32.96 | 614.47 |
| mac | 3840x2160 | SS | 253.07 | 284.74 | 32.96 | 361.02 |

## Validation and provenance

- Rust 1.90 workspace release tests on Linux: 31 core + 4 root tests and the
  constructor-option doctest, with and without default features. Mac: 31 core
  tests + doctest, both feature configurations.
- cargo-semver-checks 0.49.0 with rustc 1.98.1 against exact #197: both dssim
  and dssim-core pass with default features and none (223 passed, 30 skipped
  per crate/configuration). The options API is intentionally additive relative
  to #197; the default's performance/memory behavior changes as described above.
- All four cache policies match #197's full-output corpus byte-for-byte on each
  host, including scalar scores and maps: 220,370,288 bytes per policy.
  Linux SHA-256 `99dcf2e16049bd1d0d9b5e14b1324286c3d9ea0bedc7f6f4a17fe7f2fddf4c98`;
  Mac `fe3ca30264e9d16b2218236d464110c825b424cede2bd800eca2183a7825a8e2`.
  These establish same-host parity; the upstream full corpus already differs
  across hosts. The corpus covers linear RGB/RGBA, RGB8/RGBA8, grayscale,
  tiny/odd/padded sizes and downsampled pixels.
- 1,792 accepted timing observations, all score bits agree within every
  host/shape/input-format group. Main Mac matrix: 540 accepted / 2 rejected;
  historical post-creation A/B: 324 / 1; construction A/B: 324 / 6.
  Linux main/API matrices have 144/216/216 observations; each focused follow-up
  has 14. Rejected samples do not enter summaries.
- `measurements/results.csv` preserves process medians and all batch times;
  `summary.csv` preserves across-process ranges. `metadata` contains run/load
  records with process names removed from published load logs. `validation`
  contains tests, scores, parity hashes, allocator counters and RSS. Large binary
  dumps are deliberately not committed.

## Reproduction

Use a fresh output directory. Git must contain the three revisions listed at
this file's top. The prepare script exports immutable sources and identical
lockfiles; it does not modify an existing checkout.

```sh
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
python3 benchmarks/memory-optin/prepare.py /tmp/cache-review
cargo +1.90.0 build --release --locked --manifest-path /tmp/cache-review/base/Cargo.toml --bins
cargo +1.90.0 build --release --locked --manifest-path /tmp/cache-review/modes/Cargo.toml --bins --features modes
cargo +1.90.0 build --release --locked --manifest-path /tmp/cache-review/construction/Cargo.toml --bins
# Mac: run these sequentially, without concurrent builds/tests.
python3 /tmp/cache-review/run.py
python3 /tmp/cache-review/construction-run.py
# Linux equivalents (adjust taskset CPUs for your machine):
python3 /tmp/cache-review/linux-run.py
python3 /tmp/cache-review/linux-construction-run.py
# Validation runs separately from timing, on each host:
python3 /tmp/cache-review/check.py
```

Clear any target-specific compiler flags too. The check script writes about
1.1 GB of output dumps; they can be removed after comparison. The standalone
Cargo.toml in this directory also builds the current constructor harness against
the enclosing checkout. `summarize.py RAW_ROOT OUTPUT_DIR` aggregates accepted
runs and checks score parity. Historical post-creation rows are optional inputs;
the discarded prototype is not part of the reproduction or proposed source.
