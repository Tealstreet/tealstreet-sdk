from pathlib import Path
import subprocess,os
base=Path.cwd()/'sdk-rs/safe-cex-webworker';env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
cases=[
('position-full',base/'src/positions.rs','.add_or_update_positions_full(positions)','.add_or_update_positions(positions)','full_empty_refresh_clears_omitted_positions_but_scoped_empty_is_partial'),
('command-reqid',base/'src/command.rs','if let Some(id) = req_id {','if let Some(id) = Some(req_id.unwrap_or_default()) {','command_failures_echo_only_present_reqid_and_preserve_executor_results'),
('throttle-boundary',base/'src/throttle.rs','now - last >= self.delay as f64','now - last > self.delay as f64','source_older_tail_survives_new_leading_and_clear'),
('market-cooldown',base/'src/handler.rs','now - last >= 300_000.0','now - last >= 0.0','market_buffer_timeout_cooldown_reset_and_disposal_are_independent')]
for name,p,old,new,test in cases:
 original=p.read_text();assert old in original,name;p.write_text(original.replace(old,new,1))
 try:
  run=subprocess.run(['cargo','test','-p','safe-cex-webworker',test],cwd=base.parent,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  Path('/tmp/p6-control-'+name+'.log').write_text(run.stdout);print(name,run.returncode,flush=True)
  assert run.returncode==101 and 'test result: FAILED' in run.stdout,run.stdout[-1500:]
 finally:p.write_text(original)
