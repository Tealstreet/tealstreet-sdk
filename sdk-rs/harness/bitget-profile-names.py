"""Map source names through normal Binaryen passes; require exact production code."""
from pathlib import Path
import argparse
import subprocess
import hashlib
import re
import json

parser = argparse.ArgumentParser()
parser.add_argument('--raw', required=True)
parser.add_argument('--production', required=True)
parser.add_argument('--out', required=True)
args = parser.parse_args()
folder = Path(args.out)
folder.mkdir(exist_ok=False)
opt = 'wasm-opt'
flags = ['--enable-bulk-memory', '--enable-bulk-memory-opt', '--enable-mutable-globals',
         '--enable-sign-ext', '--enable-nontrapping-float-to-int', '--enable-reference-types', '--enable-multivalue']
def run(*args):return subprocess.check_output(args,text=True,stderr=subprocess.PIPE)
def leb(d,i):
 v=0;shift=0
 while True:
  x=d[i];i+=1;v|=(x&127)<<shift;shift+=7
  if x<128:return v,i

def sections(p):
 d=p.read_bytes();i=8;o=[]
 while i<len(d):
  start=i;k=d[i];n,i=leb(d,i+1);o.append((k,d[i:i+n],d[start:i+n]));i+=n
 return o

def codeseal(p):return hashlib.sha256(b'\0asm\1\0\0\0'+b''.join(full for k,body,full in sections(p) if k)).hexdigest()
def function_names(body):
 size,i=leb(body,0);assert body[i:i+size]==b'name';i+=size;out={}
 while i<len(body):
  tag=body[i];size,i=leb(body,i+1);end=i+size
  if tag==1:
   n,i=leb(body,i)
   for _ in range(n):
    idx,i=leb(body,i);size,i=leb(body,i);out[idx]=body[i:i+size].decode();i+=size
  i=end
 return out

def parse_map(text):
 result={}
 for line in text.splitlines():
  m=re.fullmatch(r'\s*(\d+):\s*(.*)',line)
  if m:result[int(m[1])]=m[2]
 assert result,text[:1000]
 return result

run('wasm-bindgen','--target','nodejs','--remove-producers-section','--out-name','safe_cex_wasm','--out-dir',str(folder),args.raw)
named=folder/'safe_cex_wasm_bg.wasm';names={};kept=[]
for kind,body,full in sections(named):
 if kind==0:
  size,i=leb(body,0)
  if body[i:i+size]==b'name':names=function_names(body);continue
 kept.append(full)
source=folder/'input.wasm';source.write_bytes(b'\0asm\1\0\0\0'+b''.join(kept));assert names
for phase in range(2):
 oldmap=parse_map(run(opt,str(source),'--print-function-map',*flags,'-o',str(folder/'inspected.wasm')))
 lineage={name:names.get(index,'unmapped') for index,name in oldmap.items()}
 destination=folder/f'pass-{phase}.wasm'
 output=run(opt,str(source),'-Oz','--print-function-map',*flags,'-o',str(destination))
 (folder/f'pass-{phase}.map').write_text(output)
 newmap=parse_map(output)
 sec={k:body for k,body,full in sections(destination) if k}
 count,_=leb(sec[3],0);imports,_=leb(sec[2],0)
 assert set(newmap)==set(range(count+imports)), 'printed map must describe final function indices'
 names={index:lineage.get(name,'unmapped') for index,name in newmap.items()}
 source=destination
assert codeseal(source)==codeseal(Path(args.production)), 'optimized code differs from production'
(folder/'verified-names.json').write_text(json.dumps(names,indent=2))
(folder/'proof.json').write_text(json.dumps({'codeHash': codeseal(source), 'productionSha256': hashlib.sha256(Path(args.production).read_bytes()).hexdigest(), 'rawSha256': hashlib.sha256(Path(args.raw).read_bytes()).hexdigest(), 'mapped': sum(n != 'unmapped' for n in names.values()), 'total': len(names)}, indent=2))
