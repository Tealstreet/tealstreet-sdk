"""One discriminating production control for the leverage-read port."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-paradex/src/leverage_reads.rs'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'leverage_reads', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=
           '/Users/sam/cs/tealstreet-next/node_modules/typescript')


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'leverage-reads-{label}.log').write_text(output)
    return result.returncode, output


original = SOURCE.read_text()
needle = 'set(&config.market, config.leverage)?'
assert original.count(needle) == 1
baseline, output = run('baseline')
assert baseline == 0, output
try:
    SOURCE.write_text(original.replace(needle,
                      'set(&config.market, config.isolated_margin_leverage)?'))
    mutated, output = run('wrong-field')
finally:
    SOURCE.write_text(original)
assert mutated == 101 and 'test result: FAILED' in output, output
assert 'leverage_native_value' in output, output
restored, output = run('restored')
assert restored == 0, output
report = dict(head=subprocess.check_output(['git', 'rev-parse', 'HEAD'],
              cwd=ROOT, text=True).strip(), control='wrong-field',
              assertion='leverage_native_value', baseline_exit=baseline,
              mutation_exit=mutated, restored_exit=restored, survivors=[])
(LOGS / 'leverage-reads-controls.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
