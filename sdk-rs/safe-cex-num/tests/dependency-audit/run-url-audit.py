#!/usr/bin/env python3
"""Reproduce the historical dependency experiment without editing a live graph."""
import argparse, io, os, pathlib, subprocess, tarfile
parser = argparse.ArgumentParser()
parser.add_argument("profile", choices=["before", "rejected", "newer"])
args = parser.parse_args()
evidence = pathlib.Path(__file__).resolve().parent
root = evidence.parents[3]
scratch = root / "sdk-rs/target/num-url-audit" / args.profile
scratch.mkdir(parents=True, exist_ok=True)
archive = subprocess.check_output(["git", "archive", "88214fcc06a0bd59ca86301d247c9f953514c49b", "sdk-rs", "infra/tradetunnel"], cwd=root)
with tarfile.open(fileobj=io.BytesIO(archive)) as tree:
    tree.extractall(scratch, filter="data")
if args.profile == "rejected":
    subprocess.run(["git", "apply", "--unsafe-paths", "--directory=" + str(scratch), str(evidence / "proposed-icu.patch")], cwd=root, check=True)
elif args.profile == "newer":
    manifest = scratch / "sdk-rs/safe-cex-num/Cargo.toml"
    manifest.write_text(manifest.read_text().replace("[lints]", 'icu_collator = "=2.3.1"\n[lints]'))
(scratch / "sdk-rs/safe-cex-transport/tests/icu_url_audit.rs").write_bytes((evidence / "url-audit.rs.txt").read_bytes())
env = dict(os.environ)
env["CARGO_TARGET_DIR"] = str(root / "sdk-rs/target/num-url-audit-build")
subprocess.run(["cargo", "test", "--manifest-path", str(scratch / "sdk-rs/Cargo.toml"), "-p", "safe-cex-transport", "--test", "icu_url_audit", "--jobs", "2", "--", "--nocapture"], cwd=root, env=env, check=True)
