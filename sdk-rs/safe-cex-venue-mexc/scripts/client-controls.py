"""Compiling factory mutations must fail the maintained preimage/wire oracle."""
from pathlib import Path
import subprocess
crate = Path(__file__).resolve().parents[1]
source = crate / 'src/client.rs'
original = source.read_text()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-venue-mexc', '--test', 'client', '--jobs', '2', '--', '--exact', 'mexc_factory_live_shared_http_composition', '--nocapture']
controls = [
    ('public signing bypass', 'if is_mexc_public_endpoint(Some(&request.url)) {', 'if is_mexc_public_endpoint(Some(&request.url)) && request.params.is_none() {'),
    ('HEAD signs query instead of body', 'request.method.eq_ignore_ascii_case("GET")', 'matches!(request.method.as_str(), "get" | "head")'),
    ('response rejection suppressed', 'if rejected && js_truthy(message) {', 'if rejected && js_truthy(message) && data.get("success") == Some(&Value::Bool(false)) {'),
    ('read retry count shortened', 'network_error_on_read(read, 3)', 'network_error_on_read(read, 1)'),
]
try:
    for name, before, after in controls:
        assert original.count(before) == 1
        source.write_text(original.replace(before, after))
        result = subprocess.run(command, cwd=crate.parents[1], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode == 101, result.stdout
        assert 'mexc_factory_live_shared_http_composition ... FAILED' in result.stdout, result.stdout
        assert 'assertion `left == right` failed' in result.stdout, result.stdout
        print(f'{name}: compiled and named assertion failed at 101', flush=True)
finally:
    source.write_text(original)
subprocess.run(command, cwd=crate.parents[1], check=True)
