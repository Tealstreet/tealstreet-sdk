import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-hyperliquid/src/signing.rs'
scenes = [
    ('prefix', b'.or_else(|| address.strip_prefix("0X"))', b'.or_else(|| address.strip_prefix("0x"))', 'native_realm_signer_accepts_ethers_byteslike_prefixes'),
    ('metadata', b'("code", OwnedJsValue::string("INVALID_ARGUMENT"))', b'("code", OwnedJsValue::string("CONTROL_INVALID_ARGUMENT"))', 'native_api_signing_retains_original_error_through_transport'),
]
for scene, before, after, name in scenes:
    original = source.read_bytes()
    assert original.count(before) == 1, scene
    command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-hyperliquid', '--lib', 'byteslike_signing::' + name]

    def run():
        return subprocess.run(command, cwd=workspace, env={**os.environ, 'CARGO_BUILD_JOBS': '1'}, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    try:
        source.write_bytes(original.replace(before, after))
        failed = run()
        Path(f'/tmp/hyperliquid-byteslike-{scene}-red.log').write_text(failed.stdout)
        assert failed.returncode != 0 and 'panicked at' in failed.stdout, failed.stdout
        assert name + ' ... FAILED' in failed.stdout, failed.stdout
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    restored = run()
    Path(f'/tmp/hyperliquid-byteslike-{scene}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, restored.stdout
    print(f'{scene}: compiled named RED; exact-restored GREEN', flush=True)
