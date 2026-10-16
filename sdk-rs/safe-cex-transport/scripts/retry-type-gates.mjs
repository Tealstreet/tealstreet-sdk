import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const retryFile = path.join(root, 'sdk-rs/safe-cex-transport/src/retry.rs');
const secondaryFile = path.join(root, 'sdk-rs/safe-cex-transport/src/secondary.rs');
const cargo = process.env.CARGO ?? 'cargo';
const logs = fs.mkdtempSync(path.join(os.tmpdir(), 'p2c-retry-types-'));
const args = ['test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-transport', '--doc'];
function run(name, extra) {
  const result = spawnSync(cargo, [...args, ...extra], { cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  fs.writeFileSync(path.join(logs, `${name}.log`), output);
  return { status: result.status, output };
}
function doctestIds() {
  const result = run('list', ['--', '--list']);
  assert.equal(result.status, 0, result.output);
  const ids = result.output.split('\n').filter(line => line.endsWith(': test')).map(line => line.slice(0, -6));
  const symbols = ['retry::RetryPolicy', 'retry::ReadRequest', 'retry::network_error_on_read', 'secondary::SecondaryKeyClients'];
  return symbols.map(symbol => {
    const selected = ids.filter(id => id.includes(` - ${symbol} `));
    assert.equal(selected.length, 1, `Expected exactly one ${symbol} doctest`);
    return symbol;
  });
}
const definitions = [
  ['policy-is-not-adapter', source => source + `\nimpl crate::RestAdapter for RetryPolicy {\n    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {\n        Box::pin(async { unreachable!() })\n    }\n}\n`, '/// fn adapter(policy: &RetryPolicy) -> &dyn RestAdapter { policy }'],
  ['read-token-is-private', source => source.replace('pub struct ReadRequest(RestRequest);', 'pub struct ReadRequest(pub RestRequest);'), '/// let read = ReadRequest(RestRequest { method: "POST".into(), ..Default::default() });'],
  ['write-cannot-construct-policy', source => source.replace('pub fn network_error_on_read(request: ReadRequest, retries: usize)', 'pub fn network_error_on_read(request: WriteRequest, retries: usize)').replace('    only_reads(request, retries, |_, result| {', '    only_reads(ReadRequest(request.0), retries, |_, result| {'), '/// fn retry_write(write: WriteRequest) { let _ = network_error_on_read(write, 3); }'],
  ['secondary-is-private', source => source.replace('    secondary: Arc<dyn RestAdapter>,', '    pub secondary: Arc<dyn RestAdapter>,'), '///     let _ = clients.secondary.execute(RestRequest { method: "POST".into(), ..Default::default() });'],
];
function verify(source, index, suffix = '') {
  const [name, mutate] = definitions[index];
  const filename = index === 3 ? secondaryFile : retryFile;
  const original = fs.readFileSync(filename, 'utf8');
  try {
    fs.writeFileSync(filename, source);
    const id = doctestIds()[index];
    const filter = id;
    const baseline = run(name + suffix + '-baseline', [filter]);
    assert.equal(baseline.status, 0, baseline.output);
    assert.match(baseline.output, /test result: ok\. 1 passed/, 'The selected doctest must run');
    const broken = mutate(source);
    assert.notEqual(broken, source, `${name}: invariant mutation did not apply`);
    fs.writeFileSync(filename, broken);
    const negative = run(name + suffix + '-negative', [filter]);
    // Removing this specific invariant must make this specific snippet compile.
    assert.notEqual(negative.status, 0, `${name}: invariant removal was masked by an unrelated error`);
    assert.match(negative.output, /Test compiled successfully, but it.s marked `compile_fail`/, negative.output);
    console.log(`${name}${suffix}: intended invariant proved`);
  } finally {
    fs.writeFileSync(filename, original);
  }
}
const originals = [retryFile, retryFile, retryFile, secondaryFile].map(filename => fs.readFileSync(filename, 'utf8'));
console.log(`Retry type logs: ${logs}`);
for (let index = 0; index < definitions.length; index++) {
  const original = originals[index];
  verify(original, index);
  const [name, , snippet] = definitions[index];
  const unrelated = original.replace(snippet, snippet + '\n/// let _: P2cUnrelatedMissingType;');
  assert.notEqual(unrelated, original);
  assert.throws(() => verify(unrelated, index, '-unrelated-error'), /invariant removal was masked by an unrelated error/);
  console.log(`${name}: unrelated-error false green rejected`);
}
const restored = run('restored', []);
assert.equal(restored.status, 0, restored.output);
console.log('retry doctests restored: passed');
