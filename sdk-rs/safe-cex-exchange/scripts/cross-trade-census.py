#!/usr/bin/env python3.11
from pathlib import Path
from collections import Counter
import subprocess,json,tomllib,argparse
root=Path(__file__).resolve().parents[3];parser=argparse.ArgumentParser();parser.add_argument('--base',default='c5842fe5e7');args=parser.parse_args();base=subprocess.check_output(['git','rev-parse',args.base],cwd=root,text=True).strip()
before=Counter();after=Counter();movement=Counter();changed=[];files=[]
for path in sorted((root/'sdk-rs/parity/manifest/crossTrade').rglob('*.toml')):
 rel=path.relative_to(root).as_posix();old=tomllib.loads(subprocess.check_output(['git','show',base+':'+rel],cwd=root,text=True));new=tomllib.loads(path.read_text());previous={(row['kind'],row['ts']):row for row in old.get('symbol',[])}
 assert set(previous)=={(row['kind'],row['ts'])for row in new.get('symbol',[])},rel
 for row in new.get('symbol',[]):
  status=previous[(row['kind'],row['ts'])]['status'];before[status]+=1;after[row['status']]+=1
  if status!=row['status']: movement[status+'->'+row['status']]+=1;changed.append({'file':rel,'symbol':row['ts'],'before':status,'after':row['status']})
 files.append({'file':rel,'rows':len(new.get('symbol',[]))})
assert len(files)==6 and sum(before.values())==31,'independent crossTrade floor; enumeration iteration 1'
print(json.dumps({'base':base,'scope':'crossTrade subtree only; enumeration iteration 1','before':dict(before),'after':dict(after),'movement':dict(movement),'files':files,'changed':changed},indent=2))
