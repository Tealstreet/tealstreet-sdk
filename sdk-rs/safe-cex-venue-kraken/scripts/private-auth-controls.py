#!/usr/bin/env python3
"""Prove one named native assertion discriminates for each private-auth row."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
production = crate / 'src/private_auth.rs'
original = production.read_bytes()
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('set-token', 'set_token_live_source', 'pub fn set_token(state: &RefCell<SpotAuthState>, token: JsJsonValue) {\n    state.borrow_mut().token = token;\n}',
     'pub fn set_token(state: &RefCell<SpotAuthState>, token: JsJsonValue) {\n    drop(token);\n    state.borrow_mut().token = JsJsonValue::Undefined;\n}', 'PRIVATE-AUTH native token'),
    ('spot-open', 'spot_open_live_source', 'state.borrow_mut().authenticated = true;',
     'state.borrow_mut().authenticated = false;', 'PRIVATE-AUTH native authenticated'),
    ('futures-auth', 'futures_auth_live_source', 'state.borrow_mut().waiting_challenge = true;',
     'state.borrow_mut().waiting_challenge = false;', 'PRIVATE-AUTH native waiting challenge'),
    ('futures-open', 'futures_open_live_source',
     'format!("Kraken private WS auth failed: {}", error_text(&error)),\n                LogSeverity::Error,',
     'format!("Kraken private WS auth failed: {}", error_text(&error)),\n                LogSeverity::Warning,',
     'PRIVATE-AUTH native open severity'),
]
for name, test, old, new, assertion in controls:
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', 'private_auth', test, '--jobs', '2']
    def run():
        return subprocess.run(command, env=environment, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    try:
        text = original.decode()
        assert text.count(old) == 1, (name, 'production mutation anchor')
        production.write_text(text.replace(old, new))
        broken = run()
        assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout, (name, broken.stdout)
        print(f'{name}: exit 101; {assertion}', flush=True)
    finally:
        if production.read_bytes() != original:
            production.write_bytes(original)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    assert production.read_bytes() == original
    print(f'{name}: restored exit 0', flush=True)
print(f'{revision}: 4 rows; 4 named failures; 4 restored passes')
