import json
import os
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/private_feed.rs'
out = root / 'sdk-rs/target/weex-private-feed-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
command = ['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-weex','--jobs','1','--test','private_feed']
env = dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
 ('connection','(rt.create_socket)(host, url, None, rt.auth_headers, true)', '(rt.create_socket)(host, url, None, rt.auth_headers, false)', 'native_socket_options::connect_basic'),
 ('message-pong','(rt.set_subscription_id)(host, id + 1.0)', '(rt.set_subscription_id)(host, id + 2.0)', 'native_subscription_id::message_ping'),
 ('order-status','Some(OrderStatus::Closed)', 'Some(OrderStatus::Open)', 'native_statuses::orders_closed'),
 ('position-add','(rt.add_positions)(host, positions)?;', 'drop(positions);', 'native_positions::positions_basic'),
 ('fill-price','/ (rt.number)(host, &(rt.property)(host, &fill, "fillSize")?)?', '* (rt.number)(host, &(rt.property)(host, &fill, "fillSize")?)?', 'native_fill_price::message_large_fill'),
 ('refresh-update','(rt.apply_refresh)(host, kind, value)?;', 'drop(value);', 'native_refresh_updates::orders_refresh'),
 ('dispose','(rt.base_dispose)(host)', 'Ok(())', 'native_disposed::dispose_missing_timers'),
]
selected=os.environ.get('WEEX_PRIVATE_FEED_CONTROL')
controls=[control for control in controls if selected is None or control[0]==selected]
assert controls, ('unknown control', selected)
rows=[]
for name,before,after,assertion in controls:
 original=source.read_bytes();text=original.decode();assert text.count(before)==1,(name,text.count(before))
 try:
  source.write_text(text.replace(before,after))
  broken=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (out/(name+'-broken.log')).write_text(broken.stdout)
  assert broken.returncode==101,(name,broken.returncode)
  assert assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout,(name,broken.stdout)
 finally:source.write_bytes(original)
 restored=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/(name+'-restored.log')).write_text(restored.stdout)
 assert restored.returncode==0 and '4 passed; 0 failed' in restored.stdout,(name,restored.stdout)
 row={'control':name,'assertion':assertion,'brokenExit':broken.returncode,'restoredExit':restored.returncode};rows.append(row);print(json.dumps(row),flush=True)
(out/'mutation-results.json').write_text(json.dumps({'sha':sha,'controls':rows},indent=2)+'\n')
