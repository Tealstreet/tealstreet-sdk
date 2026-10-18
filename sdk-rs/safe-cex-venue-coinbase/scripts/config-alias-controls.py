import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
scenes = [
    ("src/cdp_signing.rs", "crypto_key_and_ws_auth_aliases_use_real_import_cache_signer_and_node_verifier",
     b"*r.cache.key_secret.borrow_mut() = secret;", b"*r.cache.key_secret.borrow_mut() = String::new();"),
    ("src/order_sync.rs", "input_root_aliases_retain_presence_numbers_and_actual_event_array_adapter",
     b"Ok(self.borrow().orders.clone())", b"Ok(Rc::new(RefCell::new(self.borrow().orders.borrow().clone())))"),
]
for file, name, before, after in scenes:
    source = workspace / "safe-cex-venue-coinbase" / file
    original = source.read_bytes()
    assert original.count(before) == 1, file
    command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-coinbase",
               "--lib", "config_alias_native::" + name]

    def run():
        return subprocess.run(command, cwd=workspace, env={**os.environ, "CARGO_BUILD_JOBS": "1"},
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    try:
        source.write_bytes(original.replace(before, after))
        failed = run()
        Path(f"/tmp/coinbase-config-alias-{name}-red.log").write_text(failed.stdout)
        assert failed.returncode != 0 and "panicked at" in failed.stdout, failed.stdout
        assert name + " ... FAILED" in failed.stdout, failed.stdout
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    restored = run()
    Path(f"/tmp/coinbase-config-alias-{name}-restored.log").write_text(restored.stdout)
    assert restored.returncode == 0 and "1 passed; 0 failed" in restored.stdout, restored.stdout
    print(f"{name}: compiled production mutation RED; exact restoration GREEN", flush=True)
