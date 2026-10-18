import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-exchange/src/js_runtime/base_exchange.rs"
name = "native_payload_provider_signs_real_paradex_order_modify_and_jwt"
command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-paradex", "--lib", "payload_signer_binding::" + name]
scenes = [
    ("installation", b"host.and_then(|host| host.payload_signer())", b"host.and_then(|host| host.payload_signer()).filter(|_| false)"),
    ("hash", b"payload: &payload,", b"payload: &payload[..3],"),
]
for scene, before, after in scenes:
    original = source.read_bytes()
    assert original.count(before) == 1, scene

    def run():
        return subprocess.run(command, cwd=workspace, env={**os.environ, "CARGO_BUILD_JOBS": "1"}, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    try:
        source.write_bytes(original.replace(before, after))
        failed = run()
        Path(f"/tmp/paradex-payload-provider-{scene}-red.log").write_text(failed.stdout)
        assert failed.returncode != 0 and "panicked at" in failed.stdout, failed.stdout
        assert name + " ... FAILED" in failed.stdout, failed.stdout
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    restored = run()
    Path(f"/tmp/paradex-payload-provider-{scene}-restored.log").write_text(restored.stdout)
    assert restored.returncode == 0 and "1 passed; 0 failed" in restored.stdout, restored.stdout
    print(f"{scene}: named compiled native RED; exact source restoration GREEN", flush=True)
