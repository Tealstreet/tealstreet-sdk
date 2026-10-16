"""Symmetric enum decode/encode swap: roundtrip survives, native variant must fail."""
from pathlib import Path
import os, subprocess
root = Path(__file__).resolve().parents[3]
p = root / 'sdk-rs/safe-cex-venue-apexomni/src/signer_records.rs'
original = p.read_text()
changes = [
 ('    EthereumSignature,', '    #[serde(rename = "EIP1271Signature")]\n    EthereumSignature,'),
 ('    EIP1271Signature,', '    #[serde(rename = "EthereumSignature")]\n    EIP1271Signature,'),
 ('"EthereumSignature" => Ok(Self::EthereumSignature)', '"EthereumSignature" => Ok(Self::EIP1271Signature)'),
 ('"EIP1271Signature" => Ok(Self::EIP1271Signature)', '"EIP1271Signature" => Ok(Self::EthereumSignature)'),
]
mutated = original
for before, after in changes:
 assert mutated.count(before) == 1, before
 mutated = mutated.replace(before, after)
command = [os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'test', '--locked', '--manifest-path', str(root/'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-apexomni', '--jobs', '1', '--test', 'records', 'signer_signature_matches_actual_typescript', '--', '--nocapture']
logs = root/'sdk-rs/target/apexomni-controls'
try:
 p.write_text(mutated)
 result = subprocess.run(command, cwd=root, capture_output=True, text=True)
 output = result.stdout + result.stderr
 (logs/'signer-record-mutation.log').write_text(output)
 assert result.returncode == 101 and 'test result: FAILED.' in output, output
 assert 'TxEthSignature native decoded kind' in output, output
 assert 'TxEthSignature emitted native fields' not in output, output
 print('symmetric production swap: 101, TxEthSignature native decoded kind (roundtrip passed first)', flush=True)
finally:
 p.write_text(original)
result = subprocess.run(command, cwd=root, capture_output=True, text=True)
(logs/'signer-record-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('restored target: 0', flush=True)
