#!/usr/bin/env python3
"""Run alone: compile seven production faults, then restore exact bytes and replay.

Logs are written under sdk-rs/target/native-connecting-controls. Each RED must
compile and reach its named production contract; compiler/setup errors fail.
"""
import os, pathlib, subprocess
root=pathlib.Path(__file__).resolve().parents[2]
os.chdir(root)
logs=root/'target/native-connecting-controls'
logs.mkdir(parents=True,exist_ok=True)
p=root/'safe-cex-exchange/src/native.rs'
original=p.read_bytes()
s=original.decode()
os.environ['CARGO_INCREMENTAL']='0'
cases=[
 ('receiver-reentry','                let current = state.borrow().clone();','                let _control_borrow = state.borrow();\n                let current = state.borrow().clone();','native_connecting_waker_replacement_can_reenter_close','already borrowed'),
 ('missing-connecting-state','NativeSocketState::Connecting => 0,','NativeSocketState::Connecting => 3,','native_connecting_owner_drop_cancels_handshake_with_live_event_waiter','CONNECTING native handle exists during pending dial'),
 ('send-error-kind','error.name = "InvalidStateError".into();','error.name = "Error".into();','native_connecting_waiter_drop_releases_registration_without_canceling_dial','CONNECTING send preserves source InvalidStateError'),
 ('close-retains-io','                self.opening.close();','                /* control: retained dial */','native_connecting_waiter_drop_releases_registration_without_canceling_dial','CONNECTING close releases the actual pending dial synchronously'),
 ('close-loses-wake','                self.waiters.wake_all();','                /* control: missing receiver wake */','native_connecting_waiter_drop_releases_registration_without_canceling_dial','CONNECTING close wakes the registered event receiver'),
 ('final-drop-retains-io','        self.terminate();','        /* control: missing final teardown */','native_connecting_owner_drop_cancels_handshake_with_live_event_waiter','CONNECTING final handle drop suppresses queued failure events'),
 ('waiter-retains-waker','        let registration = self.waiters.register();','        let registration = Box::leak(Box::new(self.waiters.register()));','native_connecting_waiter_drop_releases_registration_without_canceling_dial','CONNECTING event waiter drop releases its registered waker immediately'),
]
def run(name,test):
 cmd=['cargo','test','--locked','--jobs','2','-p','safe-cex-exchange','--all-features','--lib',test,'--','--nocapture']
 out=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (logs/(name+'.log')).write_text(out.stdout)
 return out.returncode,out.stdout
try:
 for name,old,new,test,needle in cases:
  assert s.count(old)==1,(name,s.count(old))
  p.write_text(s.replace(old,new))
  status,output=run(name+'-red',test)
  assert status==101 and 'Finished `test`' in output and needle in output and 'error[E' not in output,(name,status,output[-3500:])
  print(name,'compiled named RED',flush=True)
  p.write_bytes(original)
  assert p.read_bytes()==original
  status,output=run(name+'-restored-green',test)
  assert status==0,(name,output[-3500:])
  print(name,'exact restore GREEN',flush=True)
finally:
 p.write_bytes(original)
 assert p.read_bytes()==original
