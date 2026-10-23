#!/usr/bin/env python3
"""Temporary, fully retained native diagnostic. Never a gate or merge input."""
import argparse
import ast
import datetime
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import select
import signal
import shutil
import subprocess
import sys
import traceback
import time

if not __debug__:
    raise RuntimeError("native diagnostic refuses optimized Python")

CONTRACT_SHA = "886e3485852a9a0768671cfdb25e17e69d3043f32f2d47832f17cf38784fc60c"
ORDER = ["landing", "before", "before", "landing", "landing", "current", "current", "landing"]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def validate_contract(contract):
    assert contract["schema"] == 1
    assert contract["kind"] == "temporary-native-socket-diagnostic"
    assert contract["protocol"] == {
        "order": ORDER, "durationSeconds": 40, "profileSeconds": 20,
        "frequencyHz": 999, "warmups": 3,
    }
    assert contract["toolchain"] == "1.99.0"
    assert [case["role"] for case in contract["cases"]] == ["landing", "before", "current"]
    assert len({case["head"] for case in contract["cases"]}) == 3
    for case in contract["cases"]:
        assert re.fullmatch("[0-9a-f]{40}", case["head"])
        assert re.fullmatch("[0-9a-f]{40}", case["tree"])
        assert re.fullmatch("[0-9a-f]{64}", case["workspaceLockSha256"])
    assert len({case["workspaceLockSha256"] for case in contract["cases"]}) == 1
    assert contract["facts"] == [
        "848829d3bd74fa0d6b76abba825d8e3cb38b5296e97efe6c679ace9026d0f755",
        28687, 89, 51834,
    ]


def facts(line):
    match = re.search(r"result=(\(.+\))$", line.strip())
    assert match, line
    value = ast.literal_eval(match.group(1))
    assert isinstance(value, tuple) and len(value) == 4
    return list(value)


def profile_presence(stacks, report, pid):
    """Validate capture presence, without selecting or interpreting performance."""
    samples = []
    for line in stacks.splitlines():
        if "cpu-clock:u:" in line:
            prefix, rest = line.split("cpu-clock:u:", 1)
            header = re.fullmatch(r"\s*(\d+)\s*(?:/|\s)\s*(\d+)\s+\d+(?:\.\d+)?:\s*", prefix)
            assert header and int(header[1]) == pid and int(header[2]) == pid, line
            samples.append([])
            line = rest
        if samples:
            frame = re.fullmatch(r"\s*([0-9a-fA-F]+)\s+(.+?)\s+\((.+)\)\s*", line)
            if frame:
                samples[-1].append((frame[2], frame[3]))
    assert samples, "perf captured no target samples"
    chains = sum(sum("[unknown]" not in symbol and "[unknown]" not in dso
                     for symbol, dso in frames) >= 2 for frames in samples)
    assert chains > 0, "perf captured no usable target callchains"
    rows = re.findall(r"^\s*\d+(?:\.\d+)?%\s+(\d+)\s+.+$", report, re.M)
    assert rows and sum(map(int, rows)) > 0, "perf self report contains no samples"
    return {"targetSamples": len(samples), "usableCallchains": chains,
            "selfReportSamples": sum(map(int, rows))}


def command(args, log, cwd=None, env=None):
    with log.open("wb") as output:
        completed = subprocess.run(args, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT)
    assert completed.returncode == 0, (args, completed.returncode, str(log))


def stop_owned_process(process):
    # Popen.kill polls first and can reap an exited child, losing wait4 rusage.
    waited, status, usage = os.wait4(process.pid, os.WNOHANG)
    if not waited:
        try:
            os.kill(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass  # The unreaped child may have exited between wait4 and kill.
        _, status, usage = os.wait4(process.pid, 0)
    process.returncode = os.waitstatus_to_exitcode(status)
    return usage


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def checked_source(root, case):
    assert git(root, "status", "--porcelain=v1") == "", "dirty diagnostic checkout"
    assert git(root, "rev-parse", "HEAD") == case["head"]
    assert git(root, "rev-parse", "HEAD^{tree}") == case["tree"]
    assert sha(root / "sdk-rs/Cargo.lock") == case["workspaceLockSha256"]


def run(root, inputs, out, perf):
    contract_path = inputs / "socket-native-profile.contract.json"
    assert sha(contract_path) == CONTRACT_SHA, "changed diagnostic contract"
    contract = json.loads(contract_path.read_text())
    validate_contract(contract)
    assert sha(inputs / "socket-native-profile-driver.rs") == contract["driverSha256"]
    assert sha(inputs / "socket-native-profile.Cargo.lock") == contract["lockSha256"]
    origin = git(root, "rev-parse", "HEAD")
    assert git(root, "status", "--porcelain=v1") == ""
    write_json(out / "experiment.json", {
        "kind": "NATIVE-LINUX-DIAGNOSTIC-NOT-WASM-GATE", "start": utc(),
        "diagnosticHead": origin, "contractSha256": CONTRACT_SHA,
        "scriptSha256": sha(Path(__file__)), "contract": contract,
        "github": {key: os.environ.get(key) for key in [
            "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_SHA", "GITHUB_JOB",
            "RUNNER_OS", "RUNNER_ARCH", "ImageOS", "ImageVersion",
        ]},
    })
    for name, args in {
        "uname": ["uname", "-a"], "cpu": ["lscpu"],
        "rustc": ["rustc", "+1.99.0", "--version", "--verbose"],
        "cargo": ["cargo", "+1.99.0", "--version"],
        "perf": [perf, "--version"], "memory": ["free", "-h"],
    }.items():
        command(args, out / (name + ".log"))
    command([perf, "stat", "-e", "cpu-clock:u", "--", "sleep", "0.1"], out / "perf-preflight.log")
    build = out / "driver"
    (build / "src").mkdir(parents=True)
    shutil.copyfile(inputs / "socket-native-profile-driver.rs", build / "src/main.rs")
    shutil.copyfile(inputs / "socket-native-profile.Cargo.lock", build / "Cargo.lock")
    manifest = '''[package]
name = "socket-native-profile"
version = "0.0.0"
edition = "2024"
[dependencies]
safe-cex-engine = { path = ENGINE_PATH, features = ["polling-control"] }
safe-cex-venue-bitunix = { path = VENUE_PATH }
serde_json = { version = "=1.0.152", features = ["float_roundtrip", "raw_value"] }
sha2 = "0.10"
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
debug = 1
[profile.release.package.safe-cex-venue-bitunix]
opt-level = "z"
'''.replace("ENGINE_PATH", json.dumps(str(root / "sdk-rs/engine"))).replace(
        "VENUE_PATH", json.dumps(str(root / "sdk-rs/venues/bitunix")))
    (build / "Cargo.toml").write_text(manifest)
    env = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_INCREMENTAL="0",
               RUSTFLAGS="-C force-frame-pointers=yes")
    binaries = {}
    for case in contract["cases"]:
        role = case["role"]
        command(["git", "checkout", "--detach", case["head"]], out / (role + "-checkout.log"), root)
        checked_source(root, case)
        tape_gz = root / "sdk-rs/harness/fixtures/bitunix-public.jsonl.gz"
        assert sha(tape_gz) == contract["gzipTapeSha256"]
        tape = out / "tape.jsonl"
        raw = gzip.decompress(tape_gz.read_bytes())
        assert hashlib.sha256(raw).hexdigest() == contract["rawTapeSha256"]
        if tape.exists():
            assert tape.read_bytes() == raw
        else:
            tape.write_bytes(raw)
        command(["cargo", "+1.99.0", "build", "--release", "--locked",
                 "--manifest-path", str(build / "Cargo.toml")], out / (role + "-build.log"), root, env)
        checked_source(root, case)
        assert sha(build / "Cargo.lock") == contract["lockSha256"]
        binary = out / (role + "-native")
        shutil.copy2(build / "target/release/socket-native-profile", binary)
        command(["readelf", "-n", str(binary)], out / (role + "-build-id.log"))
        command(["nm", "-n", str(binary)], out / (role + "-symbols.log"))
        binaries[role] = {"path": binary, "sha256": sha(binary)}
        write_json(out / (role + "-build.json"), {
            "source": case, "binarySha256": sha(binary),
            "manifestSha256": sha(build / "Cargo.toml"),
            "driverSha256": sha(build / "src/main.rs"), "lockSha256": sha(build / "Cargo.lock"),
            "environment": {key: env[key] for key in ["CARGO_BUILD_JOBS", "CARGO_INCREMENTAL", "RUSTFLAGS"]},
        })
    # No compilation, network or source mutation between these fixed samples.
    records = []
    for ordinal, role in enumerate(ORDER, 1):
        prefix = out / f"{ordinal}-{role}"
        binary = binaries[role]["path"]
        assert sha(binary) == binaries[role]["sha256"]
        record = {"ordinal": ordinal, "role": role, "binarySha256": sha(binary), "start": utc()}
        command(["ps", "-eo", "pid,comm,pcpu"], prefix.with_suffix(".system-before.log"))
        with prefix.with_suffix(".stderr.log").open("w") as errors:
            process = subprocess.Popen([str(binary), str(out / "tape.jsonl"), "40"],
                                       stdout=subprocess.PIPE, stderr=errors, text=True)
            try:
                assert select.select([process.stdout], [], [], 90)[0], "missing bounded READY"
                ready = process.stdout.readline()
                prefix.with_suffix(".ready.log").write_text(ready)
                assert ready.startswith("READY ") and facts(ready) == contract["facts"], ready
                record.update({"pid": process.pid, "ready": ready.strip()})
                write_json(prefix.with_suffix(".started.json"), record)
                data = prefix.with_suffix(".perf.data")
                command([perf, "record", "-e", "cpu-clock:u", "-F", "999", "--call-graph", "fp",
                         "-p", str(process.pid), "-o", str(data), "--", "sleep", "20"],
                        prefix.with_suffix(".perf-record.log"))
                wait_until = time.monotonic() + 90
                while True:
                    waited, status, usage = os.wait4(process.pid, os.WNOHANG)
                    if waited:
                        break
                    assert time.monotonic() < wait_until, "native process exceeded bounded wait"
                    time.sleep(0.05)
                process.returncode = os.waitstatus_to_exitcode(status)
                done = process.stdout.read()
                prefix.with_suffix(".done.log").write_text(done)
                record.update({"returncode": process.returncode, "cpuUserSeconds": usage.ru_utime,
                               "cpuSystemSeconds": usage.ru_stime, "maxRss": usage.ru_maxrss, "end": utc()})
                write_json(prefix.with_suffix(".exited.json"), record)
                assert process.returncode == 0 and facts(done) == contract["facts"], done
                match = re.search(r"DONE passes=(\d+) elapsed=", done)
                assert match and int(match[1]) > 0
                record.update({"done": done.strip(), "timedPasses": int(match[1]), "perfDataSha256": sha(data)})
                write_json(prefix.with_suffix(".record.json"), record)
                command([perf, "script", "-i", str(data), "-F", "pid,tid,time,event,ip,sym,dso"],
                        prefix.with_suffix(".stacks.txt"))
                command([perf, "report", "-i", str(data), "--stdio", "--no-children", "--percent-limit", "0",
                         "--sort", "symbol", "--show-nr-samples"], prefix.with_suffix(".self.txt"))
                record["capture"] = profile_presence(prefix.with_suffix(".stacks.txt").read_text(),
                                                     prefix.with_suffix(".self.txt").read_text(), process.pid)
                write_json(prefix.with_suffix(".record.json"), record)
                command(["ps", "-eo", "pid,comm,pcpu"], prefix.with_suffix(".system-after.log"))
                records.append(record)
                write_json(out / "completed-records.json", records)
                print(ordinal, role, done.strip(), "CPU", usage.ru_utime, usage.ru_stime, flush=True)
            finally:
                if process.returncode is None:
                    usage = stop_owned_process(process)
                    prefix.with_suffix(".partial-stdout.log").write_text(process.stdout.read())
                    write_json(prefix.with_suffix(".incomplete.json"), {
                        "record": record, "returncode": process.returncode,
                        "cpuUserSeconds": usage.ru_utime, "cpuSystemSeconds": usage.ru_stime,
                        "maxRss": usage.ru_maxrss, "end": utc(),
                    })
    assert [record["role"] for record in records] == ORDER
    write_json(out / "complete.json", {"end": utc(), "records": records,
               "qualification": "Native diagnostic only; complete cohort requires independent profile review."})
    command(["git", "checkout", "--detach", origin], out / "restore-diagnostic.log", root)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--inputs", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--perf", required=True)
    args = parser.parse_args()
    assert not args.out.exists(), "refuse to overwrite a diagnostic cohort"
    args.out.mkdir(parents=True)
    try:
        run(args.root.resolve(), args.inputs.resolve(), args.out.resolve(), args.perf)
    except BaseException:
        (args.out / "failure.txt").write_text(traceback.format_exc())
        raise
    finally:
        write_json(args.out / "hashes.json", {
            str(path.relative_to(args.out)): sha(path)
            for path in sorted(args.out.rglob("*"))
            if path.is_file() and "target" not in path.relative_to(args.out).parts
            and path.name != "hashes.json"
        })


if __name__ == "__main__":
    main()
