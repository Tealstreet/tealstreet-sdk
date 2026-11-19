import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { cargoFailures, commandLines, summarize } from './run-all-checks.mjs';

// Captured from cargo 1.92 `cargo test --no-fail-fast --workspace` (trimmed).
const cargoOutput = `     Running unittests src/lib.rs (target/debug/deps/a_crate-0013b4f8ed64625c)
running 3 tests
test ok ... ok
test tests::bad ... FAILED
test tests::bad2 ... FAILED

failures:
    tests::bad
    tests::bad2

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass \`-p a-crate --lib\`
     Running tests/integ.rs (target/debug/deps/integ-c31cc00377497c7d)
test integ_bad ... FAILED
error: test failed, to rerun pass \`-p a-crate --test integ\`
     Running unittests src/lib.rs (target/debug/deps/b-727eca0b15715b4f)
error: test failed, to rerun pass \`-p b --lib\`
   Doc-tests demo
test src/lib.rs - d (line 17) ... FAILED
error: doctest failed, to rerun pass \`--doc\`
error: 4 targets failed:
    \`-p a-crate --lib\`
    \`-p a-crate --test integ\`
    \`-p b --lib\`
    \`--doc\``;

test('every failing test is attributed to its crate and target', () => {
  const { crates, compile } = cargoFailures(cargoOutput);
  assert.deepEqual(compile, []);
  assert.deepEqual([...crates.get('a-crate')], [['--lib', ['tests::bad', 'tests::bad2']], ['--test integ', ['integ_bad']]]);
  assert.deepEqual([...crates.get('b')], [['--lib', ['(binary exited without per-test results)']]]);
  assert.deepEqual([...crates.get('(root package)')], [['--doc', ['src/lib.rs - d (line 17)']]]);
});

test('compile errors are reported even though no test ran', () => {
  const { compile } = cargoFailures('error: could not compile `safe-cex-engine` (lib test) due to 2 previous errors');
  assert.deepEqual(compile, ['safe-cex-engine (lib test)']);
});

test('summary lists only failed commands', () => {
  const summary = summarize([
    { command: 'cargo fmt --all --check', code: 0, output: '' },
    { command: 'cargo test --no-fail-fast', code: 101, output: cargoOutput },
  ]);
  assert.match(summary, /^Fail-complete summary: 1 of 2 command\(s\) failed/);
  assert.doesNotMatch(summary, /cargo fmt/);
  assert.match(summary, /a-crate --lib: 2 failing\n {4}tests::bad\n {4}tests::bad2/);
});

test('commands split per line; comments and continuations are handled explicitly', () => {
  assert.deepEqual(commandLines('# note\n  a --x\n\nb\n'), ['a --x', 'b']);
  assert.throws(() => commandLines('a \\\n  b'), /continuation/);
  assert.throws(() => commandLines('# only a comment'), /no commands/);
});

test('later commands still run after a failure and the runner still fails', () => {
  const summaryFile = join(mkdtempSync(join(tmpdir(), 'run-all-checks-')), 'summary.md');
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./run-all-checks.mjs', import.meta.url))], {
    env: { ...process.env, CHECK_COMMANDS: 'false\necho second-ran\nexit 3', GITHUB_STEP_SUMMARY: summaryFile, CHECK_NAME: 'demo' },
    encoding: 'utf8',
  });
  assert.equal(result.status, 1);
  assert.match(result.stdout, /second-ran/);
  assert.match(result.stdout, /2 of 3 command\(s\) failed/);
  assert.match(readFileSync(summaryFile, 'utf8'), /### demo[\s\S]*FAILED \(exit 3\): exit 3/);
});
