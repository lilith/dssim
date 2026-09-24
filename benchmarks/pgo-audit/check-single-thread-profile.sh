set -eu
root=/tmp/dssim-pgo-evidence
mkdir -p "$root/profiles-single"
LLVM_PROFILE_FILE="$root/profiles-single/pr-%m.profraw" RAYON_NUM_THREADS=1 MALLOC_MMAP_THRESHOLD_=67108864 MALLOC_TRIM_THRESHOLD_=2147483647 "$root/pr/target-generate/x86_64-unknown-linux-gnu/release/review-bench" --train
profdata=$(rustc +1.90.0 --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata
"$profdata" merge -o "$root/pr/single.profdata" "$root/profiles-single"
RUSTFLAGS="-C debuginfo=1 -C profile-use=$root/pr/single.profdata -C llvm-args=-pgo-warn-missing-function" CARGO_TARGET_DIR="$root/pr/target-single" cargo +1.90.0 build --manifest-path "$root/pr/Cargo.toml" --release --locked --offline --target x86_64-unknown-linux-gnu --bin review-bench > "$root/pr/single-build.log" 2>&1
objdump -d -C "$root/pr/target-single/x86_64-unknown-linux-gnu/release/review-bench" > "$root/pr/single.asm"
