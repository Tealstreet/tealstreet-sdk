# Run from the repository root, with no concurrent Cargo or parity jobs.
# Each mutation is restored in finally; assertion controls reject compile-only failures.
from pathlib import Path
import subprocess,os,json
root=Path.cwd(); env=dict(os.environ);env['PATH']='/Users/sam/.volta/tools/image/yarn/1.22.21/bin:/opt/homebrew/opt/rustup/bin:'+env['PATH']
base=root/'sdk-rs/safe-cex-transport'
cases=[]
def add(name,file,old,new,filter,target='--lib'):
 cases.append((name,base/file,old,new,['cargo','test','-p','safe-cex-transport',target] + (['tunnel'] if target=='--test' else [])+[filter,'--','--include-ignored']))

add('token-reset','src/how.rs','state.socket = Some(socket);\n        state.token_sent = false;','state.socket = Some(socket);','new_socket_resets_a_token_send_latched_while_disconnected','--test')
add('rpc-v4','src/how.rs','"rpc_version":4','"rpc_version":3','how_raw_mode_retains','--test')
add('deadline-precheck','src/socket.rs','host.now_ms() >= deadline || timeout.as_mut().poll(cx).is_ready()','timeout.as_mut().poll(cx).is_ready()','elapsed_deadline_never_polls_work_even_when_the_timer_has_not_fired')
add('relay-release','src/relay.rs','pub fn on_inner_close(&self, code: u16, reason: String, was_clean: bool) {\n        self.socket.borrow_mut().take();','pub fn on_inner_close(&self, code: u16, reason: String, was_clean: bool) {','remote_close_releases_the_socket_before_relay_drop','--test')
add('stale-boundary','src/how.rs','saturating_sub(entry.updated_at) > 10_000','saturating_sub(entry.updated_at) >= 10_000','latency_monitor_queries_only_known_used_domains_and_obeys_strict_staleness','--test')
add('latency-stop','src/how.rs','pub fn stop_latency_checks(&self) {\n        self.next_latency_ticks.borrow_mut().pop();','pub fn stop_latency_checks(&self) {','latency_monitor_queries_only_known_used_domains_and_obeys_strict_staleness','--test')
add('latency-error','src/how.rs','if message.get("error").is_some_and(js_truthy) {','if false {','latency_monitor_queries_only_known_used_domains_and_obeys_strict_staleness','--test')
add('latency-default','src/how.rs','cached: fallback("cached", json!(false)),','cached: fallback("cached", json!(true)),','latency_tracking_happens_only_after_a_successful_socket_send_and_preserves_defaults','--test')
add('buffered','src/relay.rs','.map_or(0, |socket| socket.buffered_amount())','.map_or(0, |_socket| 0)','relay_callbacks_and_buffered_bytes_reflect_inner_events_and_network_buffer','--test')
add('request-parser','src/how.rs','parser(text).or_else(|_| parse_fallback_data(text))','{ let _ = parser; parse_data(text.as_bytes(), raw) }','custom_parser_and_forced_region_are_used_on_both_request_and_raw_response','--test')
add('response-parser','src/how.rs','let value = parser(&raw).map_err(|error| {','let value = serde_json::from_str::<Value>(&raw).map_err(|error| { let _ = parser;','custom_parser_and_forced_region_are_used_on_both_request_and_raw_response','--test')
add('forced-region','src/how.rs','options.region.map(Into::into),','None,','custom_parser_and_forced_region_are_used_on_both_request_and_raw_response','--test')
add('early-deadline','src/how.rs','self.prepare(deadline).await?;\n        if self.host.now_ms() >= deadline {','self.prepare(deadline).await?;\n        if false {','expired_preparation_skips_parser_even_when_later_send_guard_also_rejects','--test')
add('final-deadline','src/how.rs','log_trade_tunnel_request(&frame["arg"], Some(route));\n        if self.host.now_ms() >= deadline {','log_trade_tunnel_request(&frame["arg"], Some(route));\n        if false {','parser_consumed_deadline_does_not_send_or_close_the_shared_connection','--test')
add('simulation','src/how.rs','if crate::how_config::should_simulate_http_over_websocket() {\n                let config','if false {\n                let config','simulation_deadline_distinguishes_handed_request_from_executed_direct_future','--test')
add('redaction','src/how_diagnostics.rs','if sensitive_key(key) {','if false && sensitive_key(key) {','log_limits_redaction_utf16_and_primitive_content_match')
add('string-bound','src/how_diagnostics.rs','utf16.len() <= 500','utf16.len() <= 501','log_limits_redaction_utf16_and_primitive_content_match')
add('array-bound','src/how_diagnostics.rs','.take(20)','.take(21)','log_limits_redaction_utf16_and_primitive_content_match')
add('object-bound','src/how_diagnostics.rs','values.iter().take(50)','values.iter().take(51)','log_limits_redaction_utf16_and_primitive_content_match')
add('depth-bound','src/how_diagnostics.rs','Value::Array(values) if depth < 4','Value::Array(values) if depth < 5','log_limits_redaction_utf16_and_primitive_content_match')
add('object-depth','src/how_diagnostics.rs','Value::Object(values) if depth < 4','Value::Object(values) if depth < 5','log_limits_redaction_utf16_and_primitive_content_match')
results=[]
for name,file,old,new,cmd in cases:
 original=file.read_text(); assert original.count(old)>=1,(name,original.count(old));file.write_text(original.replace(old,new,1))
 try:
  p=subprocess.run(cmd,cwd=root/'sdk-rs',env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  Path('/tmp/p2b-control-'+name+'.log').write_text(p.stdout)
  ok=p.returncode==101 and 'FAILED' in p.stdout and 'test result: FAILED' in p.stdout
  print(name,p.returncode,'assertion-red' if ok else 'BAD',flush=True);results.append({'name':name,'exit':p.returncode,'assertion_failure':ok})
  if not ok: print(p.stdout[-1600:],flush=True)
 finally:file.write_text(original)
Path('/tmp/p2b-extra-controls-results.json').write_text(json.dumps(results,indent=2))
assert all(x['assertion_failure'] for x in results)
