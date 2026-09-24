PGO as a source-annotation audit, Rust 1.90

Outcome: no new inline/cold/never attributes promoted to the production PR. Two small source candidates compiled successfully but did not show a defensible repeatable improvement. Full PGO regressed the PR's one-worker comparison on both CPUs by losing SSIM vectorization. This is a pilot with a synthetic training corpus, not a claim that PGO cannot benefit dssim.

Exact sources: upstream `0e44c9b7fe91a5265c8e463436c28512186fe9cd`; PR `eb41ae307bfda358e41c022df2e2956fe5fd868c`. Experiments used isolated worktrees. Linux upstream is compiled with `-C target-cpu=native`; Linux PR is portable and dispatches at runtime. Both Mac revisions use generic AArch64 compilation. Within each comparison, the only PGO change is profile-generate/profile-use; all use Rust 1.90, LLVM 20.1.8, opt-level 3, fat LTO, 16 CGUs, panic=abort and debuginfo=1. These binaries include a training entry point and are different from the earlier publication harness: compare within this experiment, not across historical tables.

Training: `bench.rs --train`, three repetitions per size, 127×93, 512×511 and 1600×900; a different random seed from validation; RGB, premultiplied RGBA including zero/opaque/partial alpha, grayscale, saved maps on/off, and one/six Rayon workers. Separate profiles for each architecture and each source revision, using the toolchain's matching llvm-profdata. No missing-function diagnostics appeared in the original profile-use builds. Validation uses 256×256 and 2049×1024 and the previous six codec-free operations (nine batches of at least 60 ms). Two rounds reverse binary order. Linux affinity/allocator settings match the earlier report. Hardware perf sampling is unavailable on this host; instrumentation profiles and generated code were used.

Training is not a representative photo corpus or deployment workload: it uses synthetic data, only one x86 CPU tier, and a separate training entry point. It is suitable for finding hypotheses, not for shipping a production profile. Default multithread instrumentation counters may lose updates; a separate single-worker-only training control still produced scalar SSIM in the profile-use x86 binary. This rules out cross-worker counter loss as a necessary cause of that observed vectorization regression. Six-worker timing is noisy (including a large Mac outlier); the raw data is retained without removing it.

2049×1024 full preparation of two images plus comparison, milliseconds, range of two rounds' medians:

| Host / source / workers | Ordinary build | PGO build |
|---|---:|---:|
| linux / main / 1 | 97.24–112.13 | 111.59–112.82 |
| linux / main / 6 | 43.75–46.28 | 31.82–45.56 |
| linux / pr / 1 | 103.31–103.44 | 113.38–113.61 |
| linux / pr / 6 | 33.56–39.66 | 37.27–38.67 |
| mac / main / 1 | 72.23–74.03 | 72.28–72.41 |
| mac / main / 6 | 18.63–18.66 | 18.47–18.50 |
| mac / pr / 1 | 40.72–41.04 | 54.25–55.14 |
| mac / pr / 6 | 13.13–59.65 | 15.14–15.25 |

Why the PR's one-worker PGO result regressed: disassembly of the ordinary x86 `ssim3_range_avx512` contains ZMM arithmetic and `vdivps`; PGO (both mixed-worker and single-worker-only profiles) contains scalar `vdivss`, with no ZMM/YMM arithmetic in that function. The ordinary Mac kernel has `.4s` NEON arithmetic; its PGO version has scalar `fdiv`, no `.4s` arithmetic. Inline remarks confirm the shared SSIM body is still inlined into its wrapper. We have not isolated the exact LLVM transformation responsible; this is not evidence that another inline annotation will fix it.

Source-only candidates, tested WITHOUT PGO:

1. `inline-candidate.patch`: add `#[inline(always)]` to upstream's three-channel SSIM map closure, directly in the `.map(...)` argument (valid on Rust 1.90). The Mac hot Rayon loop still calls an out-of-line `FnMut::call_mut` wrapper. The annotation did not remove that remaining wrapper or recover the row-loop speedup. Compare-stage results below do not establish a win.
2. `caps-candidate.patch`: change both PR feature gates from `#[inline]` to `#[inline(always)]`. Inlining remarks report the existing AVX-512 gate as too costly (cost 500, threshold 325 at the inspected call sites). Forcing inlining removes call boundaries but expands this Linux harness's text from 605,701 to 608,989 bytes (+3,288) without a clear pipeline improvement. No arithmetic or dispatch logic is changed.

| Host / workers | Main compare | Main + closure hint compare | PR pipeline | PR + gate hints pipeline |
|---|---:|---:|---:|---:|
| linux / 1 | 21.11–21.63 | 21.73–21.80 | 102.97–103.13 | 102.94–103.67 |
| linux / 6 | 8.61–8.94 | 8.71–8.87 | 35.38–44.03 | 32.31–45.88 |
| mac / 1 | 40.85–41.53 | 41.08–42.02 | 41.12–41.24 | N/A (x86-only gates) |
| mac / 6 | 9.79–9.88 | 9.62–9.92 | 13.04–13.23 | N/A (x86-only gates) |

No candidate was promoted, so no production-source changes or new correctness claims result from this pilot. Attribute candidates compiled on Rust 1.90; the closure candidate was measured on both architectures, the x86-only gate candidate on Linux. This pilot did not add blanket `inline(never)` or `cold` annotations. Existing panic paths already have cold handling. Feature fallbacks absent from a profile are not globally cold: they are hot on other CPUs. A separate cold first-use detection helper is a plausible later experiment, but it has not been implemented or shown to improve performance here.

To reproduce, create the exact main and PR worktrees at `/tmp/dssim-dispatch-baseline` and `/tmp/dssim-pgo-investigation`, then create `/tmp/dssim-pgo-evidence/main` and `/tmp/dssim-pgo-evidence/pr` from this harness, replacing the relative dssim-core dependency with the corresponding absolute worktree path. Start with empty evidence/profile directories; do not merge stale profiles. Install `llvm-tools-preview` for Rust 1.90. Run `build-pgo.sh` on Linux or `build-pgo-mac.sh` on Mac, then the corresponding `run-pgo-*.py`. The scripts intentionally pin the exact original directory layout.

For source candidates, apply the respective patches to separate worktrees `/tmp/dssim-inline-candidate` (main) and `/tmp/dssim-caps-candidate` (PR). Create analogous `inline` and `caps` harness directories. Build their `target-base` with the same flags and explicit target as their controls, then run `run-hints-*.py`. No PGO flags are used for these candidates. `check-single-thread-profile.sh` records the independent single-worker profile control; it performs a code-generation check, not another timing series.

Inspect generated code using `objdump -d -C` (Linux) or `otool -tvV` (Mac). Rust's `-C remark=inline` supplies the inlining diagnostics summarized in `selected-remarks.txt`. The [Rust PGO workflow](https://doc.rust-lang.org/rustc/profile-guided-optimization.html) describes the instrumentation/merge/use sequence; PGO informs code layout and other optimizations as well as inlining.
