#!/bin/sh
# Run every example/*/build.sh and report the failures. Local convenience for the
# example lane (CI discovers the same glob).
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 1
fails=0
for d in example/*/; do
  [ -f "$d/build.sh" ] || continue
  if out=$(sh "$d/build.sh" 2>&1); then
    echo "ok    $d"
  else
    echo "FAIL  $d"
    echo "$out" | tail -20
    fails=$((fails + 1))
  fi
done
echo "examples: $fails failure(s)"
exit $((fails == 0 ? 0 : 1))
