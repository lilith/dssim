set -eu
root=/tmp/dssim-pgo-evidence
profdata=$(rustc +1.90.0 --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata
for v in pr main; do
 flags='-C debuginfo=1'
 if [ "$v" = main ]; then flags="$flags -C target-cpu=native"; fi
 mkdir -p "$root/profiles/$v"
 RUSTFLAGS="$flags" CARGO_TARGET_DIR="$root/$v/target-base" cargo +1.90.0 build --manifest-path "$root/$v/Cargo.toml" --release --locked --offline --target x86_64-unknown-linux-gnu --bin review-bench > "$root/$v/base-build.log" 2>&1
 RUSTFLAGS="$flags -C profile-generate=$root/profiles/$v" CARGO_TARGET_DIR="$root/$v/target-generate" cargo +1.90.0 build --manifest-path "$root/$v/Cargo.toml" --release --locked --offline --target x86_64-unknown-linux-gnu --bin review-bench > "$root/$v/generate-build.log" 2>&1
 for t in 1 6; do
  RAYON_NUM_THREADS=$t MALLOC_MMAP_THRESHOLD_=67108864 MALLOC_TRIM_THRESHOLD_=2147483647 "$root/$v/target-generate/x86_64-unknown-linux-gnu/release/review-bench" --train
 done
 "$profdata" merge -o "$root/$v/merged.profdata" "$root/profiles/$v"
 "$profdata" show --all-functions --counts --topn=40 "$root/$v/merged.profdata" > "$root/$v/profile-summary.txt"
 RUSTFLAGS="$flags -C profile-use=$root/$v/merged.profdata -C llvm-args=-pgo-warn-missing-function" CARGO_TARGET_DIR="$root/$v/target-use" cargo +1.90.0 build --manifest-path "$root/$v/Cargo.toml" --release --locked --offline --target x86_64-unknown-linux-gnu --bin review-bench > "$root/$v/use-build.log" 2>&1
 echo "PGO built: $v"
done
