"""Serial runtime controls; compiler rejection and survivors are retained failures."""
import json,os,pathlib,subprocess,sys
root=pathlib.Path.cwd();out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
controls=[
 ('limit','native_frame_limit','pub const BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES: f64 = 4096.0;','pub const BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES: f64 = 4095.0;'),
 ('strict_limit','native_build_frames','serialized.len() as f64 > max_bytes','serialized.len() as f64 >= max_bytes'),
 ('endpoint','native_connect','if host.is_testnet() {','if !host.is_testnet() {'),
 ('open_disposed','native_open','pub fn on_open','pub fn on_open'),
 ('added_channel_order','native_markets_changed','ticker_topics.into_iter().chain(funding_topics)','funding_topics.into_iter().chain(ticker_topics)'),
 ('ping_timestamp','native_ping','state.ping_at.set(host.performance_now());','state.ping_at.set(0.0);'),
 ('own_key_order','native_subscribe','.values().flatten()','.values().rev().flatten()'),
 ('frame_order','native_send_topics','for frame in frames {','for frame in frames.into_iter().rev() {'),
 ('watchdog_delay','native_arm_notice','CONNECTION_FAILURE_NOTICE_DELAY_MS: f64 = 20_000.0;','CONNECTION_FAILURE_NOTICE_DELAY_MS: f64 = 2_000.0;'),
 ('established_flag','native_mark_established','state.connection_failure_notified.set(false);','state.connection_failure_notified.set(true);'),
]
assert len(controls)==10
path=root/'sdk-rs/safe-cex-venue-blofin/src/public_stream_core.rs';saved=path.read_bytes();original_source=saved.decode()
# Scoped onOpen-only mutation, generated from the real method span rather than
# replacing guards in unrelated functions.
a=original_source.index('pub fn on_open');b=original_source.index('pub fn on_markets_changed',a)
old=original_source[a:b];assert old.count('if !host.is_disposed() {')==1
controls[3]=('open_disposed','native_open',old,old.replace('if !host.is_disposed() {','if host.is_disposed() {',1))
# A symmetric codec mutation deliberately swaps native decoded values, then
# swaps them back at serialization. Wire bytes would agree; input/native checks
# must catch it before any wire comparison.
codec = original_source.replace('#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]', '#[derive(Debug, Clone, PartialEq, Serialize)]', 1)
needle = 'fn topic(channel: &str, inst_id: String) -> SubscriptionTopic {'
custom = """impl<'de> Deserialize<'de> for SubscriptionTopic {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self,D::Error> {
        let mut fields=JsObjectMap::<Value>::deserialize(decoder)?;
        if let (Some(channel),Some(inst_id))=(fields.get("channel").cloned(),fields.get("instId").cloned()) {
            fields.insert("channel".into(),inst_id);
            fields.insert("instId".into(),channel);
        }
        Ok(Self{fields})
    }
}
"""
assert codec.count(needle)==1
codec=codec.replace(needle,custom+needle,1)
needle='JsJsonValue::from(value.clone())'
assert codec.count(needle)==1
codec=codec.replace(needle,'JsJsonValue::from(match key.as_str() {"channel"=>topic.fields.get("instId").unwrap_or(value).clone(),"instId"=>topic.fields.get("channel").unwrap_or(value).clone(),_=>value.clone()})',1)
controls.append(('symmetric_topic_codec','native_build_frames',original_source,codec))
assert len(controls)==11
for label,_,pattern,_ in controls: assert original_source.count(pattern)==1,label+': preflight unique mutation target'
if len(sys.argv)>2 and sys.argv[2]=='--preflight':
 print('11 unique mutation targets verified; no mutation or Cargo run')
 raise SystemExit(0)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');results=[]
for index,(label,test,pattern,replacement) in enumerate(controls,1):
 assert original_source.count(pattern)==1,label+': target pattern occurs exactly once'
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','public_stream_core','--jobs','2',test,'--','--exact','--nocapture']
 try:
  path.write_text(original_source.replace(pattern,replacement,1))
  broken=subprocess.run(command,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (out/(label+'-broken.log')).write_text(broken.stdout)
 finally:path.write_bytes(saved)
 restored=subprocess.run(command,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(out/(label+'-restored.log')).write_text(restored.stdout)
 caught=broken.returncode==101 and 'panicked at' in broken.stdout and f'test {test} ... FAILED' in broken.stdout
 passed=restored.returncode==0 and 'test result: ok. 1 passed;' in restored.stdout
 row=dict(sha=sha,label=label,call_site=str(path.relative_to(root)),assertion=test,command=command,broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed)
 results.append(row);(out/'results.json').write_text(json.dumps(dict(sha=sha,expected_controls=11,stopped_after=index,results=results),indent=2)+'\n');print(json.dumps(row),flush=True)
 assert path.read_bytes()==saved
 if not caught or not passed:raise SystemExit('Survivor or failed before named runtime assertion; retained: '+label)
