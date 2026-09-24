Mac native controls and SSIM ablation

Sources: upstream `0e44c9b` and PR `eb41ae3`, unchanged from the current-main report. Same Rust 1.90 harness, dependency lock, release profile, Apple M4 Pro and 2049×1024 inputs. These follow-up runs include generic and `RUSTFLAGS='-C target-cpu=native'` builds of BOTH revisions, two rounds with reversed variant order, at one and six Rayon workers. Build native binaries in separate target directories; do not overwrite the generic binaries. Raw CSVs: `results/mac-nativecheck-*.csv`.

The ablation uses PR sources except that `dssim-core/src/dssim.rs` is restored verbatim from upstream `0e44c9b`. This restores the original SSIM implementation while keeping all other PR kernel changes. Reproduce in a separate worktree with `git show 0e44c9b:dssim-core/src/dssim.rs > dssim-core/src/dssim.rs`, then build the same harness against that worktree. Ablation measurements use generic compilation and follow the four variants in each round; this is a coarse causal check, not a precise decomposition of small effects.

Times in milliseconds, range of two batch medians; lower is better. These hosts are not isolated, so small differences should not be treated as established wins.

| Operation / workers | Main generic | Main native | PR generic | PR native | PR with upstream SSIM |
|---|---:|---:|---:|---:|---:|
| prepare_pair_compare / 1 | 66.56–67.39 | 65.75–67.85 | 38.31–38.51 | 38.07–38.72 | 64.79–65.85 |
| prepare_pair_compare / 6 | 18.08–18.82 | 18.51–18.94 | 13.14–13.24 | 13.30–13.30 | 18.03–18.06 |
| compare / 1 | 36.85–40.11 | 37.11–38.28 | 11.11–11.40 | 11.00–11.09 | 37.50–37.81 |
| compare / 6 | 9.47–9.48 | 9.62–9.68 | 5.01–5.15 | 4.80–5.24 | 9.48–9.72 |

Native compilation has no clear whole-pipeline benefit on this Mac for either revision. Restoring upstream SSIM removes almost all the PR's whole-pipeline improvement. The shared loop refactoring, rather than runtime CPU dispatch, is responsible for the Mac gain: x86 feature wrappers are not compiled on AArch64.

Disassembly from `otool -tvV` explains the main effect. Follow the call from `dssim_core::dssim::Dssim::compare_scale_3ch` into `rayon::iter::plumbing::bridge_producer_consumer::helper` (hash `h3c57852595804fa5` in this build). In BOTH upstream generic and native binaries, its hot loop calls `core::ops::function::impls::...::FnMut::call_mut` (hash `h9a9f33b8eccc5dae`) for each pixel, stores one `s0` result, and increments the index by one. That callback performs scalar `fmadd` and `fdiv`, with input bounds checks per call.

In the generic PR binary, `dssim_core::dssim::ssim3_range_base` has a four-pixel NEON loop: `fmul.4s`, `fadd.4s`, `fmla.4s`, `fdiv.4s`, a `str q4`, and a 16-byte advance. The scalar arithmetic is unchanged; independent pixels occupy the vector lanes. Extraction into an explicit output-slice loop and inline-always pixel body exposes the loop to LLVM. Bounds and alias checks remain around the vectorized loop, with a scalar tail. There are no manual NEON intrinsics or runtime ARM feature checks.

Therefore, “dispatch-only” describes the scope relative to the larger experimental stack, but it is not a pure feature-flag toggle: the necessary shared-loop extraction also changes inlining, allocation/collection shape and chunking. The Mac result must not be attributed to AVX-512 or additional ARM instruction-set features. Remaining small preparation changes are not individually isolated by this ablation.
