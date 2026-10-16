"""Sequential compiling route-selection controls, restoring after each run."""
from pathlib import Path
import os
import subprocess
import sys

source = Path('sdk-rs/safe-cex-venue-woo/src/runtime_selector.rs')
original = source.read_text()
cases = [
    ('signed-zero', 'let mode = requested_mode.as_str();', 'let changed = serde_json::json!(0.0);\n    let requested_mode = if safe_cex_num::numeric_ingress::js_value_as_f64(requested_mode) == Some(0.0) { &changed } else { requested_mode };\n    let mode = requested_mode.as_str();', 'route native number bits markets.requestedMode'),
    ('finite-payload', 'let mode = requested_mode.as_str();', 'let changed = serde_json::json!(1.5);\n    let requested_mode = if safe_cex_num::numeric_ingress::js_value_as_f64(requested_mode) == Some(1.25) { &changed } else { requested_mode };\n    let mode = requested_mode.as_str();', 'route native number bits markets.requestedMode'),
    ('ownerless', 'if route.runtime_lifecycle == Some(WooV3MigrationRouteRuntimeLifecycle::Ownerless)', 'if false && route.runtime_lifecycle == Some(WooV3MigrationRouteRuntimeLifecycle::Ownerless)', 'route blocker identity'),
    ('already-v3', 'if route.runtime_lifecycle == Some(WooV3MigrationRouteRuntimeLifecycle::AlreadyV3)', 'if false && route.runtime_lifecycle == Some(WooV3MigrationRouteRuntimeLifecycle::AlreadyV3)', 'route native field'),
    ('closed-gate', 'if !gate_open {', 'if gate_open {', 'route blocker identity'),
    ('legacy-path', 'target("legacy", route.legacy_method, &route.legacy_path)', 'target("legacy", route.legacy_method, v3_path)', 'route native field markets.authoritative.path'),
    ('mode-first', 'if !matches!(mode, Some("legacy" | "shadow-v3" | "v3"))', 'if false && !matches!(mode, Some("legacy" | "shadow-v3" | "v3"))', 'route blocker identity'),
]
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
for name, before, after, assertion in cases:
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    assert original.count(before) == 1, name
    try:
        source.write_text(original.replace(before, after))
        run = subprocess.run(['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'runtime_selector'], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-runtime-selector-control-' + name + '.log').write_text(run.stdout)
        valid = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if valid else 'INVALID', flush=True)
        assert valid, run.stdout
    finally:
        source.write_text(original)
