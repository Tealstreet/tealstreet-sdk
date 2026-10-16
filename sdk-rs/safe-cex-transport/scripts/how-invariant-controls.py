# Run from the repository root, with no concurrent Cargo or parity jobs.
# Each mutation is restored in finally; assertion controls reject compile-only failures.
from pathlib import Path
import subprocess,os,json
root=Path.cwd(); env=dict(os.environ);env['PATH']='/Users/sam/.volta/tools/image/yarn/1.22.21/bin:/opt/homebrew/opt/rustup/bin:'+env['PATH']
base=root/'sdk-rs/safe-cex-transport'
cases=[]
def add(name,file,old,new,filter,target='--lib'):
 cases.append((name,base/file,old,new,['cargo','test','-p','safe-cex-transport',target] + (['tunnel'] if target=='--test' else [])+[filter,'--','--include-ignored']))
add('generation','src/how.rs','pub fn on_close(&self) {\n        self.generation.set(self.generation.get().wrapping_add(1));','pub fn on_close(&self) {\n        self.generation.set(self.generation.get());','old_receive_error_cannot_close_the_replacement_socket','--test')
add('ping-continue','src/how.rs','if now >= client.next_ping.get() && client.ping().is_err() {\n                    client.on_close();\n                    continue;','if now >= client.next_ping.get() && client.ping().is_err() {\n                    client.on_close();\n                    return;','ping_write_failure_reconnects_and_keeps_idle_monitor_running','--test')
add('grvt','src/how.rs','if base.to_ascii_lowercase().contains("grvt") {','if false {','grvt_frame_bypasses_proxy_but_other_venues_do_not','--test')
add('integer16','src/how.rs','end - digits >= 16','end - digits >= 17','raw_integer_scanner_protects_exactly_nested_sixteen_digit_tokens')
codec='src/codec.rs';test='binary_envelope_guards_report_the_specific_invalid_component'
for name,old,new in [('prefix','if bytes.len() < 8 {','if false {'),('magic','if &bytes[..4] != b"TSB1" {','if false {'),('length-zero','length == 0 || length > 1024 * 1024','length > 1024 * 1024'),('length-max','length == 0 || length > 1024 * 1024','length == 0'),('payload','if offset >= bytes.len() {','if offset > bytes.len() {'),('type','value["type"] != "rpc.http"','false'),('nonce','!value["nonce"].is_string()','false'),('encoding','value["data"]["data_encoding"] != "gzip"','false'),('utf8','std::str::from_utf8(&bytes[8..offset]).map_err(decode_error)?','std::str::from_utf8(&bytes[8..offset]).unwrap_or("{}")'),('json-valid-path','crate::how::json_parse(header)','{ let _ = header; serde_json::json!({}) }')]:
 add(name,codec,old,new,test)
add('fresh-each-dial','src/relay.rs','Some(UpgradeHeaders::Fresh(provider)) => provider(),','Some(UpgradeHeaders::Fresh(_provider)) => indexmap::IndexMap::new(),','relay_fresh_headers_are_lazy_and_resolved_for_each_tunnel_dial','--test')
add('required-preflight','src/relay.rs','if connection_options.headers_required','if false && connection_options.headers_required','relay_browser_ignores_best_effort_headers_but_rejects_required_auth_before_dial','--test')
results=[]
for name,file,old,new,cmd in cases:
 original=file.read_text(); assert original.count(old)==1,(name,original.count(old));file.write_text(original.replace(old,new,1))
 try:
  p=subprocess.run(cmd,cwd=root/'sdk-rs',env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  Path('/tmp/p2b-control-'+name+'.log').write_text(p.stdout)
  ok=p.returncode==101 and 'FAILED' in p.stdout and 'test result: FAILED' in p.stdout
  print(name,p.returncode,'assertion-red' if ok else 'BAD',flush=True);results.append({'name':name,'exit':p.returncode,'assertion_failure':ok})
  if not ok: print(p.stdout[-1600:],flush=True)
 finally:file.write_text(original)
Path('/tmp/p2b-controls-results.json').write_text(json.dumps(results,indent=2))
assert all(x['assertion_failure'] for x in results)
