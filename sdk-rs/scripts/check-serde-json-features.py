#!/usr/bin/env python3
"""The restored candle-cache crates keep master's serde_json number features.

Those features change serde_json Number and map semantics, so the port's own
packages must never resolve them. The candle-cache crate must (positive control).
"""
import subprocess
import sys

MASTER_ONLY = ("arbitrary_precision", "preserve_order")
PORT = [
    ["-p", "safe-cex-engine", "--all-features"],
    ["-p", "safe-cex-crypto", "--all-features"],
    ["-p", "safe-cex-native"],
    ["-p", "safe-cex-wasm"],
]


def features(args):
    out = subprocess.run(
        ["cargo", "tree", "--locked", *args, "-e", "features", "-i", "serde_json"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return {feature for feature in MASTER_ONLY if f'feature "{feature}"' in out}


def main():
    failed = False
    for args in PORT:
        leaked = features(args)
        if leaked:
            print(f"BANNED: {' '.join(args)} resolves serde_json {sorted(leaked)}")
            failed = True
    control = features(["-p", "safe-cex-candle-cache"])
    if control != set(MASTER_ONLY):
        print(f"BANNED: candle-cache must keep master serde_json features, got {sorted(control)}")
        failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
