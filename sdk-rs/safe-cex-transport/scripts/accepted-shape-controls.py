#!/usr/bin/env python3
"""Independently disconnect each Transport map/string admission guard, then restore."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / "sdk-rs"
PACKAGE = SDK / "safe-cex-transport"
FRAMES = ROOT / "infra/tradetunnel/src/frames.rs"
CASES = []


def add(path, name, old, new, target, test, assertion):
    CASES.append((path, name, old, new, target, test, assertion))


for owner, test in [
    ("LatencyEntry", "latency_entry_requires_an_object_and_preserves_native_fields"),
    ("ResolvedWs", "resolved_ws_requires_an_object_and_preserves_native_fields"),
    ("ResolvedRouting", "resolved_routing_requires_an_object_and_preserves_native_fields"),
    ("RoutingOptions", "routing_options_require_an_object_and_preserve_native_fields"),
    ("SigningHeader", "signing::header_shape_tests::header_requires_object_and_preserves_native_fields"),
    ("ResolvedTransport", "resolution_transport_requires_object_and_preserves_optional_flags"),
    ("TransportMetadata", "resolution_metadata_requires_object_and_preserves_native_region_order"),
]:
    source = "transport_resolution.rs" if owner in {"ResolvedTransport", "TransportMetadata"} else "how_config.rs" if owner == "LatencyEntry" else "signing.rs" if owner == "SigningHeader" else "routing.rs"
    add(PACKAGE / "src" / source, owner,
        f"{owner}Wire::deserialize(safe_cex_types::strict_decode::object(d))",
        f"{owner}Wire::deserialize(d)",
        ["--lib"] if owner == "SigningHeader" else ["--test", "accepted_wire_shapes"],
        test, "complete ordered array must be refused")

for owner, source, test in [
    ("HOWRegion", "how_config.rs", "how_region_requires_strings_and_preserves_native_variants"),
    ("HowEgress", "routing.rs", "how_egress_requires_strings_and_preserves_native_variants"),
]:
    add(PACKAGE / "src" / source, owner,
        f"let value = String::deserialize(d)?;\n        {owner}Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))",
        f"{owner}Wire::deserialize(d)", ["--test", "accepted_wire_shapes"], test,
        "valid tagged map must be refused")

for owner, test in [
    ("HttpRequestArgs", "http_request_requires_object_and_preserves_native_fields"),
    ("RpcHttpResponse", "rpc_response_requires_object_and_preserves_native_fields"),
    ("ResponseData", "response_data_requires_object_and_preserves_native_fields"),
    ("Config", "config_requires_object_and_preserves_native_url"),
    ("RequestDump", "request_dump_requires_object_and_preserves_native_fields"),
]:
    visitor = f"{owner}MapVisitor"
    wire = f"{owner}Wire"
    if owner == "HttpRequestArgs":
        visitor += "::<D>(std::marker::PhantomData)"
        wire += "::<D>"
    # Retain a real construction so the disconnected visitor does not trigger
    # dead_code; the only semantic change is bypassing admission through visit_map.
    add(FRAMES, owner, f"d.deserialize_map({visitor})",
        f"let _visitor = {visitor};\n        {wire}::deserialize(d)",
        ["--lib"], f"accepted_frame_shapes::{test}",
        "complete ordered array must be refused")


def execute(case, restored):
    _, name, _, _, target, test, assertion = case
    result = subprocess.run(
        ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-transport", *target, test, "--", "--exact"],
        cwd=SDK, capture_output=True, text=True,
    )
    output = result.stdout + result.stderr
    suffix = "restored" if restored else "disconnected"
    Path(f"/tmp/transport-shape-{name}-{suffix}.log").write_text(output)
    expected = 0 if restored else 101
    marker = f"test {test} ... {'ok' if restored else 'FAILED'}"
    if result.returncode != expected or marker not in output or "error[E" in output or "could not compile" in output or (not restored and assertion not in output):
        raise RuntimeError(f"{name} {suffix}: missing compiled named witness\n{output}")
    print(f"{name}: {suffix} compiled named test {'PASS' if restored else 'FAIL'}", flush=True)


if len(sys.argv) > 1:
    requested = set(sys.argv[1:])
    assert requested <= {case[1] for case in CASES}, requested
    CASES = [case for case in CASES if case[1] in requested]
snapshots = {case[0]: case[0].read_bytes() for case in CASES}
try:
    for case in CASES:
        path, name, old, new, *_ = case
        source = snapshots[path].decode()
        assert source.count(old) == 1, (name, source.count(old))
        try:
            path.write_text(source.replace(old, new))
            execute(case, False)
        finally:
            path.write_bytes(snapshots[path])
        execute(case, True)
finally:
    for path, original in snapshots.items():
        path.write_bytes(original)
assert all(path.read_bytes() == original for path, original in snapshots.items())
print(f"{len(CASES)} independent guards failed named assertions; exact byte restoration passed", flush=True)
