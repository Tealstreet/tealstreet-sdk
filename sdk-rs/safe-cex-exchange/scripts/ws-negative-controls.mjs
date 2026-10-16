import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
const crate = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const file = path.join(crate, 'src/websocket.rs');
const source = fs.readFileSync(file, 'utf8');
const packageName = fs.readFileSync(path.join(crate, 'Cargo.toml'), 'utf8').match(/^name = "([^"]+)"/m)[1];
const run = (test, env = {}) => spawnSync('cargo', ['test', '--locked', '-p', packageName, '--lib', `websocket::tests::${test}`, '--', '--include-ignored'], { cwd: path.dirname(crate), encoding: 'utf8', env: {...process.env,...env} });
const controls = [
  { name: 'send disposal guard', test: 'send_drops_only_heartbeats_and_disposal_never_reconnects', mutate: text => {
    const start = text.indexOf('pub fn send(self: &Rc<Self>');
    const end = text.indexOf('pub fn is_connected', start);
    const section = text.slice(start,end);
    if (!section.includes('if self.is_disposed()')) throw new Error('send mutation target vanished');
    return text.slice(0,start) + section.replace('if self.is_disposed()', 'if false') + text.slice(end);
  } },
  { name: 'pending reopen rerun', test: 'reopen_resync_coalesces_siblings_but_not_same_socket_and_one_pending_followup', mutate: text => text.replace('state.rerun = true;', 'state.rerun = false;') },
  { name: 'diagnostic frame cap', test: 'diagnostics_are_opt_in_redacted_rate_limited_and_utf16_capped', mutate: text => text.replace('s.diagnostic_frames <= 200', 's.diagnostic_frames <= 201') },
];
try {
  for (const control of controls) {
    const mutated = control.mutate(source);
    if (mutated === source) throw new Error(`${control.name}: mutation did not apply`);
    fs.writeFileSync(file, mutated);
    const result = run(control.test);
    const output = result.stdout + result.stderr;
    if (result.status === 0 || !output.includes(`websocket::tests::${control.test} ... FAILED`) || !output.includes('assertion')) throw new Error(`${control.name}: did not fail its intended assertion\n${output}`);
    fs.writeFileSync(file, source);
    const restored = run(control.test);
    if (restored.status !== 0) throw new Error(`${control.name}: restored control failed\n${restored.stdout}${restored.stderr}`);
    console.log(`${control.name}: intended assertion failed; restored test passed`);
  }
  const test = 'live_ws_typescript_oracle';
  const failed = run(test,{SAFE_CEX_WS_ORACLE_NEGATIVE_CONTROL:'heartbeat'});
  const output=failed.stdout+failed.stderr;
  if(failed.status===0||!output.includes(`websocket::tests::${test} ... FAILED`)||!output.includes('"input":"ping"'))throw new Error(`live source mutation did not fail on ping\n${output}`);
  const passed=run(test);if(passed.status!==0)throw new Error(`restored live oracle failed\n${passed.stdout}${passed.stderr}`);
  console.log('live TypeScript heartbeat mutation: ping disagreement failed; restored source oracle passed');
} finally {
  fs.writeFileSync(file, source);
}
// Prove the production wasm target build rejects a native-only dependency.
try {
  fs.writeFileSync(file, source + '\npub fn wasm_negative_control(_: std::os::unix::net::UnixStream) {}\n');
  const broken = spawnSync('cargo', ['build','--locked','-p',packageName,'--target','wasm32-unknown-unknown'], {cwd:path.dirname(crate),encoding:'utf8'});
  if(broken.status===0 || !(broken.stdout+broken.stderr).includes('could not find `unix` in `os`'))throw new Error(`wasm control failed for an unrelated reason\n${broken.stdout}${broken.stderr}`);
  fs.writeFileSync(file,source);
  const restored=spawnSync('cargo',['build','--locked','-p',packageName,'--target','wasm32-unknown-unknown'],{cwd:path.dirname(crate),encoding:'utf8'});
  if(restored.status!==0)throw new Error(`restored wasm gate failed\n${restored.stdout}${restored.stderr}`);
  console.log('native-only Unix type: wasm target rejected it; restored target build passed');
} finally {fs.writeFileSync(file,source);}
