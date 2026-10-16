"""One production mutation for each of four newly ported operations."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p',
           'safe-cex-venue-paradex','--test','sdk_operations','--jobs','2']
ENV = dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
sdk = (CRATE / 'src/sdk_reads.rs').read_text()
controls = []
for name, function, request in [('balance','get_paraclear_balance','TokenBalanceRequest'),('factor','get_socialized_loss_factor','LossFactorRequest')]:
    start = sdk.index(f'pub async fn {function}<')
    end = sdk.index('\n}',start) + 2
    body = sdk[start:end]
    body = body.replace('S, M, C, P,', 'S, M, C: From<P>, P: From<C>,')
    needle = '            config,\n            provider,' if name == 'balance' else 'LossFactorRequest { config, provider }'
    replacement = '            config: provider.into(),\n            provider: config.into(),' if name == 'balance' else 'LossFactorRequest { config: provider.into(), provider: config.into() }'
    assert body.count(needle) == 1, (name,body)
    body = body.replace(needle, replacement)
    controls.append((name,'src/sdk_reads.rs',sdk[:start]+body+sdk[end:],name+'_native_config'))
source = (CRATE / 'src/lifecycle.rs').read_text()
controls.append(('dispose','src/lifecycle.rs',source.replace('    dispose_public()?;\n    dispose_private()?;', '    dispose_private()?;\n    dispose_public()?;'),'dispose_native_trace'))
source = (CRATE / 'src/bracket_validation.rs').read_text()
controls.append(('bracket','src/bracket_validation.rs',source.replace('long && tp <= entry','long && tp < entry'),'bracket_native_value'))


def run(label):
    result = subprocess.run(COMMAND,cwd=ROOT,env=ENV,capture_output=True,text=True)
    output = result.stdout + result.stderr
    (LOGS / f'sdk-operations-{label}.log').write_text(output)
    return result.returncode,output


head = subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
baseline,output = run('baseline')
assert baseline == 0,output
report = dict(head=head,baseline_exit=baseline,controls=[],survivors=[])
for name,file,changed,assertion in controls:
    path = CRATE / file
    original = path.read_text()
    assert changed != original,(name,'unchanged control')
    try:
        path.write_text(changed)
        mutated,output = run(name)
    finally:
        path.write_text(original)
    assert mutated == 101 and 'test result: FAILED' in output,(name,output)
    assert assertion in output,(name,output)
    restored,output = run(name+'-restored')
    assert restored == 0,(name,output)
    row = dict(control=name,assertion=assertion,mutation_exit=mutated,restored_exit=restored)
    report['controls'].append(row)
    (LOGS / 'sdk-operations-controls.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(row),flush=True)
assert len(report['controls']) == 4
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip() == head
