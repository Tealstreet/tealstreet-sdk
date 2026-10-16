"""Check pinned corpus counts/identities; --write explicitly recaptures after review."""
from pathlib import Path
import argparse,json,os,subprocess
parser=argparse.ArgumentParser();parser.add_argument('--write',action='store_true');parser.add_argument("--only");args=parser.parse_args()
root=Path(__file__).resolve().parent
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript',TZ='UTC')
for name,script in [('parity','oracle.cjs'),('portfolio','portfolio_cardinality_oracle.cjs'),('portfolio_reads','portfolio_reads_oracle.cjs'),('raw_reads','rest_reads_oracle.cjs'),('quotes','quotes_oracle.cjs')]:
 if args.only and args.only!=name:continue
 result=subprocess.run(['node',str(root/script)],env=env,capture_output=True,text=True,check=True)
 corpus=json.loads(result.stdout);counts={}
 def walk(value,path):
  if isinstance(value,list):
   counts[path]=len(value)
   for i,item in enumerate(value):walk(item,f'{path}/{i}')
  elif isinstance(value,dict):
   for key,item in value.items():walk(item,f'{path}/{key}')
 walk(corpus,'');identities={}
 for key,items in corpus.items():
  if isinstance(items,list):
   ids=[]
   for item in items:
    identity={field:item[field] for field in ['name','method','scenario','key','field','profile','arg'] if field in item}
    if isinstance(item.get('expected'),dict):identity['expectedKeys']=sorted(item['expected'])
    ids.append(identity)
   identities[key]=ids
 capture={'counts':counts,'identities':identities};path=root/'support'/f'{name}-census.json'
 if args.write:path.write_text(json.dumps(capture,sort_keys=True,separators=(',',':'))+'\n')
 else:assert capture==json.loads(path.read_text()),name
 print(name,len(corpus['cases']),'cases',len(counts),'array counts',corpus['runtime'])
