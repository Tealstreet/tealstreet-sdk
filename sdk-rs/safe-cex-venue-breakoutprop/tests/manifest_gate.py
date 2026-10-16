"""Run maintained source/index gates on only this venue; all scratch stays in target."""
from pathlib import Path
import collections
import json
import os
import re
import subprocess
import tomllib

crate=Path(__file__).resolve().parents[1]
sdk=crate.parent
manifest=sdk/'parity/manifest/exchanges/breakoutprop'
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
actual=json.loads(subprocess.check_output(['node',str(crate/'tests/manifest_symbols.cjs')],env=env,text=True))
counts=collections.Counter()
for source in actual:
    data=tomllib.loads((manifest/(Path(source['file']).name+'.toml')).read_text())
    expected={(s['ts'],s['kind']) for s in data['symbol']}
    extracted={(s['ts'],s['kind']) for s in source['symbols']}
    assert expected==extracted,(source['file'],expected-extracted,extracted-expected)
    counts.update(s['status'] for s in data['symbol'])
assert len(actual)==len(list(manifest.glob('*.toml')))==4
print('source identities:',sum(counts.values()),'rows;',dict(counts),flush=True)
# The indexer's public API scopes by workspace. Supply a one-member view whose
# source is a symlink to this actual crate, preserving dependency declarations.
scope=sdk/'target/breakoutprop-parity-scope'
scope.mkdir(parents=True,exist_ok=True)
config=(sdk/'Cargo.toml').read_text()
config=re.sub(r'members = \[.*?\]', 'members = ["safe-cex-venue-breakoutprop"]',config,count=1,flags=re.S)
config=re.sub(r'path = "(safe-cex[^"]*)"', lambda m: 'path = '+json.dumps(str(sdk/m[1])), config)
(scope/'Cargo.toml').write_text(config)
link=scope/'safe-cex-venue-breakoutprop'
if not link.exists():link.symlink_to(crate,target_is_directory=True)
(scope/'exempt.toml').write_text('')
subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','run','--manifest-path',str(sdk/'Cargo.toml'),'-p','safe-cex-parity','--bin','parity-check','--jobs','1','--','--workspace',str(scope),'--manifest',str(manifest),'--exempt',str(scope/'exempt.toml')],check=True,env=env)
