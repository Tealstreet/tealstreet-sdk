"""One compiled production-body control per remaining WebSocket row."""
from pathlib import Path
import subprocess,sys
root=Path(__file__).resolve().parents[3]
crate=root/'sdk-rs/safe-cex-venue-apexomni'
logs=root/'sdk-rs/target/apexomni-controls/ws-controls'
logs.mkdir(parents=True,exist_ok=True)
files={name:crate/'src'/name for name in ['ws_public.rs','ws_private.rs']}
original={name:path.read_text() for name,path in files.items()}
controls=[
 ('public','class',None,'"instrumentInfo.all".into()','"instrumentInfo.bad".into()','class'),
 ('public','constructor','constructor','host.initialize(false, false)?;','host.initialize(false, true)?;','constructor'),
 ('public','connect','connectAndSubscribe','usize::from(self.parent.base.options().testnet == Some(true))','usize::from(self.parent.base.options().testnet != Some(true))','connect'),
 ('public','open','onOpen','self.subscribe()?;','self.ping()?;','open'),
 ('public','ping','ping','"ping".into()','"bad-ping".into()','ping'),
 ('public','subscribe','subscribe','topics.chunks(50)','topics.chunks(1)','subscribe'),
 ('public','message','onMessage','if !truthy(&json)','if truthy(&json)','message'),
 ('public','instrument','handleInstrumentInfoEvent','100.','101.','instrument'),
 ('public','candle','listenOHLCV','volume: float(get(row, "volume"))?,','volume: float(get(row, "volume"))? * 2.,','candle'),
 ('public','book','listenOrderBook','safe_cex_store::orderbook::calc_order_book_total(&mut next);','next.bids.iter_mut().for_each(|level| level.total = 0.);','book'),
 ('private','class',None,'last_connect_time: Cell::new(0.)','last_connect_time: Cell::new(1.)','class'),
 ('private','ordersThrottle','throttleFetchOrders','1000,','1001,','orders_throttle'),
 ('private','positionsThrottle','throttleFetchPositions','1000,','1001,','positions_throttle'),
 ('private','connect','connectAndSubscribe','2 + usize::from(self.parent.base.options().testnet == Some(true))','3 - usize::from(self.parent.base.options().testnet == Some(true))','connect'),
 ('private','open','onOpen','self.last_connect_time.set(self.host.now_ms());','self.last_connect_time.set(self.host.now_ms() + 1.);','open'),
 ('private','message','onMessage','self.host.handle_pong(Some(5000))?;','self.host.handle_pong(Some(5001))?;','message'),
 ('private','ping','ping','5000,','5001,','ping'),
 ('private','accounts','handleWsAccountsV2Topic','["fills", "orders", "positions"]','["orders", "fills", "positions"]','accounts'),
 ('private','fills','handleFillTopic','self.parent.record_ws_execution(fill)?;','self.parent.record_ws_execution(if matches!(fill, J::Object(_)) { &J::Null } else { fill })?;','fills'),
 ('private','orders','handleOrderTopic','> 5000. && fresh','> 1e9 && fresh','orders'),
 ('private','positions','handlePositionTopic','.set(Some(self.host.now_ms()));','.set(Some(self.host.now_ms() + 1.));','positions'),
 ('private','auth','#auth','{}GET/ws/accounts','{}GET/ws/accountz','auth'),
 ('private','subscribe','#subscribe','ws_user_v1','ws_user_bad','subscribe'),
 ('private','dispose','dispose','self.orders_timer.get()','self.positions_timer.get()','dispose'),
]
assert len(controls)==24
base=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--test','exchange','--jobs','2']
try:
 selected=controls if len(sys.argv)==1 else controls[next(i for i,c in enumerate(controls)if c[0]+'.'+c[1]==sys.argv[1]):]
 for privacy,name,method,before,after,test in selected:
  filename='ws_'+privacy+'.rs';source=original[filename]
  if method is None:start=0;end=len(source)
  else:
   marker='// ts: ApexOmni'+privacy.title()+'Websocket'+('' if method.startswith('#') else '.')+method
   start=source.index(marker);end=source.find('// ts:',start+len(marker));end=len(source) if end<0 else end
  body=source[start:end];assert body.count(before)==1,(name,before,body.count(before))
  changed=source[:start]+body.replace(before,after)+source[end:]
  files[filename].write_text(changed)
  p=subprocess.run(base+['websocket_tests::ws_'+privacy+'_'+test,'--','--test-threads=1'],cwd=root,capture_output=True,text=True)
  output=p.stdout+p.stderr;(logs/(privacy+'-'+name+'.log')).write_text(output)
  label='ws '+privacy+' '+name+' native'
  assert p.returncode==101 and 'test result: FAILED.' in output and label in output,(privacy,name,p.returncode,output)
  files[filename].write_text(source)
  print(f'{privacy}.{name}: compiled; exit 101; {label}',flush=True)
finally:
 for name,path in files.items():path.write_text(original[name])
p=subprocess.run(base+['websocket_tests','--','--test-threads=1'],cwd=root,capture_output=True,text=True)
output=p.stdout+p.stderr;(logs/'restored.log').write_text(output)
assert p.returncode==0 and '25 passed' in output,output
for privacy,name,_,_,_,_ in controls:
 output=(logs/(privacy+'-'+name+'.log')).read_text()
 assert 'test result: FAILED.' in output and 'Finished `test` profile' in output and 'ws '+privacy+' '+name+' native' in output,(privacy,name)
print('24 body controls caught; zero survivors; restored 25 WebSocket tests exit 0',flush=True)
