import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const job = (text, name) => {
  const match = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:\\n|$(?![\\s\\S]))`, 'm').exec(text);
  assert(match, `missing ${name}`); return match[1];
};

function check(text) {
  assert.match(text, /push:\n    branches: \[feat\/rust-rewrite\]/);
  for (const name of ['gate', 'bitget-paired', 'cpu-plan', 'cpu-second', 'cpu-certify', 'cpu-final']) {
    const value = job(text, name);
    assert.match(value, /runs-on: ubuntu-latest/);
    assert.doesNotMatch(value, /github\.event_name != 'pull_request'|continue-on-error/);
  }
  const gate = job(text, 'replay');
  assert.match(gate, /needs: \[selection, families, combined\]/);
  assert.doesNotMatch(gate.match(/^    needs:.*$/m)[0], /cpu-|bitget-paired/);
  assert.match(gate, /RUST_REPLAY_CORRECTNESS_ONLY: 'true'/);
  assert.match(gate, /RUST_REPLAY_CI_WASM: sdk-rs\/hosts\/wasm\/pkg\/ci-replay/);
  assert.doesNotMatch(gate, /--runs|RUST_REPLAY_PUBLIC_RUNS|cpu-ci\.mjs|cpu_prepare|cpu_collect/);
  assert.match(gate, /name: Verify complete correctness coverage\n        if: always\(\)/);
  assert.match(gate, /verify-gates\.mjs --suite '\$\{\{ matrix.venue \}\}' --mode correctness/);
  assert.match(gate, /include: \$\{\{ fromJSON\(needs.selection.outputs.replay\) \}\}/);
  assert.match(gate, /name: wasm-family-\$\{\{ matrix.family \}\}/);
  assert.match(gate, /ci-wasm\.mjs --venue.*--verify/);
  assert.match(gate, /name: Credentialed lifecycle runner \(scripted transport only\)\n        if: matrix.venue == 'binance'\n        run: corepack yarn rust:test-live-smoke/);
  assert.doesNotMatch(gate, /rust:build-wasm|rust:test-harness|capability-feature-control/);
  assert.doesNotMatch(text, /ref: \$\{\{ needs.selection.outputs.checkoutSha/);
  const required = job(text, 'result').match(/^    needs:.*$/m)[0];
  assert.match(required, /replay/); assert.doesNotMatch(required, /cpu-|bitget-paired|browser|, gate/);
  // Parallel combined producers must not publish the same immutable artifact.
  assert.match(job(text, 'combined'), /name: browser-wasm-combined/);
  assert.match(job(text, 'gate'), /name: measured-browser-wasm-combined/);
  assert.doesNotMatch(job(text, 'gate'), /name: browser-wasm-combined/);
  const families = job(text, 'families');
  assert.match(families, /if: needs.selection.outputs.sizeRequired == 'true'/);
  assert.match(families, /family: \$\{\{ fromJSON\(needs.selection.outputs.families\) \}\}/);
  assert.match(families, /rust:build-wasm --venue '\$\{\{ matrix.family \}\}'/);
  assert.match(families, /name: Lint the selected venue family with the pinned toolchain\n        working-directory: sdk-rs\n        run: cargo clippy --locked/);
  assert.doesNotMatch(families, /cargo clippy --manifest-path/);
  assert.match(families, /name: wasm-family-\$\{\{ matrix.family \}\}/);
  assert.match(families, /if-no-files-found: error/);
  assert.match(families, /name: MEXC missing command-feature negative control\n        if: matrix.family == 'mexc'\n        run: corepack yarn -s node sdk-rs\/harness\/capability-feature-control\.mjs/);
  assert(families.indexOf('uses: ./.github/actions/install-binaryen') < families.indexOf('capability-feature-control.mjs'));
  assert(families.indexOf('cargo install wasm-bindgen-cli') < families.indexOf('capability-feature-control.mjs'));
  assert.match(job(text, 'wasm'), /if: needs.selection.outputs.nativeFull == 'true'/);
  assert.match(job(text, 'combined'), /if: needs.selection.outputs.combinedRequired == 'true'/);
  assert.match(job(text, 'combined'), /CHANGED_INTEGRATION_TESTS: \$\{\{ needs.selection.outputs.integrationTests \}\}/);
  assert.match(job(text, 'combined'), /\['--import', 'tsx', '--test', \.\.\.tests\]/);
  const native = job(text, 'native');
  assert.match(native, /cargo test --locked --workspace/);
  assert.match(native, /python3 scripts\/test-http-plan-feature-modes.py/);
  assert.match(native, /check: mutation-barriers/);
  assert.match(job(text, 'result'), /if: always\(\)/);
  assert.match(job(text, 'result'), /CI_JOB_RESULTS: \$\{\{ toJSON\(needs\) \}\}/);
}

test('PR replay is affected-only and independent; CPU stays on calibrated landing runners', () => check(source));

test('correctness commands cannot receive measurement flags or wait for CPU', () => {
  for (const [before, after] of [
    ["RUST_REPLAY_CORRECTNESS_ONLY: 'true'", "RUST_REPLAY_CORRECTNESS_ONLY: 'false'"],
    ['--mode correctness', '--mode collect'],
    ['needs: [selection, families, combined]', 'needs: [selection, families, combined, cpu-certify]'],
    ["if: needs.selection.outputs.sizeRequired == 'true'", 'if: always()'],
    ['include: ${{ fromJSON(needs.selection.outputs.replay) }}', 'include: []'],
    ['family: ${{ fromJSON(needs.selection.outputs.families) }}', 'family: [bitget]'],
    ['--verify sdk-rs/hosts/wasm/pkg/ci-replay', '--archive sdk-rs/hosts/wasm/pkg/ci-replay'],
    ['run: corepack yarn rust:test-live-smoke', 'run: true'],
  ]) {
    const changed = source.replaceAll(before, after); assert.notEqual(changed, source); assert.throws(() => check(changed));
  }
  assert.throws(() => check(source.replace(job(source, 'replay'), job(source, 'replay').replace('--books', '--runs 9 --books'))));
});
