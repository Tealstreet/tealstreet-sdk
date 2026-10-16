#!/usr/bin/env python3.11
"""Scoped tomllib census, comparing symbol identities to an explicit git base."""
from pathlib import Path
from collections import Counter
import argparse,json,subprocess,tomllib
root=Path(__file__).resolve().parents[3]
parser=argparse.ArgumentParser();parser.add_argument('--base',default='9d6f40b96f');args=parser.parse_args()
base=subprocess.check_output(['git','rev-parse',args.base],cwd=root,text=True).strip()
manifest=root/'sdk-rs/parity/manifest/transport'
files=[];before=Counter();after=Counter();movement=Counter();changed=[]
for path in sorted(manifest.rglob('*.toml')):
    relative=path.relative_to(root).as_posix()
    old=tomllib.loads(subprocess.check_output(['git','show',base+':'+relative],cwd=root,text=True))
    new=tomllib.loads(path.read_text())
    old_rows={(row['kind'],row['ts']):row for row in old.get('symbol',[])}
    assert set(old_rows)=={(row['kind'],row['ts']) for row in new.get('symbol',[])},relative
    for row in new.get('symbol',[]):
        original=old_rows[(row['kind'],row['ts'])];before[original['status']]+=1;after[row['status']]+=1
        if row['status']!=original['status']:
            movement[original['status']+'->'+row['status']]+=1
            changed.append({'file':relative,'symbol':row['ts'],'before':original['status'],'after':row['status']})
    files.append({'file':relative,'rows':len(new.get('symbol',[])),'statuses':dict(Counter(row['status'] for row in new.get('symbol',[])))})
assert len(files)==11,'independent transport-file floor at iteration 1'
assert sum(before.values())==63,'independent transport-row floor at iteration 1'
print(json.dumps({'base':base,'scope':'transport subtree only, enumeration iteration 1','before':dict(before),'after':dict(after),'movement':dict(movement),'files':files,'changed':changed},indent=2))
