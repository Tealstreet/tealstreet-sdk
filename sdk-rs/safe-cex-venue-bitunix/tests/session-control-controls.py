from pathlib import Path
import os
import subprocess
import json

root = Path(__file__).resolve().parents[3]
venue = root / 'sdk-rs/safe-cex-venue-bitunix'
out = root / 'sdk-rs/target/bitunix-session-control-evidence'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
 ('startup', 'session_control.rs', 'start', 'host.connect("futuresWebsocket")?', 'host.connect("privateWebsocket")?', 'session_control', 'actual_typescript_start', 'SESSION native effect'),
 ('tick-period', 'session_control.rs', 'tick', 'count % 32.0 == 0.0', 'count % 31.0 == 0.0', 'session_control', 'actual_typescript_tick', 'SESSION native effect'),
 ('candle-delegate', 'session_control.rs', 'listen_ohlcv', 'if !configured(symbol(&options)?)?', 'if configured(symbol(&options)?)?', 'session_control', 'actual_typescript_candle_delegate', 'SESSION native effect'),
 ('public-connect', 'session_control.rs', 'connect_public', 'connect(host, SocketKind::Public)', 'connect(host, SocketKind::Futures)', 'session_control', 'actual_typescript_public_connection', 'SESSION native effect'),
 ('futures-connect', 'session_control.rs', 'connect_futures', 'connect(host, SocketKind::Futures)', 'connect(host, SocketKind::Public)', 'session_control', 'actual_typescript_futures_connection', 'SESSION native effect'),
 ('private-connect', 'session_control.rs', 'connect_private', 'connect(host, SocketKind::Private)', 'connect(host, SocketKind::Futures)', 'session_control', 'actual_typescript_private_connection', 'SESSION native effect'),
 ('private-open', 'session_control.rs', 'private_after_login', '15_000.0', '14_999.0', 'session_control', 'actual_typescript_private_open', 'SESSION native effect'),
 ('candle-volume', 'ohlcv_subscription.rs', 'parse', 'volume: float(field(json, "volume")?)?', 'volume: float(field(json, "close")?)?', 'ohlcv_subscription', 'actual_typescript_ohlcv_subscription', 'CANDLE native fields'),
]
results = []
for name, file, function, before, after, target, test, assertion in controls:
 source = venue / 'src' / file
 original = source.read_text()
 markers = ['pub fn ' + function, 'pub async fn ' + function]
 start = next(original.index(m) for m in markers if m in original)
 end = original.find('\npub ', start + 1)
 if end == -1:
  end = len(original)
 block = original[start:end]
 assert block.count(before) == 1, (name, before)
 command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bitunix', '--jobs', '2', '--test', target, test, '--', '--exact', '--nocapture']
 try:
  source.write_text(original[:start] + block.replace(before, after) + original[end:])
  failed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
  (out / (name + '-mutated.log')).write_text(failed.stdout)
  assert failed.returncode == 101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout, (name, failed.stdout[-3000:])
 finally:
  source.write_text(original)
 passed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
 (out / (name + '-restored.log')).write_text(passed.stdout)
 assert passed.returncode == 0, (name, passed.stdout[-3000:])
 results.append({'family': name, 'compiled_exit': failed.returncode, 'assertion': assertion, 'restored_exit': passed.returncode})
 (out / 'controls.json').write_text(json.dumps(results, indent=2) + '\n')
 print(name + ': compiled native failure 101; restored pass 0', flush=True)
assert len(results) == 8
