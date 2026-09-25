# End-to-end preparation, comparison, and caching

This is the primary end-to-end benchmark for PRs [#197](https://github.com/kornelski/dssim/pull/197)–[#200](https://github.com/kornelski/dssim/pull/200), measured on Linux Ryzen 9 7900X and macOS M4 Pro. All inputs are generated RGBA8 pixel buffers; codecs, file I/O, and input generation are excluded. Times include the work specified below, including destruction of images created inside the timed operation.

The two workloads have different timing boundaries. **Compare values within the same table.**

- **Fresh pair:** prepare both images, compare, drop both. “Streaming pair” caches neither image; “cached pair” builds both caches inside timing.
- **Reused cached reference:** prepare the reference once; then prepare, compare, and drop successive candidates. The table reports the one-time reference preparation separately, followed by the complete per-candidate cost. “Streaming candidate” caches only the reference. “Cached candidate / traditional” caches both, including each new candidate's preparation inside timing.

The harness rotates four distinct generated RGBA8 candidates, with equal numbers of each candidate in every batch. Candidate pixel generation is outside timing; every operation creates a new prepared candidate. One Dssim context lives outside timing, uses default scales, and does not request SSIM maps. Source pixel buffers stay live. Reference preparation is measured after thread-pool initialization and is not amortized over an assumed number of candidates: for N candidates, total work is the one-time reference preparation plus N per-candidate operations.

Preparation, comparison, and destruction phases are measured **inside the same operation** as the total. Each group's phase breakdown comes from the same process and batch as its median total, so it describes that observed lifecycle. The total also includes timer/loop bookkeeping. Comparison is shown only as a component, never as the headline cost. No prepared-pair-only benchmark is used.

## Fresh pair: both preparations + comparison + destruction, ms

| Host | Size | Workers | Upstream main | Fresh pair streaming | Fresh pair cached |
| --- | --- | ---: | ---: | ---: | ---: |
| linux | 512x512 | 1 | 52.43 | 5.29 | 9.30 |
| linux | 512x512 | 6 | 10.11 | 2.24 | 5.87 |
| linux | 3840x2160 | 1 | 1440.29 | 195.48 | 271.06 |
| linux | 3840x2160 | 6 | 462.61 | 79.72 | 194.23 |
| mac | 512x512 | 1 | 9.75 | 5.52 | 5.49 |
| mac | 512x512 | 6 | 2.86 | 1.58 | 1.61 |
| mac | 3840x2160 | 1 | 285.16 | 170.83 | 170.14 |
| mac | 3840x2160 | 6 | 84.83 | 45.54 | 50.18 |

## Reused reference: candidate preparation + comparison + destruction, ms

| Host | Size | Workers | Upstream main | Cached ref / streaming candidate | Cached both / traditional |
| --- | --- | ---: | ---: | ---: | ---: |
| linux | 512x512 | 1 | 23.91 | 4.26 | 7.97 |
| linux | 512x512 | 6 | 4.54 | 1.26 | 3.50 |
| linux | 3840x2160 | 1 | 897.47 | 129.89 | 246.31 |
| linux | 3840x2160 | 6 | 271.87 | 48.51 | 132.41 |
| mac | 512x512 | 1 | 7.42 | 3.53 | 3.34 |
| mac | 512x512 | 6 | 2.10 | 1.08 | 1.10 |
| mac | 3840x2160 | 1 | 215.81 | 108.85 | 106.13 |
| mac | 3840x2160 | 6 | 59.90 | 30.15 | 34.41 |


At 4K/six workers, fresh streaming pairs take 79.72 ms on Linux and 45.54 ms on Mac, versus 462.61 and 84.83 ms on upstream main. Those gains include the complete series, not just the cache policy. On Mac with one worker, caching each successive candidate is faster than streaming it (3.34 vs 3.53 ms at 512×512; 106.13 vs 108.85 ms at 4K). The compact default trades that cost for lower retained memory.

## Source revisions

| Label | Exact commit | Role |
| --- | --- | --- |
| Upstream main | `0e44c9b7fe91a5265c8e463436c28512186fe9cd` | `kornelski/main`, verified with `git ls-remote` before this run |
| #197 dispatch | `eb41ae307bfda358e41c022df2e2956fe5fd868c` | Runtime-dispatched autovectorized kernels |
| #198 fusion | `27a478073263aef6fad9b095c466dc77a1b85033` | Private gamma-input adapter; removes intermediate linear buffer |
| Streaming / cached policies | `331b56cf8998d89c3c89444880f8a0f51f046fb8` | Exact #200 head, including #197–#199; `cache_for_reuse` API |

Upstream, #197, and #198 always retain caches. The streaming/cached policy columns measure the **complete series**, not #199 or #200's isolated speedup. Attribution controls below keep both images cached while adding dispatch and input fusion. PR #199 is not separately timed in this matrix.

## Method

Rust 1.90.0, generic portable release builds, opt-level 3, fat LTO, 16 codegen units, panic=abort, identical dependency lockfiles. No native CPU targeting, PGO, or allocator tuning. Linux uses CPU 2 for one worker and CPUs 0–5 for six physical-core workers. Mac uses the normal OS scheduler. Both use the system allocator with its defaults. Score checks initialize Rayon before timing. Each process warms for 100 ms, then measures seven batches of at least 100 ms, each completing a multiple of four candidate operations. Tables show the median of three independent process medians; the appendix retains their ranges. Phase breakdowns come from that median process; one-time reference preparation is the median of the three separately recorded reference preparations. Group and variant order are shuffled with fixed seeds each round.

Competing CPU load is sampled every 250 ms. Runs wait for four samples below 60% of one core; observations exceeding 80% are rejected and retried. Linux uses `/proc` CPU-time deltas, Mac uses `ps` CPU percentages. This screening is not CPU isolation and does not eliminate allocator, scheduler, clock, thermal, or bandwidth variation. Raw rejected results are preserved and excluded from tables. Tiny differences and overlapping ranges do not establish speedups.

The default allocator is deliberate: earlier reports used glibc mmap/trim tuning. Earlier reports also mixed linear-input probes and superseded API revisions. Their timings remain valid for those configurations but should not be mixed with this matrix. This report replaces them as the primary end-to-end comparison; earlier native-build controls, full-output parity tests, and safety audits remain linked in the PRs.

## Preparation and full-operation breakdown, ms

Reference-once is a separately measured setup cost for the reused-reference workload. All other phases come from the same timed operation as the total. Add reference-once once per reference, not once per candidate.

| Host | Size | Workers | Workload / policy | Reference once | Reference prep per operation | Candidate prep | Compare | Drop | Total per operation |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| linux | 512x512 | 1 | pair: Upstream main | 0.000 | 22.781 | 23.725 | 4.888 | 1.032 | 52.426 |
| linux | 512x512 | 1 | pair: Fresh pair streaming | 0.000 | 1.054 | 1.377 | 2.773 | 0.082 | 5.286 |
| linux | 512x512 | 1 | pair: Fresh pair cached | 0.000 | 1.923 | 4.469 | 2.486 | 0.424 | 9.302 |
| linux | 512x512 | 1 | candidate: Upstream main | 21.140 | 0.000 | 18.372 | 5.036 | 0.505 | 23.914 |
| linux | 512x512 | 1 | candidate: Cached ref / streaming candidate | 3.347 | 0.000 | 2.048 | 2.055 | 0.159 | 4.263 |
| linux | 512x512 | 1 | candidate: Cached both / traditional | 3.426 | 0.000 | 4.978 | 2.493 | 0.502 | 7.973 |
| linux | 512x512 | 6 | pair: Upstream main | 0.000 | 4.038 | 4.371 | 1.173 | 0.524 | 10.107 |
| linux | 512x512 | 6 | pair: Fresh pair streaming | 0.000 | 0.557 | 0.590 | 0.758 | 0.333 | 2.238 |
| linux | 512x512 | 6 | pair: Fresh pair cached | 0.000 | 1.409 | 1.627 | 1.366 | 1.469 | 5.870 |
| linux | 512x512 | 6 | candidate: Upstream main | 4.235 | 0.000 | 3.531 | 1.006 | 0.003 | 4.539 |
| linux | 512x512 | 6 | candidate: Cached ref / streaming candidate | 1.433 | 0.000 | 0.465 | 0.661 | 0.138 | 1.264 |
| linux | 512x512 | 6 | candidate: Cached both / traditional | 1.055 | 0.000 | 1.449 | 1.333 | 0.715 | 3.497 |
| linux | 3840x2160 | 1 | pair: Upstream main | 0.000 | 566.158 | 582.755 | 287.785 | 3.588 | 1440.286 |
| linux | 3840x2160 | 1 | pair: Fresh pair streaming | 0.000 | 40.128 | 79.383 | 73.298 | 2.668 | 195.477 |
| linux | 3840x2160 | 1 | pair: Fresh pair cached | 0.000 | 74.244 | 87.055 | 106.590 | 3.165 | 271.055 |
| linux | 3840x2160 | 1 | candidate: Upstream main | 572.199 | 0.000 | 588.286 | 305.410 | 3.772 | 897.468 |
| linux | 3840x2160 | 1 | candidate: Cached ref / streaming candidate | 107.925 | 0.000 | 63.092 | 62.716 | 4.085 | 129.893 |
| linux | 3840x2160 | 1 | candidate: Cached both / traditional | 76.169 | 0.000 | 126.038 | 108.526 | 11.745 | 246.309 |
| linux | 3840x2160 | 6 | pair: Upstream main | 0.000 | 179.782 | 188.647 | 66.841 | 27.344 | 462.614 |
| linux | 3840x2160 | 6 | pair: Fresh pair streaming | 0.000 | 19.705 | 26.061 | 24.428 | 9.523 | 79.718 |
| linux | 3840x2160 | 6 | pair: Fresh pair cached | 0.000 | 53.325 | 58.013 | 56.470 | 26.422 | 194.230 |
| linux | 3840x2160 | 6 | candidate: Upstream main | 181.095 | 0.000 | 181.100 | 74.149 | 16.622 | 271.872 |
| linux | 3840x2160 | 6 | candidate: Cached ref / streaming candidate | 57.323 | 0.000 | 19.622 | 25.499 | 3.387 | 48.508 |
| linux | 3840x2160 | 6 | candidate: Cached both / traditional | 59.284 | 0.000 | 59.693 | 57.714 | 15.002 | 132.410 |
| mac | 512x512 | 1 | pair: Upstream main | 0.000 | 2.297 | 2.061 | 5.383 | 0.006 | 9.746 |
| mac | 512x512 | 1 | pair: Fresh pair streaming | 0.000 | 1.181 | 1.181 | 3.160 | 0.001 | 5.524 |
| mac | 512x512 | 1 | pair: Fresh pair cached | 0.000 | 2.058 | 1.948 | 1.480 | 0.005 | 5.491 |
| mac | 512x512 | 1 | candidate: Upstream main | 2.077 | 0.000 | 2.051 | 5.365 | 0.002 | 7.419 |
| mac | 512x512 | 1 | candidate: Cached ref / streaming candidate | 1.893 | 0.000 | 1.185 | 2.340 | 0.001 | 3.526 |
| mac | 512x512 | 1 | candidate: Cached both / traditional | 1.936 | 0.000 | 1.891 | 1.447 | 0.002 | 3.340 |
| mac | 512x512 | 6 | pair: Upstream main | 0.000 | 0.728 | 0.734 | 1.392 | 0.009 | 2.862 |
| mac | 512x512 | 6 | pair: Fresh pair streaming | 0.000 | 0.329 | 0.330 | 0.917 | 0.004 | 1.581 |
| mac | 512x512 | 6 | pair: Fresh pair cached | 0.000 | 0.447 | 0.456 | 0.695 | 0.009 | 1.607 |
| mac | 512x512 | 6 | candidate: Upstream main | 0.726 | 0.000 | 0.734 | 1.360 | 0.005 | 2.099 |
| mac | 512x512 | 6 | candidate: Cached ref / streaming candidate | 0.424 | 0.000 | 0.334 | 0.748 | 0.002 | 1.085 |
| mac | 512x512 | 6 | candidate: Cached both / traditional | 0.445 | 0.000 | 0.440 | 0.655 | 0.005 | 1.101 |
| mac | 3840x2160 | 1 | pair: Upstream main | 0.000 | 69.175 | 68.563 | 147.405 | 0.014 | 285.156 |
| mac | 3840x2160 | 1 | pair: Fresh pair streaming | 0.000 | 38.265 | 38.458 | 94.098 | 0.006 | 170.827 |
| mac | 3840x2160 | 1 | pair: Fresh pair cached | 0.000 | 63.401 | 63.185 | 43.543 | 0.015 | 170.145 |
| mac | 3840x2160 | 1 | candidate: Upstream main | 68.376 | 0.000 | 69.000 | 146.798 | 0.008 | 215.806 |
| mac | 3840x2160 | 1 | candidate: Cached ref / streaming candidate | 64.620 | 0.000 | 38.409 | 70.435 | 0.004 | 108.848 |
| mac | 3840x2160 | 1 | candidate: Cached both / traditional | 64.667 | 0.000 | 63.247 | 42.877 | 0.009 | 106.133 |
| mac | 3840x2160 | 6 | pair: Upstream main | 0.000 | 22.924 | 23.808 | 38.083 | 0.014 | 84.829 |
| mac | 3840x2160 | 6 | pair: Fresh pair streaming | 0.000 | 8.980 | 9.208 | 27.343 | 0.007 | 45.538 |
| mac | 3840x2160 | 6 | pair: Fresh pair cached | 0.000 | 15.614 | 15.108 | 19.439 | 0.015 | 50.176 |
| mac | 3840x2160 | 6 | candidate: Upstream main | 24.226 | 0.000 | 23.914 | 35.977 | 0.009 | 59.899 |
| mac | 3840x2160 | 6 | candidate: Cached ref / streaming candidate | 14.864 | 0.000 | 8.747 | 21.397 | 0.004 | 30.148 |
| mac | 3840x2160 | 6 | candidate: Cached both / traditional | 15.742 | 0.000 | 15.185 | 19.215 | 0.011 | 34.412 |

## Attribution controls: fresh pair, caching both, ms

| Host | Size | Workers | Upstream main | #197 dispatch | #198 fusion | #200 cached |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| linux | 512x512 | 1 | 52.43 | 12.75 | 12.62 | 9.30 |
| linux | 512x512 | 6 | 10.11 | 4.95 | 5.63 | 5.87 |
| linux | 3840x2160 | 1 | 1440.29 | 365.55 | 276.18 | 271.06 |
| linux | 3840x2160 | 6 | 462.61 | 302.41 | 196.26 | 194.23 |
| mac | 512x512 | 1 | 9.75 | 5.24 | 5.25 | 5.49 |
| mac | 512x512 | 6 | 2.86 | 2.14 | 1.59 | 1.61 |
| mac | 3840x2160 | 1 | 285.16 | 170.20 | 168.58 | 170.14 |
| mac | 3840x2160 | 6 | 84.83 | 63.27 | 50.24 | 50.18 |

## Across-process ranges

Each cell is median (minimum–maximum) of three process medians, ms.

| Host | Size | Workers | Workload | Revision / policy | Time |
| --- | --- | ---: | --- | --- | ---: |
| linux | 3840x2160 | 1 | candidate | dispatch / cc | 254.892 (253.803–255.524) |
| linux | 3840x2160 | 1 | candidate | main / cc | 897.468 (894.991–900.537) |
| linux | 3840x2160 | 1 | candidate | options / cc | 246.309 (246.087–246.429) |
| linux | 3840x2160 | 1 | candidate | options / cs | 129.893 (129.851–132.866) |
| linux | 3840x2160 | 1 | pair | dispatch / cc | 365.546 (364.484–375.794) |
| linux | 3840x2160 | 1 | pair | fusion / cc | 276.185 (275.543–277.177) |
| linux | 3840x2160 | 1 | pair | main / cc | 1440.286 (1433.803–1442.238) |
| linux | 3840x2160 | 1 | pair | options / cc | 271.055 (270.726–271.092) |
| linux | 3840x2160 | 1 | pair | options / ss | 195.477 (190.929–195.820) |
| linux | 3840x2160 | 6 | candidate | dispatch / cc | 186.213 (183.871–194.100) |
| linux | 3840x2160 | 6 | candidate | main / cc | 271.872 (268.367–281.870) |
| linux | 3840x2160 | 6 | candidate | options / cc | 132.410 (129.764–132.728) |
| linux | 3840x2160 | 6 | candidate | options / cs | 48.508 (44.978–57.910) |
| linux | 3840x2160 | 6 | pair | dispatch / cc | 302.412 (276.248–308.379) |
| linux | 3840x2160 | 6 | pair | fusion / cc | 196.261 (194.129–196.264) |
| linux | 3840x2160 | 6 | pair | main / cc | 462.614 (461.744–475.681) |
| linux | 3840x2160 | 6 | pair | options / cc | 194.230 (186.829–208.555) |
| linux | 3840x2160 | 6 | pair | options / ss | 79.718 (79.367–83.288) |
| linux | 512x512 | 1 | candidate | dispatch / cc | 6.954 (6.938–7.051) |
| linux | 512x512 | 1 | candidate | main / cc | 23.914 (23.742–28.208) |
| linux | 512x512 | 1 | candidate | options / cc | 7.973 (7.792–7.975) |
| linux | 512x512 | 1 | candidate | options / cs | 4.263 (4.254–4.269) |
| linux | 512x512 | 1 | pair | dispatch / cc | 12.751 (12.656–12.770) |
| linux | 512x512 | 1 | pair | fusion / cc | 12.618 (12.538–12.678) |
| linux | 512x512 | 1 | pair | main / cc | 52.426 (43.850–53.478) |
| linux | 512x512 | 1 | pair | options / cc | 9.302 (9.231–9.315) |
| linux | 512x512 | 1 | pair | options / ss | 5.286 (5.268–5.313) |
| linux | 512x512 | 6 | candidate | dispatch / cc | 1.295 (1.277–1.298) |
| linux | 512x512 | 6 | candidate | main / cc | 4.539 (4.535–4.628) |
| linux | 512x512 | 6 | candidate | options / cc | 3.497 (3.360–3.515) |
| linux | 512x512 | 6 | candidate | options / cs | 1.264 (1.251–1.347) |
| linux | 512x512 | 6 | pair | dispatch / cc | 4.954 (2.349–5.043) |
| linux | 512x512 | 6 | pair | fusion / cc | 5.628 (5.456–6.042) |
| linux | 512x512 | 6 | pair | main / cc | 10.107 (8.939–10.496) |
| linux | 512x512 | 6 | pair | options / cc | 5.870 (5.483–5.942) |
| linux | 512x512 | 6 | pair | options / ss | 2.238 (2.170–2.257) |
| mac | 3840x2160 | 1 | candidate | dispatch / cc | 105.535 (105.266–105.765) |
| mac | 3840x2160 | 1 | candidate | main / cc | 215.806 (215.534–216.024) |
| mac | 3840x2160 | 1 | candidate | options / cc | 106.133 (105.966–106.197) |
| mac | 3840x2160 | 1 | candidate | options / cs | 108.848 (108.817–108.924) |
| mac | 3840x2160 | 1 | pair | dispatch / cc | 170.199 (169.022–170.378) |
| mac | 3840x2160 | 1 | pair | fusion / cc | 168.579 (168.482–169.931) |
| mac | 3840x2160 | 1 | pair | main / cc | 285.156 (285.044–285.579) |
| mac | 3840x2160 | 1 | pair | options / cc | 170.145 (169.672–170.189) |
| mac | 3840x2160 | 1 | pair | options / ss | 170.827 (170.792–170.992) |
| mac | 3840x2160 | 6 | candidate | dispatch / cc | 40.706 (39.987–41.129) |
| mac | 3840x2160 | 6 | candidate | main / cc | 59.899 (58.993–60.372) |
| mac | 3840x2160 | 6 | candidate | options / cc | 34.412 (34.297–34.415) |
| mac | 3840x2160 | 6 | candidate | options / cs | 30.148 (30.143–31.076) |
| mac | 3840x2160 | 6 | pair | dispatch / cc | 63.269 (60.994–63.733) |
| mac | 3840x2160 | 6 | pair | fusion / cc | 50.243 (49.385–50.964) |
| mac | 3840x2160 | 6 | pair | main / cc | 84.829 (82.623–86.341) |
| mac | 3840x2160 | 6 | pair | options / cc | 50.176 (49.803–50.335) |
| mac | 3840x2160 | 6 | pair | options / ss | 45.538 (44.719–45.948) |
| mac | 512x512 | 1 | candidate | dispatch / cc | 3.339 (3.336–3.503) |
| mac | 512x512 | 1 | candidate | main / cc | 7.419 (7.387–7.428) |
| mac | 512x512 | 1 | candidate | options / cc | 3.340 (3.337–3.433) |
| mac | 512x512 | 1 | candidate | options / cs | 3.526 (3.526–3.530) |
| mac | 512x512 | 1 | pair | dispatch / cc | 5.245 (5.240–5.263) |
| mac | 512x512 | 1 | pair | fusion / cc | 5.250 (5.246–5.257) |
| mac | 512x512 | 1 | pair | main / cc | 9.746 (9.514–10.181) |
| mac | 512x512 | 1 | pair | options / cc | 5.491 (5.229–6.033) |
| mac | 512x512 | 1 | pair | options / ss | 5.524 (5.523–5.533) |
| mac | 512x512 | 6 | candidate | dispatch / cc | 1.371 (1.363–1.391) |
| mac | 512x512 | 6 | candidate | main / cc | 2.099 (2.054–2.100) |
| mac | 512x512 | 6 | candidate | options / cc | 1.101 (1.097–1.162) |
| mac | 512x512 | 6 | candidate | options / cs | 1.085 (1.076–1.085) |
| mac | 512x512 | 6 | pair | dispatch / cc | 2.141 (2.130–2.147) |
| mac | 512x512 | 6 | pair | fusion / cc | 1.592 (1.583–1.592) |
| mac | 512x512 | 6 | pair | main / cc | 2.862 (2.845–2.902) |
| mac | 512x512 | 6 | pair | options / cc | 1.607 (1.589–1.611) |
| mac | 512x512 | 6 | pair | options / ss | 1.581 (1.579–1.599) |

## Timing validation

216 accepted observations; all seven batches and their phases are in `results.csv`. The four candidate score bit patterns agree across every revision, policy, worker count, and workload within each host/size:

- linux 3840x2160: `3fb3e9db342ea340;3fb4769ccb400040;3fb50e77f35a2ba0;3fb59b28305d0e40`
- linux 512x512: `3fb58062a905a380;3fb620044d9ce440;3fb66d98bd446c70;3fb7046468de4740`
- mac 3840x2160: `3fb3e9db32af9790;3fb4769caa7ad030;3fb50e77faf08dd0;3fb59b28529fab00`
- mac 512x512: `3fb58063054aaa10;3fb6200361cd8a50;3fb66d98ebb40e90;3fb7046397b8ecd0`
- linux: 108 accepted, 0 rejected for competing CPU load.
- mac: 108 accepted, 7 rejected for competing CPU load.


## Memory measurements

Separate fresh processes use a counting allocator forwarding to `System`; it is not used by timing binaries. Counts are requested heap bytes, excluding allocator metadata and thread stacks. The allocation probe uses one RGBA8 reference and one candidate, both kept live. “Prepared pair” and “preparation peak” exclude the source-input baseline; “comparison scratch” is additional to the retained prepared pair. One worker is used to make allocation lifetimes easier to interpret.

| Host | Size | Revision / policy | Prepared pair | Preparation peak | Comparison scratch |
| --- | --- | --- | ---: | ---: | ---: |
| linux | 512x512 | main | 24.00 | 28.50 | 4.00 |
| linux | 512x512 | #197 | 24.00 | 28.50 | 4.00 |
| linux | 512x512 | #198 | 24.00 | 24.50 | 4.00 |
| linux | 512x512 | Streaming | 8.00 | 9.00 | 1.18 |
| linux | 512x512 | Reference cached | 16.00 | 17.00 | 1.18 |
| linux | 512x512 | Both cached | 24.00 | 24.50 | 4.00 |
| linux | 3840x2160 | main | 759.19 | 901.67 | 126.56 |
| linux | 3840x2160 | #197 | 759.19 | 901.67 | 126.56 |
| linux | 3840x2160 | #198 | 759.19 | 775.10 | 126.56 |
| linux | 3840x2160 | Streaming | 253.07 | 284.74 | 32.96 |
| linux | 3840x2160 | Reference cached | 506.13 | 537.80 | 32.96 |
| linux | 3840x2160 | Both cached | 759.19 | 775.10 | 126.56 |
| mac | 512x512 | main | 24.00 | 28.50 | 4.00 |
| mac | 512x512 | #197 | 24.00 | 28.50 | 4.00 |
| mac | 512x512 | #198 | 24.00 | 24.50 | 4.00 |
| mac | 512x512 | Streaming | 8.00 | 9.00 | 1.18 |
| mac | 512x512 | Reference cached | 16.00 | 17.00 | 1.18 |
| mac | 512x512 | Both cached | 24.00 | 24.50 | 4.00 |
| mac | 3840x2160 | main | 759.19 | 901.67 | 126.56 |
| mac | 3840x2160 | #197 | 759.19 | 901.67 | 126.56 |
| mac | 3840x2160 | #198 | 759.19 | 775.10 | 126.56 |
| mac | 3840x2160 | Streaming | 253.07 | 284.74 | 32.96 |
| mac | 3840x2160 | Reference cached | 506.13 | 537.80 | 32.96 |
| mac | 3840x2160 | Both cached | 759.19 | 775.10 | 126.56 |


## Reproduction

Run from this benchmark branch, which contains the pinned commits. Use a fresh directory on each host and run builds before benchmarks, not concurrently with them.

```sh
python3 benchmarks/e2e/prepare.py /tmp/dssim-e2e-run
sh /tmp/dssim-e2e-run/build.sh
python3 /tmp/dssim-e2e-run/run.py
python3 benchmarks/e2e/memory-run.py /tmp/dssim-e2e-run
# Copy results-linux and results-mac under one directory, then:
python3 benchmarks/e2e/summarize.py /path/to/combined-results
```

The CLI arguments to the shared harness are `width height cc|cs|ss pair|candidate`. `cc` caches both, `cs` caches the reference, and `ss` caches neither. Only the #200 binary accepts `cs`/`ss`; the other builds use identical harness source with the ordinary constructor. Existing caches are never cloned. Build logs, Rust version, source hashes, binary hashes, complete batch times, load records, and rejected samples accompany this report.

Earlier reports: [dispatch/native controls](https://github.com/lilith/dssim/tree/bench/autovec-dispatch-review/benchmarks/dispatch), [memory implementation audit](https://github.com/lilith/dssim/tree/bench/memory-197/benchmarks/memory), [superseded caching API experiments](https://github.com/lilith/dssim/tree/bench/memory-optin-197/benchmarks/memory-optin), and [chunk-size experiment](https://github.com/lilith/dssim/tree/bench/chunk-size-197/benchmarks/chunk-size). Production chunk sizes remain unchanged.
