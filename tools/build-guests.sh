#!/bin/sh
# Build the freestanding guest programs and copy the ELF images into fixtures/.
#
#   tools/build-guests.sh
#
# Needs the riscv32im-unknown-none-elf target:
#
#   rustup target add riscv32im-unknown-none-elf
#
# The images are committed, so this is only needed when a guest changes. CI runs
# the committed images rather than rebuilding, because a different rustc emits
# different instruction selections and the point of the fixtures is that they are
# a fixed, known workload.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root/guests"

cargo build --release

out="$root/fixtures"
mkdir -p "$out"

built=0
for bin in arith muldiv memops sort control; do
  src="target/riscv32im-unknown-none-elf/release/$bin"
  if [ ! -f "$src" ]; then
    echo "missing $src" >&2
    exit 1
  fi
  cp "$src" "$out/$bin.elf"
  built=$((built + 1))
  printf '%-10s %8s bytes\n' "$bin.elf" "$(wc -c < "$out/$bin.elf")"
done

echo "$built guest images written to fixtures/"
