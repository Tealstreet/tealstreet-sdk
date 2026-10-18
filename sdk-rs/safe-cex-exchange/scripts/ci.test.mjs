import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, mkdtempSync, rmSync } from 'node:fs';
import test from 'node:test';

// See sdk-rs/scripts/rust-ci-wiring.test.mjs: the tier lives on the mirror.
const candidates = [
  '../../../mirrors/tealstreet-sdk/.github/workflows/rust.yml',
  '../../../.github/workflows/rust.yml',
];
const found = candidates.map((rel) => new URL(rel, import.meta.url)).find((url) => existsSync(url));
if (!found) throw new Error('no mirror workflow found');
const workflow = readFileSync(found, 'utf8');
const nativeCommands = [
  'node safe-cex-exchange/scripts/ci-deps.cjs "$RUNNER_TEMP/exchange-typescript-deps"',
  'corepack yarn --cwd "$RUNNER_TEMP/exchange-typescript-deps" install --frozen-lockfile --ignore-scripts --non-interactive',
  'export SAFE_CEX_TYPESCRIPT_PATH="$RUNNER_TEMP/exchange-typescript-deps/node_modules/typescript"',

  'node --test safe-cex-exchange/scripts/ci.test.mjs safe-cex-exchange/scripts/health-features.test.mjs',
  'node safe-cex-exchange/scripts/generate-health.cjs --check',
  'node safe-cex-exchange/scripts/generate-error-reporting.cjs --check',
  'node safe-cex-exchange/scripts/client-id-patterns.mjs',
  'cargo clippy --locked -p safe-cex-exchange --all-features --all-targets',
  'cargo test --locked -p safe-cex-exchange --all-features --all-targets -- --include-ignored',
  'cargo test --locked -p safe-cex-exchange --all-features --doc',
];
const wasmCommands = [
  'cargo build --locked -p safe-cex-exchange --target wasm32-unknown-unknown',
  'cargo build --locked -p safe-cex-exchange --all-features --target wasm32-unknown-unknown',
];
const escape = value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

function audit(source) {
  assert.doesNotMatch(source, /^\s*shell:/m, 'shell override may disable fail-fast execution');
  assert.match(source, /^defaults:\n  run:\n    working-directory: sdk-rs\n/m, 'commands require SDK working directory');
  assert.match(source, /^  RUSTFLAGS: -Dwarnings$/m, 'CI warning denial must remain enabled');
  const job = name => {
    const matches = [...source.matchAll(new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [\\w-]+:|$(?![\\s\\S]))`, 'gm'))];
    assert.equal(matches.length, 1, `exactly one ${name} job required`);
    assert.doesNotMatch(matches[0][1], /^    (?:if|continue-on-error):/m, `${name} job must remain mandatory`);
    return matches[0][1];
  };
  const step = (body, name, condition, workingDirectory) => {
    const matches = [...body.matchAll(new RegExp(`^      - name: ${escape(name)}\\n([\\s\\S]*?)(?=^      - |$(?![\\s\\S]))`, 'gm'))];
    assert.equal(matches.length, 1, `exactly one ${name} step required`);
    const block = matches[0][1];
    assert.doesNotMatch(block, /^        continue-on-error:/m, `${name} must not bypass execution`);
    const directories = [...block.matchAll(/^        working-directory: (.*)$/gm)];
    assert.deepEqual(directories.map(match => match[1]), workingDirectory === undefined ? [] : [workingDirectory], `${name} working directory`);
    const conditions = [...block.matchAll(/^        if: (.*)$/gm)];
    assert.deepEqual(conditions.map(match => match[1]), condition === undefined ? [] : [condition], `${name} condition must be reachable`);
    return block;
  };
  const exactCommands = (block, commands, label) => {
    assert.equal((block.match(/^        run:/gm) ?? []).length, 1, `${label} requires one run declaration`);
    const body = block.match(/^        run: \|\n((?:          [^\n]*\n?)+)/m)?.[1];
    const actual = body?.trimEnd().split('\n').map(line => line.slice(10));
    assert.deepEqual(actual, commands, `${label} must retain fail-fast commands`);
  };
  const native = job('native');
  const matrix = native.match(/^      matrix:\n([\s\S]*?)(?=^    steps:)/m)?.[1];
  assert.ok(matrix, 'native matrix required');
  assert.match(matrix, /^        include:\n(?:          - check: [a-z]+\n            command: [^\n]+\n)+$/, 'native matrix must be literal and have no exclusions');
  assert.match(matrix, /^          - check: test\n            command: cargo test --locked --no-fail-fast$/m, 'native test matrix arm must remain reachable');
  const setup = step(native, 'Set up Node for differential tests and crypto fixtures', "matrix.check == 'test'");
  assert.match(setup, /^        uses: actions\/setup-node@/m, 'native Node setup required');
  const dependencies = step(native, 'Install isolated TypeScript for routing oracle', "matrix.check == 'test'", '.');
  assert.match(dependencies, /SAFE_CEX_TYPESCRIPT_PATH=.*node_modules\/typescript/, 'live core generators require isolated TypeScript');
  const coreName = 'Check exchange core parity and lifecycle';
  const core = step(native, coreName, "matrix.check == 'test'");
  assert.ok(native.indexOf(`- name: ${coreName}`) > native.indexOf('- name: Install isolated TypeScript for routing oracle'), 'core checks must follow dependency installation');
  exactCommands(core, nativeCommands, 'native core gate');
  exactCommands(step(job('wasm'), 'Build exchange core wasm feature sets'), wasmCommands, 'wasm core gate');
  assert.match(source, /^on:\n/m, 'workflow events required');
  for (const event of ['push', 'pull_request']) {
    const matches = [...source.matchAll(new RegExp(`^  ${event}:\\n([\\s\\S]*?)(?=^  [\\w-]+:|^permissions:)`, 'gm'))];
    assert.equal(matches.length, 1, `${event} trigger required`);
    const block = matches[0][1];
    for (const path of ['sdk-rs/**', 'packages/safe-cex/**', 'thirdparty/reconnecting-websocket/**', 'mirrors/tealstreet-sdk/.github/workflows/rust.yml']) {
      assert.ok(block.split('\n').includes(`      - '${path}'`), `${event} trigger missing ${path}`);
    }
    if (event === 'push') {
      const branches = block.match(/^    branches: \[([^\]]+)\]$/m)?.[1]?.split(',').map(branch => branch.trim());
      assert.ok(branches?.includes('master'), 'master pushes must trigger the workflow');
    }
  }
}

test('mirror CI runs reachable live core gates and both wasm feature sets', () => audit(workflow));
for (const command of nativeCommands) {
  test(`removing core command fails: ${command}`, () => {
    assert.throws(() => audit(workflow.replace(`          ${command}\n`, '')), /native core gate must retain fail-fast commands/);
  });
}
const testCommand = nativeCommands.find(command => command.includes('--include-ignored'));
const clippyCommand = nativeCommands.find(command => command.startsWith('cargo clippy'));
const docCommand = nativeCommands.find(command => command.endsWith('--doc'));
for (const [label, before, after] of [
  ['ignored oracle flag', testCommand, testCommand.replace(' -- --include-ignored', '')],
  ['all-features test flag', testCommand, testCommand.replace(' --all-features', '')],
  ['all-targets test flag', testCommand, testCommand.replace(' --all-targets', '')],
  ['all-features clippy flag', clippyCommand, clippyCommand.replace(' --all-features', '')],
  ['all-features docs flag', docCommand, docCommand.replace(' --all-features', '')],
  ['failure mask', testCommand, `${testCommand} || true`],
  ['shell error suppression', nativeCommands[0], `set +e\n          ${nativeCommands[0]}\n          exit 0`],
]) {
  test(`rejects ${label}`, () => assert.throws(() => audit(workflow.replace(before, after)), /native core gate must retain fail-fast commands/));
}
for (const command of wasmCommands) {
  test(`removing wasm command fails: ${command}`, () => assert.throws(() => audit(workflow.replace(`          ${command}\n`, '')), /wasm core gate must retain fail-fast commands/));
}
test('removing test matrix arm fails', () => assert.throws(() => audit(workflow.replace('          - check: test\n            command: cargo test --locked --no-fail-fast\n', '')), /native test matrix arm must remain reachable/));
test('dropping the full failure inventory flag fails', () => assert.throws(() => audit(workflow.replace('command: cargo test --locked --no-fail-fast', 'command: cargo test --locked')), /native test matrix arm must remain reachable/));
test('unreachable core step fails', () => assert.throws(() => audit(workflow.replace('      - name: Check exchange core parity and lifecycle\n        if: matrix.check == \'test\'', '      - name: Check exchange core parity and lifecycle\n        if: false')), /condition must be reachable/));
test('native job bypass fails', () => assert.throws(() => audit(workflow.replace('  native:\n', '  native:\n    if: false\n')), /native job must remain mandatory/));
test('step failure suppression fails', () => assert.throws(() => audit(workflow.replace('      - name: Check exchange core parity and lifecycle\n', '      - name: Check exchange core parity and lifecycle\n        continue-on-error: true\n')), /must not bypass execution/));
test('shell override fails', () => assert.throws(() => audit(workflow.replace('defaults:\n  run:\n', 'defaults:\n  run:\n    shell: bash {0}\n')), /shell override may disable fail-fast execution/));
test('duplicate run declarations fail', () => assert.throws(() => audit(workflow.replace('          node --test safe-cex-exchange/scripts/ci.test.mjs safe-cex-exchange/scripts/health-features.test.mjs\n', '          node --test safe-cex-exchange/scripts/ci.test.mjs safe-cex-exchange/scripts/health-features.test.mjs\n        run: true\n')), /requires one run declaration/));
test('removing master branch trigger fails', () => assert.throws(() => audit(workflow.replace('branches: [master, ', 'branches: [')), /master pushes must trigger/));
for (const event of ['push', 'pull_request']) {
  test(`narrowing ${event} source triggers fails`, () => {
    const start = workflow.indexOf(`  ${event}:\n`);
    const broken = workflow.slice(0, start) + workflow.slice(start).replace("'packages/safe-cex/**'", "'packages/safe-cex/src/types.ts'");
    assert.throws(() => audit(broken), new RegExp(`${event} trigger missing`));
  });
}

test('isolated core dependencies include the real JWT codec under repository frozen lock', () => {
  const target = mkdtempSync(join(tmpdir(), 'exchange-ci-deps-'));
  try {
    const result = spawnSync(process.execPath, [new URL('./ci-deps.cjs', import.meta.url).pathname, target], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const projected = JSON.parse(readFileSync(join(target, 'package.json'), 'utf8'));
    const root = JSON.parse(readFileSync(new URL('../../../package.json', import.meta.url), 'utf8'));
    const sdk = JSON.parse(readFileSync(new URL('../../../packages/safe-cex/package.json', import.meta.url), 'utf8'));
    const check = manifest => {
      assert.equal(manifest.dependencies["tiny-emitter"], sdk.dependencies["tiny-emitter"], "listener oracle requires real TinyEmitter");
      assert.equal(manifest.dependencies.nanoid, sdk.dependencies.nanoid, "whole remote adapter requires real nanoid");
      assert.equal(manifest.dependencies.axios, sdk.dependencies.axios, "live request transform requires Axios");
      assert.equal(manifest.dependencies.typescript, root.devDependencies.typescript);
      assert.equal(manifest.dependencies.jsonwebtoken, sdk.dependencies.jsonwebtoken, 'real JWT dependency required');
      assert.equal(manifest.dependencies['axios-retry'], sdk.dependencies['axios-retry'], 'live network retry predicate required');
      assert.equal(manifest.dependencies.lodash, sdk.dependencies.lodash, 'live CrossTrade mapping requires Lodash');
      assert.equal(manifest.dependencies.qs, sdk.dependencies.qs, 'live query-options oracle requires qs');
    };
    check(projected);
    const missingEmitter = structuredClone(projected);
    delete missingEmitter.dependencies["tiny-emitter"];
    assert.throws(() => check(missingEmitter), /listener oracle requires real TinyEmitter/);
    const missingNanoid = structuredClone(projected);
    delete missingNanoid.dependencies.nanoid;
    assert.throws(() => check(missingNanoid), /whole remote adapter requires real nanoid/);
    const missingAxios = structuredClone(projected);
    delete missingAxios.dependencies.axios;
    assert.throws(() => check(missingAxios), /live request transform requires Axios/);
    assert.equal(readFileSync(join(target, 'yarn.lock'), 'utf8'), readFileSync(new URL('../../../yarn.lock', import.meta.url), 'utf8'));
    const broken = structuredClone(projected);
    delete broken.dependencies.jsonwebtoken;
    assert.throws(() => check(broken), /real JWT dependency required/);
    const missingRetry = structuredClone(projected);
    delete missingRetry.dependencies['axios-retry'];
    assert.throws(() => check(missingRetry), /live network retry predicate required/);
    const missingLodash = structuredClone(projected);
    delete missingLodash.dependencies.lodash;
    assert.throws(() => check(missingLodash), /live CrossTrade mapping requires Lodash/);
    const missingQs = structuredClone(projected);
    delete missingQs.dependencies.qs;
    assert.throws(() => check(missingQs), /live query-options oracle requires qs/);
  } finally {
    rmSync(target, { recursive: true, force: true });
  }
});

for (const event of ['push', 'pull_request']) {
  test(`removing ${event} RWS oracle source trigger fails`, () => {
    const start = workflow.indexOf(`  ${event}:`);
    const broken = workflow.slice(0, start) + workflow.slice(start).replace("      - 'thirdparty/reconnecting-websocket/**'\n", '');
    assert.throws(() => audit(broken), new RegExp(`${event} trigger missing thirdparty/reconnecting-websocket`));
  });
}
