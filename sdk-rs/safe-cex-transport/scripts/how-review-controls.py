# Run alone from repository root; require runtime assertion failure, never compilation failure.
from pathlib import Path
import subprocess, os, json
base=Path('sdk-rs/safe-cex-transport')
env=dict(os.environ)
env['PATH']='/Users/sam/.volta/tools/image/yarn/1.22.21/bin:/opt/homebrew/opt/rustup/bin:'+env['PATH']
cases=[
('url-split-surrogate','how_diagnostics.rs','String::from_utf16_lossy(&utf16[..500])','String::from_utf16_lossy(&utf16[..499])','url_summary_discloses'),
('live-ts-relay-open','../../../packages/http-over-websocket/src/RelayWebSocket.ts','for (const data of this.queue.splice(0)) this.inner.send(data as any);','try { for (const data of this.queue.splice(0)) this.inner.send(data as any); } catch {}','live_typescript_relay_open'),
('http-frame-order','how.rs','let mut frame = json!({"type":"rpc.http", "rpc_version":4});','let mut frame = json!({"type":"rpc.http", "rpc_version":4, "nonce":nonce, "arg":args});','http_frame_top_level'),
('heartbeat-error','how.rs','let _ = self.send(json!({"type":"rpc.ping", "nonce":self.next_nonce()}));','self.send(json!({"type":"rpc.ping", "nonce":self.next_nonce()}))?;','heartbeat_send_failure'),
('relay-token-message','relay.rs','.await\n            .map_err(|_| TransportError::Host("proxy token unavailable".into()))?;','.await?;','relay_token_rejection'),
('relay-auth-send-message','relay.rs','.map_err(|_| TransportError::Host("proxy token unavailable".into()))\n','\n','relay_token_rejection'),
('relay-open-throw','relay.rs','self.inner_send(frame)?;','if let Err(error) = self.inner_send(frame) { self.fail(error.to_string()); return Ok(()); }','relay_open_send_failure'),
('text-frame-rounding','codec.rs','crate::how::json_parse(&text)','serde_json::from_str(&text).unwrap_or(Value::Null)','latency_entry_serializes'),
('binary-frame-rounding','codec.rs','crate::how::json_parse(header)','serde_json::from_str(header).unwrap_or(Value::Null)','frame_numbers_collapse'),
('closed-send','how.rs','if !lower.contains("ping")','if true || !lower.contains("ping")','closed_how_ping_substrings'),
('ready-state','how.rs','socket.ready_state() == 1','{ let _ = socket; true }','ping_on_not_ready'),
('latency-orphan','how.rs','pub fn start_latency_checks(&self) {','pub fn start_latency_checks(&self) { self.stop_latency_checks();','repeated_latency_starts'),
('request-exhaustion','how.rs','share_failure.set(false);','/* exhaustion incorrectly remains shared */','late_auth_joiner'),
('simulation-terminal','how.rs','self.simulated_closed.set(true);','self.simulated_closed.set(false);','simulated_how_close'),
('dispose-deadline','how.rs','pub fn dispose(&self) {','pub fn dispose(&self) { self.on_close();','how_dispose_keeps'),
('base64-padding','codec.rs','DecodePaddingMode::RequireNone','DecodePaddingMode::Indifferent','owned_binary_views'),
('gzip-trailing','codec.rs','if !decoder.into_inner().is_empty()','if false && !decoder.into_inner().is_empty()','browser_decoder_strips'),
('header-bom','codec.rs',r"let header = header.strip_prefix('\u{feff}').unwrap_or(header);","let header = header;",'browser_decoder_strips'),
('body-bom','codec.rs',r".strip_prefix('\u{feff}')"+"\n                .unwrap_or(&decoded)",'.strip_prefix("")'+"\n                .unwrap_or(&decoded)",'browser_decoder_strips'),
('text-json-null','codec.rs','Ok((crate::how::json_parse(&text), None))','Ok((serde_json::from_str(&text).unwrap_or(serde_json::json!({})), None))','browser_decoder_strips'),
('relay-string','relay.rs','serde_json::Value::String(value) => value.clone(),','serde_json::Value::String(value) => serde_json::to_string(value).unwrap(),','relay_refusal_strings'),
('relay-null','relay.rs','.filter(|value| !value.is_null())','.filter(|_| true)','relay_refusal_strings'),
('parked-deadline','relay.rs','if self.state.get() == RelayState::Connecting\n                        && timer.as_mut().poll(cx).is_ready()','if timer.as_mut().poll(cx).is_ready()','relay_omitted_close'),
('live-ts-ping','../../../packages/http-over-websocket/src/HOWWebSocket.ts',"message.match(/(ping|pong)/i) || data?.type === 'rpc.ping'","false",'live_typescript_closed_send'),
('live-ts-latency','../../../packages/http-over-websocket/src/HOWWebSocket.ts','startLatencyChecks = () => {','startLatencyChecks = () => { this.stopLatencyChecks();','live_typescript_closed_send'),
('plain-json-rounding','how.rs','pub fn json_parse(text: &str) -> Value {\n    let mut value = serde_json::from_str(text).unwrap_or(Value::Null);\n    normalize_json_numbers(&mut value, false, false);','pub fn json_parse(text: &str) -> Value {\n    let mut value = serde_json::from_str(text).unwrap_or(Value::Null);\n    normalize_json_numbers(&mut value, true, false);','shared_json_parser_contract'),
('tunnel-reason','how.rs','reason: None,','reason: Some(response.data.status_text),','how_routes_and_preserves_signed_payload'),
('undefined-reset','how_config.rs','for field in patch.undefined_fields {','for field in patch.undefined_fields.into_iter().take(0) {','explicit_undefined_patch'),
]
if os.environ.get('HOW_CONTROL'):
 cases=[case for case in cases if case[0] in os.environ['HOW_CONTROL'].split(',')]
results=[]
for name,file,old,new,test in cases:
 p=base/'src'/file; original=p.read_text()
 assert original.count(old)==1,(name,original.count(old))
 p.write_text(original.replace(old,new))
 try:
  run=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-transport',test],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/how-review-control-'+name+'.log').write_text(run.stdout)
  ok=run.returncode==101 and 'test result: FAILED' in run.stdout
  print(name,run.returncode,'assertion-red' if ok else 'BAD',flush=True)
  results.append(dict(name=name,exit=run.returncode,assertion_failure=ok))
  if not ok: print(run.stdout[-2000:],flush=True)
 finally:p.write_text(original)
Path('/tmp/how-review-controls-results'+('-'+os.environ['HOW_CONTROL'].replace(',','-') if os.environ.get('HOW_CONTROL') else '')+'.json').write_text(json.dumps(results,indent=2))
assert all(r['assertion_failure'] for r in results)
