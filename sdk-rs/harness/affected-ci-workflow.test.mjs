import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { nativeMatrix } from './native-ci-checks.mjs';

const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const job = (text, name) => {
  const match = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:\\n|$(?![\\s\\S]))`, 'm').exec(text);
  assert(match, `missing ${name}`); return match[1];
};

function check(text) {
  assert.match(text, /push:\n    branches: \[feat\/rust-rewrite\]/);
  for (const name of ['bitget-paired', 'cpu-plan', 'cpu-second', 'cpu-certify', 'cpu-final']) {
    const value = job(text, name);
    assert.match(value, /^    if: .*github\.event_name != 'pull_request'/m, name);
    assert.match(value, /runs-on: ubuntu-latest/);
  }
  const gate = job(text, 'gate');
  assert.match(gate, /needs: \[selection, families, combined\]/);
  assert.doesNotMatch(gate.match(/^    needs:.*$/m)[0], /cpu-|bitget-paired/);
  assert.match(gate, /RUST_REPLAY_CORRECTNESS_ONLY: \$\{\{ github.event_name == 'pull_request' && 'true' \|\| 'false' \}\}/);
  assert.match(gate, /name: Seal approved CPU measurement inputs\n        if: github.event_name != 'pull_request'/);
  assert.match(gate, /name: Verify complete PR correctness coverage\n        if: always\(\) && github.event_name == 'pull_request'/);
  assert.match(gate, /verify-gates\.mjs --suite '\$\{\{ matrix.venue \}\}' --mode correctness/);
  assert.match(gate, /include: \$\{\{ fromJSON\(needs.selection.outputs.replay\) \}\}/);
  assert.match(gate, /name: wasm-family-\$\{\{ matrix.family \}\}/);
  assert.match(gate, /ci-wasm\.mjs --venue.*--verify/);
  assert.match(gate, /name: Credentialed lifecycle runner \(scripted transport only\)\n        if: matrix.venue == 'binance'\n        run: corepack yarn rust:test-live-smoke/);
  assert.doesNotMatch(gate, /rust:build-wasm|rust:test-harness/);
  assert.doesNotMatch(gate, /capability-feature-control/);
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
  assert.match(native, /include: \$\{\{ fromJSON\(needs.selection.outputs.native\) \}\}/);
  assert.match(native, /NATIVE_CRATES: \$\{\{ needs.selection.outputs.nativeCrates \}\}/);
  assert.match(job(text, 'result'), /if: always\(\)/);
  assert.match(job(text, 'result'), /CI_JOB_RESULTS: \$\{\{ toJSON\(needs\) \}\}/);
}

test('PR replay is affected-only and independent; CPU stays on calibrated landing runners', () => check(source));

test('removing a path/coverage guard, enabling PR CPU, or rebuilding replay refuses', () => {
  const mutations = [
    text => text.replace(job(text, 'bitget-paired'), job(text, 'bitget-paired').replace("if: github.event_name != 'pull_request'", 'if: always()')),
    text => text.replace(job(text, 'gate'), job(text, 'gate').replace('needs: [selection, families, combined]', 'needs: [selection, families, combined, cpu-certify]')),
    text => text.replace("RUST_REPLAY_CORRECTNESS_ONLY: ${{ github.event_name == 'pull_request' && 'true' || 'false' }}", "RUST_REPLAY_CORRECTNESS_ONLY: 'false'"),
    text => text.replace("if: needs.selection.outputs.sizeRequired == 'true'", 'if: always()'),
    text => text.replace('include: ${{ fromJSON(needs.selection.outputs.replay) }}', 'include: []'),
    text => text.replace('family: ${{ fromJSON(needs.selection.outputs.families) }}', 'family: [bitget]'),
    text => text.replace('--mode correctness', '--mode collect'),
    text => text.replace('--verify sdk-rs/hosts/wasm/pkg/ci-replay', '--archive sdk-rs/hosts/wasm/pkg/ci-replay'),
    text => text.replace('include: ${{ fromJSON(needs.selection.outputs.native) }}', 'include: []'),
    text => text.replace('run: corepack yarn rust:test-live-smoke', 'run: true'),
    text => text.replace('name: Lint the selected venue family with the pinned toolchain\n        working-directory: sdk-rs', 'name: Lint the selected venue family with the pinned toolchain\n        working-directory: .'),
    text => text.replace('      - name: Verify the same-run matching family capsule', '      - run: corepack yarn rust:build-wasm\n      - name: Verify the same-run matching family capsule'),
  ];
  for (const [index, mutate] of mutations.entries()) {
    const changed = mutate(source); assert.notEqual(changed, source); assert.throws(() => check(changed), `mutation ${index}`);
  }
});

test('engine modes partition all checks while leaf and docs PRs allocate only needed jobs', () => {
  const full = nativeMatrix(true, []);
  for (let i = 0; i < 4; i++) assert.equal(full.filter(row => row.command.includes(`--shard ${i}/4`)).length, 1);
  assert.deepEqual(nativeMatrix(false, ['bitget']).map(row => row.check), ['fmt', 'clippy', 'test']);
  assert.deepEqual(nativeMatrix(false, []).map(row => row.check), ['fmt']);
  assert(full.find(row => row.check === 'mutation-barriers').command.includes('plan_mutation_prepare_barrier'));
});
