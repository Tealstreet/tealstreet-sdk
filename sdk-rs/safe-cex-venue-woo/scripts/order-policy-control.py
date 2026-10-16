"""One discriminating native predicate control, followed by restoration."""
import json
import os
import subprocess
from pathlib import Path

path = Path('sdk-rs/safe-cex-venue-woo/src/orders.rs')
original = path.read_text()
before = 'Some("stop_market" | "take_profit_market" | "trailing_stop_market")'
after = 'Some("take_profit_market" | "trailing_stop_market")'
assert original.count(before) == 1, 'POLICY mutation admission'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path',
           'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-woo', '--test', 'order_policy',
           '--jobs', '2', '--offline', '--locked']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
evidence = {'sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
            'command': command}
try:
    path.write_text(original.replace(before, after, 1))
    run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path('/tmp/woo-policy-control.log').write_text(run.stdout)
    evidence['mutant_exit'] = run.returncode
    assert run.returncode == 101 and 'test result: FAILED' in run.stdout and \
        'POLICY native classification at 3' in run.stdout, run.stdout
    print('stop-loss-omission 101 POLICY native classification at 3', flush=True)
finally:
    path.write_text(original)
    Path('/tmp/woo-policy-control-evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
Path('/tmp/woo-policy-restored.log').write_text(run.stdout)
evidence['restored_exit'] = run.returncode
Path('/tmp/woo-policy-control-evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
assert run.returncode == 0, run.stdout
print('restored target passed', flush=True)
