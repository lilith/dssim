# Rechecking SSIM chunk size

The original dispatch work changed the RGB SSIM map iteration from
`with_min_len(1024)` to `par_chunks_mut(4096)`. Commit 388414e's comment says the
4096 size was chosen to amortize capability checks. PR #197 carries that choice
as `SSIM3_CHUNK = 1 << 12`; the memory branch's cached comparison path uses the
same value for RGB and grayscale. These are kernel-call/iterator-item sizes:
Rayon can batch multiple chunks into a job. Its old `with_min_len` was a minimum
split size, not a guarantee of precisely 1024 pixels per job.

This experiment varies only that constant, keeping arithmetic, reductions,
feature checks, input generation, and allocation strategy fixed within each
lineage. It does not change the independent 16-row streaming blocks. Thus it
affects cached/cached comparisons; compact and mixed images do not use this
chunked full-plane path.

## Sources and variants

- `current`: construction-only opt-in API, **1b891ad2597d70f21d90ffb874bac53fd00a37c9**;
  1024, 2048, 4096, 8192 pixels.
- `pr197`: exact PR #197, **eb41ae307bfda358e41c022df2e2956fe5fd868c**;
  1024 versus 4096. Its parent is upstream main
  **0e44c9b7fe91a5265c8e463436c28512186fe9cd**.
- `modes`: earlier context-setting API, **f300435f330056a7707e39aa53bad014c34373e8**;
  1024 versus 4096, to revisit the candidate-creation regression probe.

All variants are fresh builds from separate worktrees. The construction and
context APIs both already used 4096, so that shared constant cannot by itself
explain their previous performance difference. Compare chunk sizes *within*
a lineage. Comparisons with #197 also include unrelated input fusion and memory
changes and should not be attributed to chunk size alone.

## Method

Rust 1.90.0, identical lockfile, generic portable builds, opt-level=3, fat LTO,
16 codegen units, panic=abort. No native targeting or PGO. Linux Ryzen 7900X:
1 worker pinned to CPU 2, 6 workers to CPUs 0–5; allocator thresholds
MALLOC_MMAP_THRESHOLD_=67108864 and MALLOC_TRIM_THRESHOLD_=2147483647.
Apple M4 Pro: 1/6/12 workers, normal OS scheduling/default allocator. The hosts
are not exclusively reserved. AArch64 compiles out x86 feature checks, so Mac
chunk effects measure kernel-call and scheduling granularity, not runtime CPU
feature detection. The x86 path uses cached detection and the same dispatched
AVX-512 kernels across sizes on this machine.

RGBA8 inputs are generated before timing; codecs and file I/O are excluded.
Each process warms for 50 ms, then records seven batches of at least 60 ms.
Three rounds shuffle groups and chunk order. Tables show the median of the
three process medians; raw files preserve within-process batches and across-run
ranges. Both images retain moments (CC) in every timing case.

- `compare`: repeatedly borrow two prepared images, no deep clone or creation.
- `candidate`: prepare, compare and drop each candidate; reference preparation
  is outside timing.
- `pair`: prepare, compare and drop both images inside timing.

The current-API sweep covers 256x256, 512x512 and 3840x2160, all scenarios and
worker counts. #197 covers 512x512 and 4K, compare/pair, all worker counts.
The context API covers 512x512 and 4K candidate creation at six workers.

Both runners sample other-process CPU every 0.5 seconds, gate starts above 60%
of one core, and reject/retry observations exceeding 80%. This screens obvious
CPU contention, not every source of timing noise; allocator behavior, scheduler
placement and memory bandwidth still vary, particularly for parallel 4K cases.

## Results
The sweep does **not** show a consistent benefit from reducing 4096 to 1024.
The production chunk size is unchanged. Across the current API's cases, the
unweighted geometric mean of (1K time / 4K time) is **1.0049 on Mac** and
**1.0040 on Linux**: about 0.5% and 0.4% slower at 1K, with individual cases
moving in either direction. This is a descriptive aggregate across workloads,
not a statistical significance estimate or a prediction for a particular app.
For #197 itself the same ratios are 0.9939 (Mac) and 0.9997 (Linux), likewise
insufficient to establish a general winner. The data does not establish that
4K is necessary to amortize dispatch; it also does not justify reverting it.

### Current API: prepared comparisons

Milliseconds; both images cached. These measure comparisons without image
creation. Values are medians of three process medians. See `summary.csv` for
ranges before interpreting small differences.

| Host | Size | Workers | 1024 | 2048 | 4096 (current) | 8192 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| mac | 256x256 | 1 | 0.343 | 0.341 | 0.340 | 0.338 |
| mac | 256x256 | 6 | 0.198 | 0.193 | 0.190 | 0.221 |
| mac | 256x256 | 12 | 0.248 | 0.314 | 0.241 | 0.253 |
| mac | 512x512 | 1 | 1.453 | 1.445 | 1.439 | 1.436 |
| mac | 512x512 | 6 | 0.642 | 0.651 | 0.646 | 0.671 |
| mac | 512x512 | 12 | 0.772 | 0.726 | 0.780 | 0.740 |
| mac | 3840x2160 | 1 | 42.599 | 42.221 | 42.226 | 43.654 |
| mac | 3840x2160 | 6 | 18.806 | 19.054 | 19.241 | 18.893 |
| mac | 3840x2160 | 12 | 18.703 | 20.310 | 18.963 | 18.760 |
| linux | 256x256 | 1 | 0.288 | 0.284 | 0.283 | 0.284 |
| linux | 256x256 | 6 | 0.135 | 0.135 | 0.136 | 0.136 |
| linux | 512x512 | 1 | 1.184 | 1.162 | 1.148 | 1.141 |
| linux | 512x512 | 6 | 0.558 | 0.551 | 0.556 | 0.556 |
| linux | 3840x2160 | 1 | 90.642 | 89.026 | 88.665 | 88.242 |
| linux | 3840x2160 | 6 | 53.906 | 51.807 | 51.320 | 52.944 |

### Current API: candidate preparation + comparison

Reference is already prepared; candidate creation/comparison/drop are timed.
This includes the 512x512 case that previously showed an API regression.

| Host | Size | Workers | 1024 | 2048 | 4096 (current) | 8192 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| mac | 256x256 | 1 | 0.825 | 0.823 | 0.820 | 0.819 |
| mac | 256x256 | 6 | 0.333 | 0.335 | 0.344 | 0.343 |
| mac | 256x256 | 12 | 0.525 | 0.515 | 0.515 | 0.524 |
| mac | 512x512 | 1 | 4.124 | 4.118 | 4.114 | 4.106 |
| mac | 512x512 | 6 | 1.169 | 1.215 | 1.102 | 1.123 |
| mac | 512x512 | 12 | 1.339 | 1.323 | 1.302 | 1.319 |
| mac | 3840x2160 | 1 | 105.527 | 105.637 | 107.729 | 105.320 |
| mac | 3840x2160 | 6 | 34.407 | 34.043 | 34.944 | 34.158 |
| mac | 3840x2160 | 12 | 32.636 | 32.436 | 33.019 | 32.996 |
| linux | 256x256 | 1 | 0.811 | 0.812 | 0.807 | 0.807 |
| linux | 256x256 | 6 | 0.269 | 0.268 | 0.266 | 0.267 |
| linux | 512x512 | 1 | 3.028 | 3.024 | 3.028 | 2.963 |
| linux | 512x512 | 6 | 1.125 | 1.124 | 1.134 | 1.108 |
| linux | 3840x2160 | 1 | 287.958 | 286.432 | 285.784 | 284.360 |
| linux | 3840x2160 | 6 | 124.421 | 114.520 | 127.773 | 124.408 |

8192 does not offer a consistent improvement either. For example, on Mac at
six workers, prepared 256x256 comparisons rise from 0.190 ms at 4K to 0.221 ms
at 8K. Conversely, some other cases favor 8K. There is no robust case here for
a more complicated size or architecture-dependent policy.

### Exact #197, 1024 versus 4096

Both sizes use the same #197 source except for `SSIM3_CHUNK`. Milliseconds.

| Host | Size | Workers | Scenario | 1024 | 4096 | 1K relative to 4K |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| mac | 512x512 | 1 | compare | 1.460 | 1.444 | +1.2% |
| mac | 512x512 | 1 | pair | 6.040 | 6.030 | +0.2% |
| mac | 512x512 | 6 | compare | 0.643 | 0.653 | -1.6% |
| mac | 512x512 | 6 | pair | 2.104 | 2.135 | -1.5% |
| mac | 512x512 | 12 | compare | 0.736 | 0.728 | +1.0% |
| mac | 512x512 | 12 | pair | 2.325 | 2.308 | +0.7% |
| mac | 3840x2160 | 1 | compare | 42.675 | 41.996 | +1.6% |
| mac | 3840x2160 | 1 | pair | 170.953 | 170.848 | +0.1% |
| mac | 3840x2160 | 6 | compare | 19.048 | 20.149 | -5.5% |
| mac | 3840x2160 | 6 | pair | 61.906 | 61.877 | +0.0% |
| mac | 3840x2160 | 12 | compare | 18.681 | 18.977 | -1.6% |
| mac | 3840x2160 | 12 | pair | 57.174 | 58.205 | -1.8% |
| linux | 512x512 | 1 | compare | 1.184 | 1.151 | +2.8% |
| linux | 512x512 | 1 | pair | 5.078 | 5.015 | +1.3% |
| linux | 512x512 | 6 | compare | 0.562 | 0.550 | +2.2% |
| linux | 512x512 | 6 | pair | 2.069 | 2.076 | -0.4% |
| linux | 3840x2160 | 1 | compare | 92.342 | 88.561 | +4.3% |
| linux | 3840x2160 | 1 | pair | 450.441 | 448.980 | +0.3% |
| linux | 3840x2160 | 6 | compare | 48.456 | 51.464 | -5.8% |
| linux | 3840x2160 | 6 | pair | 204.228 | 213.661 | -4.4% |

### Does chunk size explain the earlier API regression?

No clear evidence supports that explanation. Both APIs already used the same
4096-pixel chunk size. In these fresh builds, the old measured Mac 8% penalty
for construction options does not reproduce: the relative result reverses.
Six-worker 512x512 cached-candidate creation + comparison:

| Host | API | 1024 median (range) | 4096 median (range) |
| --- | --- | ---: | ---: |
| mac | context setter | 1.219 (1.107–1.225) | 1.221 (1.092–1.307) |
| mac | construction options | 1.169 (1.102–1.211) | 1.102 (1.086–1.110) |
| linux | context setter | 1.116 (1.111–1.116) | 1.124 (1.119–1.129) |
| linux | construction options | 1.125 (1.122–1.131) | 1.134 (1.118–1.147) |

The Mac results vary enough across process runs/builds that the earlier 8%
number should not be treated as an intrinsic cost of the API. This sweep does
not identify the source of that variation. It preserves the previous report's
observation rather than silently replacing it. The constructor implementation
still has one preparation phase and reuses its scratch storage.

## Validation and artifacts

- **684 accepted observations**: 408 Mac, 276 Linux. No observations were
  rejected for excess competing CPU in this sweep. None overlapped our builds
  or correctness probes on the same host.
- All timing scores match bit-for-bit across chunk sizes, lineages and worker
  counts within each host/image shape.
- All eight variants reproduce the complete #197-4096 output corpus byte-for-byte
  on each host: 220,370,288 bytes per variant plus matching scalar-score text.
  This covers odd/padded sizes, tiny images, linear RGB/RGBA, RGB8/RGBA8,
  grayscale, downsampling and full SSIM maps.
- Linux SHA-256: `99dcf2e16049bd1d0d9b5e14b1324286c3d9ea0bedc7f6f4a17fe7f2fddf4c98`.
  Mac: `fe3ca30264e9d16b2218236d464110c825b424cede2bd800eca2183a7825a8e2`.
  These establish same-host parity; #197's full corpus already differs across
  the two hosts. Binary dumps are not committed.
- `measurements/results.csv` contains every accepted process measurement and
  its seven raw batches. `summary.csv` contains medians/ranges over the three
  processes. `metadata` contains run/load records and host observations; process
  names are omitted from published load records. `variants.json` pins revisions
  and modified-file hashes; `variants/*.patch` shows the constant-only changes.

## Reproduce

Use an idle host and a fresh destination. Git must contain the three revisions
listed above. Adjust Linux CPU affinity in `run.py` for your machine.

```sh
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
python3 benchmarks/chunk-size/prepare.py /tmp/chunk-review
python3 /tmp/chunk-review/build.py
# Only after every build finishes:
python3 /tmp/chunk-review/run.py
# Only after timing finishes:
python3 /tmp/chunk-review/validate.py
python3 /tmp/chunk-review/summarize.py /tmp/chunk-review /tmp/chunk-summary
```

Run separately on Linux and Mac; the scripts detect the host. Clear any
additional target-specific compiler flags. `build.py` produces immutable
executable copies for each constant and restores the exported source file
afterward. Validation writes about 1.8 GB of binary dumps per host; delete
those after checking if desired. No production source change is proposed by
this experiment.
