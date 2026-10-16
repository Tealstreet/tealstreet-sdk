"""Sequential compiled controls; preserve raw failures and always restore source."""
import json, os, pathlib, subprocess, sys
root=pathlib.Path(__file__).resolve().parents[3]
os.chdir(root)
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
source=pathlib.Path('sdk-rs/safe-cex-venue-coinbase/src/exchange_lifecycle.rs')
original=source.read_text();out=pathlib.Path('/tmp/coinbase-lifecycle-controls')/sha;out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ)
controls=[
 ('dispose-order','lifecycle_live_native_corpus','dispose-empty: native call order','r.host.dispose_public()?;\n    r.host.dispose_private()','r.host.dispose_private()?;\n    r.host.dispose_public()'),
 ('account-user-id','lifecycle_live_native_corpus','account-cached: native account userId','Ok(Rc::new(RefCell::new(LifecycleAccount { user_id: uuid })))','Ok(Rc::new(RefCell::new(LifecycleAccount { user_id: format!("wrong-{uuid}") })))'),
 ('validation-own-presence','lifecycle_live_native_corpus','validate-ok: native balance own-property presence','has_balance: true,','has_balance: false,'),
 ('startup-unawaited-update','public_update_is_unawaited_but_initial_market_update_is_awaited','initial market update must be awaited','''                r.host
                    .update(fields([
                        ("markets", markets),
                        ("loaded", loaded(&r, &["markets"])),
                    ]))?
                    .await?;''','''                drop(r.host.update(fields([
                    ("markets", markets),
                    ("loaded", loaded(&r, &["markets"])),
                ]))?);'''),
 ('write-grace-equality','lifecycle_live_native_corpus','write-grace: independent source invocation count','> last_at_schedule','>= last_at_schedule'),
 ('reconciliation-update-root','lifecycle_live_native_corpus','reconcile-ok: update.fields.orders: source array length','let orders = r.host.merge_rest_orders_with_fresh_ws_state(orders)?;','let _ = r.host.merge_rest_orders_with_fresh_ws_state(orders.clone())?;'),
 ('reconciliation-late-clear','timer_callback_clears_handle_before_reentrant_effect','reconciliation clears handle before hot fetch effect','''            *callback_receiver.order_write_reconcile_timeout.borrow_mut() = None;
            drop(reconcile_open_orders_from_rest(
                callback_receiver.clone(),
                reason.clone(),
            ));''','''            drop(reconcile_open_orders_from_rest(
                callback_receiver.clone(),
                reason.clone(),
            ));
            *callback_receiver.order_write_reconcile_timeout.borrow_mut() = None;'''),
]
results=[]
for label,test,assertion,old,new in controls:
 if len(sys.argv)>1 and label!=sys.argv[1]:continue
 assert original.count(old)==1,(label,'unique mutation target')
 cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','exchange_lifecycle','--jobs','2',test,'--','--exact']
 try:
  source.write_text(original.replace(old,new))
  broken=subprocess.run(cmd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 finally:source.write_text(original)
 restored=subprocess.run(cmd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 result=dict(control=label,gate_sha=sha,target=str(source),test=test,assertion=assertion,command=cmd,mutation_exit=broken.returncode,restored_exit=restored.returncode,mutation_output=broken.stdout,restored_output=restored.stdout,source_restored=source.read_text()==original)
 results.append(result);(out/('results-'+sys.argv[1]+'.json' if len(sys.argv)>1 else 'results.json')).write_text(json.dumps(dict(sha=sha,controls=results),indent=2)+'\n')
 discriminates=broken.returncode==101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout and restored.returncode==0
 print(json.dumps(dict(control=label,mutation_exit=broken.returncode,restored_exit=restored.returncode,named_failure=assertion in broken.stdout,compiled='test result: FAILED' in broken.stdout)),flush=True)
 if not discriminates:raise SystemExit('CONTROL FINDING or instrumentation failure: '+label+'; raw outputs retained '+str(out))
