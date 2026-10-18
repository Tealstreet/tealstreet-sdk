import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
scenes = [
    ("src/public_stream_dispatch.rs", "stream_and_tick_aliases_capture_actual_class_handlers_and_native_frames",
     b"        // for-of is live; appending a callback during delivery is observable.\n        let mut i = 0;\n        loop {",
     b"        let callback_count = callbacks.borrow().len();\n        let mut i = 0;\n        loop {\n            if i >= callback_count { break; }"),
    ("src/binding/sockets.rs", "book_aliases_retain_native_level_roots_at_the_production_store_adapter",
     b"                    price: level.price,", b"                    price: 0.0,"),
]
for file, name, before, after in scenes:
    source = workspace / "safe-cex-venue-coinbase" / file
    original = source.read_bytes()
    assert original.count(before) == 1, file
    command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-coinbase",
               "--lib", "public_stream_alias_native::" + name]

    def run():
        return subprocess.run(command, cwd=workspace, env={**os.environ, "CARGO_BUILD_JOBS": "1"},
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    try:
        source.write_bytes(original.replace(before, after))
        failed = run()
        Path(f"/tmp/coinbase-public-stream-alias-{name}-red.log").write_text(failed.stdout)
        assert failed.returncode != 0 and "panicked at" in failed.stdout, failed.stdout
        assert name + " ... FAILED" in failed.stdout, failed.stdout
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    restored = run()
    Path(f"/tmp/coinbase-public-stream-alias-{name}-restored.log").write_text(restored.stdout)
    assert restored.returncode == 0 and "1 passed; 0 failed" in restored.stdout, restored.stdout
    print(f"{name}: compiled production mutation RED; exact restoration GREEN", flush=True)
