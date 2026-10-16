from pathlib import Path
import subprocess,os
root=Path(__file__).resolve().parents[3];p=root/'sdk-rs/safe-cex-venue-breakoutprop/src/quotes.rs';original=p.read_bytes();s=original.decode();needle='                timestamp: None,';assert s.count(needle)==1
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-breakoutprop','--jobs','2','--test','quotes','actual_quote_reads','--','--exact'];env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
try:
 p.write_text(s.replace(needle,'                timestamp: Some(0.0),'))
 r=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True);text=r.stdout+r.stderr;Path('/tmp/core-p4a-timestamp-mutated.log').write_text(text)
 if r.returncode!=101 or 'QUOTES native absent timestamp' not in text or 'error[E' in text:raise SystemExit(text)
 print('unexpected-timestamp: exit 101 at QUOTES native absent timestamp',flush=True)
finally:p.write_bytes(original)
r=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True);Path('/tmp/core-p4a-timestamp-restored.log').write_text(r.stdout+r.stderr)
if r.returncode:raise SystemExit(r.stdout+r.stderr)
print('unexpected-timestamp: restored exit 0',flush=True)
