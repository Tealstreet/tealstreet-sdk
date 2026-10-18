#!/usr/bin/env python3
"""One compiling mutation per new diagnostic family, serial Cargo, exact restore."""
from pathlib import Path
import os,subprocess,sys
root=Path(__file__).resolve().parents[3]
path=root/'sdk-rs/safe-cex-venue-bybit/src/failure_diagnostics.rs'
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','failure_diagnostics','--jobs','2','--','--include-ignored']
controls=[
 ('constant','5 * 60_000','4 * 60_000','DIAGNOSTIC native cooldown'),
 ('response','    pub terminal: Option<bool>,','    #[serde(rename = "recorded")]\n    pub terminal: Option<bool>,','DIAGNOSTIC native response terminal'),
 ('telemetry','strict_status == Some(429.)','strict_status == Some(428.)','DIAGNOSTIC native telemetry reason'),
 ('retry','numeric >= 500. && numeric < 600.','numeric >= 500. && numeric < 500.','DIAGNOSTIC native retry'),
 ('field','if !value.is_null() {','if value.is_null() {','DIAGNOSTIC native error field'),
 ('request-error','Value::String("bybit_markets_startup".into())','Value::String("changed".into())','DIAGNOSTIC native request tags'),
 ('leverage','code == 110043.','code == 110044.','DIAGNOSTIC native leverage'),
 ('request-default','retry_count.unwrap_or(0.)','retry_count.unwrap_or(1.)','DIAGNOSTIC native request default tags'),
]
if '--default-only' in sys.argv: controls=[control for control in controls if control[0]=='request-default']
print('candidate SHA',subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),flush=True)
for family,before,after,assertion in controls:
 original=path.read_text();assert original.count(before)==1,(family,'selector drift')
 try:
  changed=original.replace(before,after)
  if family=='response':
   # Swap both ingress/egress attributes. Only native field assertions catch a
   # symmetric mapping swap; a decode/re-encode comparison would be blind.
   assert changed.count('    pub recorded: Option<bool>,')==1
   changed=changed.replace('    pub recorded: Option<bool>,','    #[serde(rename = "terminal")]\n    pub recorded: Option<bool>,')
  path.write_text(changed)
  r=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  print(family,'mutated exit',r.returncode,flush=True);print(r.stdout,flush=True)
  assert r.returncode==101 and 'panicked at' in r.stdout and assertion in r.stdout,(family,'SURVIVOR or non-runtime failure')
 finally:path.write_text(original)
 r=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 print(family,'restored exit',r.returncode,flush=True);print(r.stdout,flush=True)
 assert r.returncode==0 and '1 passed; 0 failed; 0 ignored' in r.stdout,(family,'restore failed')
