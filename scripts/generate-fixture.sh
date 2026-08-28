#!/usr/bin/env bash
set -euo pipefail

root="${1:-target/fixtures}"
mkdir -p "$root"

for count in 1000 10000 100000; do
  directory="$root/${count}"
  mkdir -p "$directory"
  echo "Generating $count deterministic entries in $directory"
  for ((index = 0; index < count; index++)); do
    printf -v name 'entry-%06d.txt' "$index"
    : > "$directory/$name"
  done
done
