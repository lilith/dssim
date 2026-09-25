#!/bin/sh
set -eu
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
for version in main dispatch fusion options; do
  extra=""
  if [ "$version" = options ]; then extra="--features options"; fi
  cargo +1.90.0 build --release --locked --manifest-path "$version/Cargo.toml" --bins $extra > "$version-build.log" 2>&1
  echo "Built $version"
done
