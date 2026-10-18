import './parity-toolchain.test.mjs';
// The existing CI audit also executes the live Cargo layering suite.
import './check-layers.test.mjs';
import '../safe-cex-types/tests/ci-deps.test.cjs';
import '../safe-cex-venue-binance/scripts/types-toolchain.test.mjs';
import '../safe-cex-parity/scripts/review-contract-proposals.test.mjs';
import assert from 'node:assert/strict';
import fs, { existsSync, readFileSync } from 'node:fs';
import test from 'node:test';
import path from 'node:path';
import os from 'node:os';
import vm from 'node:vm';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { checkExtractorDependency } from '../parity/extract-ts/scripts/check-dependency.mjs';
import { auditExtractorInstallWiring } from '../parity/extract-ts/scripts/clean-install-wiring.mjs';

/**
 * The Rust tier runs in ONE place: `Tealstreet/tealstreet-sdk`, the public
 * mirror, where the Actions minutes are free. This audit lives in `sdk-rs`, so
 * it travels with the code and runs in whichever repo holds the slice.
 *
 * `mirrors/tealstreet-sdk/.github/workflows/rust.yml` is the monorepo-authored
 * path; `.github/workflows/ci.yml` is that same file from inside the mirror.
 * Whichever exists is audited, and if neither does the audit FAILS rather than
 * skipping — a gate running nowhere is the state this file exists to prevent.
 * `tealstreet-next` no longer has a Rust workflow at all, by design.
 */
// The tier runs on the public mirror; this repo has no Rust workflow. Reads the
// monorepo-authored path first, then the same file as seen from inside the
// mirror, and fails rather than skipping if neither is there.
let workflowUrlUsed;
const workflowSource = (depth) => {
  const candidates = [
    `${'../'.repeat(depth)}mirrors/tealstreet-sdk/.github/workflows/rust.yml`,
    `${'../'.repeat(depth)}.github/workflows/rust.yml`,
  ];
  const hit = candidates.map((rel) => new URL(rel, import.meta.url)).find((url) => existsSync(url));
  if (!hit) throw new Error(`no mirror workflow found: ${candidates.join(', ')}`);
  workflowUrlUsed = hit;
  return readFileSync(hit, 'utf8');
};

const workflow = workflowSource(2);


/**
 * The scripts must come from the SAME repo context as the workflow. Inside the
 * mirror that is the root manifest; in the monorepo, where the tier's scripts
 * were removed with the workflow, it is the scaffold that BECOMES that manifest.
 */
const manifestFor = (workflowUrl) =>
  workflowUrl.pathname.includes('/mirrors/tealstreet-sdk/')
    ? new URL('../../mirrors/tealstreet-sdk/package.json', import.meta.url)
    : new URL('../../package.json', import.meta.url);
const packageScripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
function audit(source, scripts = packageScripts) {
  const pushBranches = source.match(/^  push:\n    branches: \[([^\n]+)\]$/m)?.[1];
  assert.deepEqual(pushBranches?.split(',').map(s => s.trim().replace(/^['"]|['"]$/g, '')),
    ['master', 'feat/safe-cex-rust'],
    'push branches must retain only master and feat/safe-cex-rust; PRs cover feature branches');
  // Rely on GitHub's fail-fast default shell; overrides require an audited change.
  assert.doesNotMatch(source, /^\s*shell:/m, 'shell override may disable fail-fast execution');
  const job = (name) => {
    const body = source.match(new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [\\w-]+:|$(?![\\s\\S]))`, 'm'))?.[1];
    assert.ok(body, `missing ${name} job`);
    assert.doesNotMatch(body, /^    (?:if|continue-on-error):/m, `${name} job may not bypass gates`);
    return body;
  };
  const step = (body, name, command, condition) => {
    const block = body.match(new RegExp(`^      - name: ${name}\\n([\\s\\S]*?)(?=^      - |$(?![\\s\\S]))`, 'm'))?.[1];
    assert.ok(block, `missing ${name}`);
    assert.equal((block.match(/^        run:/gm) ?? []).length, 1, `${name} must have one run declaration`);
    assert.doesNotMatch(block, /^        continue-on-error:/m, `${name} may not continue on error`);
    const conditions = block.match(/^        if: (.*)$/m);
    assert.equal(conditions?.[1], condition, `${name} condition`);
    assert.match(block, command, `${name} command`);
    return block;
  };
  step(job('types'), 'Check live KuCoin TypeScript equivalence', /^        run: yarn venue-oracle:kucoin$/m);
  const native = job('native');
  // Validate the worker prerequisite before cancellation's downstream step.
  step(native, 'Install isolated worker oracle dependencies', /^        run: \|$/m, "matrix.check == 'test'");
  const cancellation = step(native, 'Audit cancellable future classifications', /node --test safe-cex-exchange\/scripts\/cancellable-futures\.test\.cjs/, "matrix.check == 'test'");
  assert.equal(cancellation, "        if: matrix.check == 'test'\n        run: |\n          node --test safe-cex-exchange/scripts/cancellable-futures.test.cjs\n          cargo test --locked -p safe-cex-exchange --all-features --test cancellable_future_inventory --jobs 2\n", 'cancellation classification must execute both exact mandatory commands');
  assert.equal((native.match(/^      - name: Audit cancellable future classifications$/gm) ?? []).length, 1, 'cancellation classification must run exactly once');
  const matrix = native.match(/^      matrix:\n([\s\S]*?)(?=^    steps:)/m)?.[1];
  assert.ok(matrix, 'native test matrix missing');
  assert.match(matrix, /^        include:\n(?:          - check: [a-z]+\n            command: [^\n]+\n)+$/, 'native matrix must be a literal include without exclusions');
  assert.match(matrix, /^          - check: test\n            command: cargo test --locked --no-fail-fast$/m, 'native matrix must satisfy matrix.check == test');
  const exactRun = (block, commands, label) => {
    assert.equal((block.match(/^        run:/gm) ?? []).length, 1, `${label} must have one run declaration`);
    const actual = block.match(/^        run: \|\n((?:          [^\n]*\n?)+)/m)?.[1]?.trimEnd().split('\n').map(line => line.slice(10));
    assert.deepEqual(actual, commands, `${label} must retain fail-fast commands`);
  };
  const parity = job('types');
  // Coinbase tables/types must stay a mandatory live-source oracle after isolated TS setup.
  auditCryptocom(source);
  const coinbase = step(parity, 'Check Coinbase live TypeScript oracle', /^        run: corepack yarn venue-oracle:coinbase$/m);
  assert.match(coinbase, /^        working-directory: \.$/m);
  assert.doesNotMatch(coinbase, /^        env:/m, 'Coinbase oracle may not override environment');
  assert.ok(parity.indexOf('- name: Install isolated oracle dependencies from the repo lockfile') < parity.indexOf('- name: Check Coinbase live TypeScript oracle'), 'Coinbase oracle must follow isolated TypeScript setup');
  assert.match(step(parity, 'Check mexc live TypeScript oracle', /^        run: yarn venue-oracle:mexc$/m), /^        working-directory: \.$/m);
  assert.equal((source.match(/      - 'packages\/safe-cex\/src\/exchanges\/mexc\/\*\*'/g) ?? []).length, 2, 'mexc source triggers');
  assert.match(parity, /fetch-depth: 0\n          filter: blob:none/, 'accepted-set ratchet requires complete commit history');
  for (const [name, command] of [
    ['Check Bitget TypeScript equivalence', /^        run: yarn venue-oracle:bitget$/m],
    ['Check live lighter TypeScript oracle', /^        run: yarn venue-oracle:lighter$/m],
    ['Check parity manifest coverage', /^        run: yarn parity:extract --check$/m],
    ['Check live registry TypeScript equivalence', /^        run: yarn registry:check$/m],
    ['Check apexomni live TypeScript conformance', /^        run: yarn venue-oracle:apexomni$/m],
    ['Check live worker TypeScript equivalence', /^        run: \|\n          node sdk-rs\/safe-cex-webworker\/ci-deps\.cjs "\$RUNNER_TEMP\/worker-oracle-deps"\n          corepack yarn --cwd "\$RUNNER_TEMP\/worker-oracle-deps" install --frozen-lockfile --ignore-scripts --non-interactive\n          export SAFE_CEX_WORKER_LODASH_PATH="\$RUNNER_TEMP\/worker-oracle-deps\/node_modules\/lodash"\n          yarn worker-oracle$/m],
    ['Check venue oracle binance', /^        run: yarn venue-oracle:binance$/m],
    ['Check live Asterdex TypeScript equivalence', /^        run: yarn venue-oracle:asterdex$/m],
    ['Check WOO venue TypeScript oracle', /^        run: yarn venue-oracle:woo$/m],
    ['Check production accepted sets', /^        run: yarn parity:accepted$/m],
    ['Check accepted-set negative controls', /^        run: yarn parity:accepted:controls$/m],
    ['Check parity invention and status', /^        run: yarn parity:check$/m],
  ]) assert.match(step(parity, name, command), /^        working-directory: \.$/m);
  const binance = step(parity, 'Check venue oracle binance', /^        run: yarn venue-oracle:binance$/m);
  assert.doesNotMatch(binance, /^        env:/m, 'Binance oracle may not override environment');
  assert.ok(parity.indexOf('- name: Install isolated oracle dependencies from the repo lockfile') < parity.indexOf('- name: Check venue oracle binance'), 'Binance oracle must follow isolated TypeScript installation');
  assert.ok(parity.indexOf('- name: Install dependencies for manifest coverage') < parity.indexOf('- name: Check apexomni live TypeScript conformance'), 'apexomni oracle must follow root install');
  assert.equal(scripts['venue-oracle:apexomni'], 'cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --test protocol --test records --test utils --test signer_utils --test transaction_records --test pure --test signing --test exchange --jobs 1', 'Apexomni oracle must retain all eight test targets');
  assert.equal((source.match(/      - 'packages\/safe-cex\/src\/exchanges\/apexomni\/\*\*'/g) ?? []).length, 2, 'apexomni source triggers required');
  step(parity, 'Check live model and production store seams', /^        run: cargo test --locked -p safe-cex-types -p safe-cex --all-targets -- --include-ignored$/m);
  assert.ok(parity.indexOf('- name: Install dependencies for manifest coverage') >= 0 && parity.indexOf('- name: Install dependencies for manifest coverage') < parity.indexOf('- name: Check live registry TypeScript equivalence'), 'registry oracle must follow root dependency installation');
  assert.ok(parity.indexOf('- name: Install dependencies for manifest coverage') < parity.indexOf('- name: Check Bitget TypeScript equivalence'), 'Bitget oracle must follow root dependency installation');
  assert.doesNotMatch(step(parity, 'Check live registry TypeScript equivalence', /^        run: yarn registry:check$/m), /^        env:/m, 'registry oracle may not override environment');
  // `Isolate Rust toolchain` is deliberately absent: it existed because five
  // self-hosted jobs shared one $HOME and collided over ~/.cargo. Each hosted
  // runner is its own machine, so the isolation would only hide the cache.
  assert.doesNotMatch(parity, /CARGO_HOME=/, 'hosted runners need no CARGO_HOME override');
  exactRun(step(parity, "Install dependencies for manifest coverage", /^        run: \|$/m), ["test \"$(corepack yarn --version)\" = \"$(node -p \"require('./package.json').packageManager.split('@')[1]\")\"", "corepack yarn install --frozen-lockfile --ignore-scripts --non-interactive"], "Install dependencies for manifest coverage");
  const bybit = step(parity, 'Check live venue bybit conformance', /^        run: yarn venue-oracle:bybit$/m);
  assert.doesNotMatch(bybit, /^        env:/m, 'Bybit oracle may not override environment');
  assert.match(bybit, /^        working-directory: \.$/m, 'Bybit oracle runs at repository root');
  assert.ok(parity.indexOf('Check live venue bybit conformance') > parity.indexOf('Install dependencies for manifest coverage'), 'Bybit oracle must follow root dependency installation');
  const routing = step(native, 'Run routing TypeScript equivalence gate',
    /^          cargo test --locked -p safe-cex-transport --all-targets -- --include-ignored$/m,
    "matrix.check == 'test'");
  assert.match(routing, /^          node safe-cex-transport\/scripts\/routing.mjs --check --equivalence$/m, 'live routing command');
  assert.match(routing, /^          cargo test --locked -p safe-cex-transport --doc$/m, 'typed site ID compile-fail gate');
  assert.match(routing, /^          node --test scripts\/rust-ci-wiring.test.mjs$/m, 'audit must invoke itself');
  exactRun(routing, [
    'node --test scripts/rust-ci-wiring.test.mjs',
    'node safe-cex-transport/scripts/routing.mjs --check --equivalence',
    'cargo test --locked -p safe-cex-transport --all-targets -- --include-ignored',
    'cargo test --locked -p safe-cex-transport --doc',
  ], 'routing gate');
  const workerDeps = step(native, 'Install isolated worker oracle dependencies', /^        run: \|$/m, "matrix.check == 'test'");
  exactRun(workerDeps, [
    'node sdk-rs/safe-cex-webworker/ci-deps.cjs "$RUNNER_TEMP/worker-oracle-deps"',
    'corepack yarn --cwd "$RUNNER_TEMP/worker-oracle-deps" install --frozen-lockfile --ignore-scripts --non-interactive',
    'echo "SAFE_CEX_WORKER_LODASH_PATH=$RUNNER_TEMP/worker-oracle-deps/node_modules/lodash" >> "$GITHUB_ENV"',
  ], 'worker oracle dependencies');
  assert.ok(native.indexOf('- name: Install isolated worker oracle dependencies') < native.indexOf('- name: Run ${{ matrix.check }}'), 'worker dependencies must precede native tests');
  const numeric = step(native, 'Run numeric TypeScript differential gate', /^          cargo test --locked -p safe-cex-num --all-targets -- --include-ignored --nocapture$/m, "matrix.check == 'test'");
  const okx = step(native, 'Run OKX venue TypeScript equivalence gate', /^          corepack yarn venue-oracle:okx$/m, "matrix.check == 'test'");
  assert.match(okx, /^        working-directory: \.$/m, 'OKX root script working directory');
  exactRun(okx, ['corepack yarn venue-oracle:okx'], 'OKX venue gate');
  exactRun(numeric, ['node --test safe-cex-num/tests/gate.test.cjs', 'cargo test --locked -p safe-cex-num --all-targets -- --include-ignored --nocapture'], 'numeric gate');
  const warning = step(job('wasm'), 'Verify transport warnings in actual wasm', /^          node safe-cex-transport\/scripts\/warning-wasm.mjs$/m);
  assert.match(warning, /^          node safe-cex-ffi\/scripts\/setup.mjs$/m, 'locked wasm tooling setup');
  exactRun(warning, ['node safe-cex-ffi/scripts/setup.mjs', 'node safe-cex-transport/scripts/warning-wasm.mjs'], 'wasm warning gate');
  for (const event of ['push', 'pull_request']) {
    const block = source.match(new RegExp(`^  ${event}:\\n([\\s\\S]*?)(?=^  [\\w-]+:|^permissions:)`, 'm'))?.[1];
    for (const path of ['sdk-rs/**', 'packages/safe-cex/**', 'packages/safe-cex/src/exchanges/binance/**', 'packages/lighter-sdk/**', 'packages/poseidon-crypto/**', 'packages/script-api-core/src/order/**', 'packages/http-over-websocket/src/egressRegion.ts', 'packages/http-over-websocket/src/tunnelEndpoints.ts', 'sdk-rs/scripts/rust-ci-wiring.test.mjs']) {
      assert.ok(block?.split('\n').includes(`      - '${path}'`), `${event} trigger missing ${path}`);
    }
  }
  auditExtractorInstallWiring(source);
}

function auditGrvt(source) {
  const native = source.match(/^  native:\n([\s\S]*?)(?=^  [\w-]+:|$(?![\s\S]))/m)?.[1];
  const block = native?.match(/^      - name: Run GRVT venue TypeScript equivalence gate\n([\s\S]*?)(?=^      - |$(?![\s\S]))/m)?.[1];
  assert.equal(block, "        if: matrix.check == 'test'\n        working-directory: .\n        run: |\n          corepack yarn venue-oracle:grvt\n", 'GRVT mandatory oracle step');
}
test('GRVT mandatory live oracle wiring', () => auditGrvt(workflow));
test('GRVT oracle deletion is rejected', () => assert.throws(() => auditGrvt(workflow.replace('Run GRVT venue TypeScript equivalence gate', 'removed')), /GRVT mandatory oracle step/));
test('GRVT oracle failure masking is rejected', () => assert.throws(() => auditGrvt(workflow.replace('corepack yarn venue-oracle:grvt', 'corepack yarn venue-oracle:grvt || true')), /GRVT mandatory oracle step/));
test('GRVT oracle outside native job is rejected', () => assert.throws(() => auditGrvt(workflow.replace(/^  native:/m, '  misplaced:')), /GRVT mandatory oracle step/));
test('GRVT script retains all venue test targets', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:grvt'], 'cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-grvt --all-targets --jobs 2 -- --include-ignored && cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-grvt --doc --jobs 2');
});
test('Rust workflow enforces parity and live routing gates', () => audit(workflow));
test('omitting the OKX live oracle is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Run OKX venue TypeScript equivalence gate\n[\s\S]*?(?=      # Local)/, '')), /missing Run OKX venue TypeScript equivalence gate/));
test('masking the OKX oracle is rejected', () => assert.throws(() => audit(workflow.replace('corepack yarn venue-oracle:okx', 'corepack yarn venue-oracle:okx || true')), /Run OKX venue TypeScript equivalence gate command/));
test('OKX root script retains the live Cargo oracle', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:okx'], 'cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-okx --all-targets -- --include-ignored');
});
test('removing coverage is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check parity manifest coverage\n[\s\S]*?(?=      - name:)/, '')), /missing Check parity manifest coverage/));
test('continue-on-error is rejected', () => assert.throws(() => audit(workflow.replace('      - name: Check parity manifest coverage\n', '      - name: Check parity manifest coverage\n        continue-on-error: true\n')), /may not continue on error/));
test('narrowing source coverage is rejected', () => assert.throws(() => audit(workflow.replace("'packages/safe-cex/**'", "'packages/safe-cex/src/types/**'")), /push trigger missing packages\/safe-cex/));
test('omitting ignored routing oracle is rejected', () => assert.throws(() => audit(workflow.replace('--all-targets -- --include-ignored', '--all-targets')), /routing TypeScript equivalence gate command/));

test('omitting compile-fail gate is rejected', () => assert.throws(() => audit(workflow.replace('          cargo test --locked -p safe-cex-transport --doc\n', '')), /typed site ID compile-fail gate/));
test('omitting wasm warning probe is rejected', () => assert.throws(() => audit(workflow.replace('          node safe-cex-transport/scripts/warning-wasm.mjs', '          echo skipped')), /Verify transport warnings in actual wasm command/));

test('removing test matrix arm is rejected', () => assert.throws(() => audit(workflow.replace('          - check: test\n            command: cargo test --locked --no-fail-fast\n', '')), /native matrix must satisfy/));
test('removing audit self invocation is rejected', () => assert.throws(() => audit(workflow.replace('          node --test scripts/rust-ci-wiring.test.mjs\n', '')), /audit must invoke itself/));
test('masking a failed shell command is rejected', () => assert.throws(() => audit(workflow.replace('          node --test scripts/rust-ci-wiring.test.mjs', '          set +e\n          node --test scripts/rust-ci-wiring.test.mjs\n          exit 0')), /routing gate must retain fail-fast commands/));
test('overriding the step shell is rejected', () => assert.throws(() => audit(workflow.replace('      - name: Run routing TypeScript equivalence gate\n', '      - name: Run routing TypeScript equivalence gate\n        shell: bash --noprofile --norc {0}\n')), /shell override/));
test('overriding the default shell is rejected', () => assert.throws(() => audit(workflow.replace('defaults:\n  run:\n', 'defaults:\n  run:\n    shell: bash {0}\n')), /shell override/));

// The live oracle is the only behavioral check for shared web/native registry mappings.
test('removing live registry equivalence is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check live registry TypeScript equivalence\n[\s\S]*?(?=      - name:)/, '')), /missing Check live registry TypeScript equivalence/));
test('masking registry equivalence failure is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn registry:check', 'run: yarn registry:check || true')), /Check live registry TypeScript equivalence command/));

// CI delegates self-invocation to this exact root script; keep one executable command.
test('CI wiring root script retains the complete audit without a failure mask', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['rust-ci-wiring:check'], 'node --test sdk-rs/scripts/rust-ci-wiring.test.mjs');
});

test('registry script runs the live oracle without a failure mask', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['registry:check'], 'node sdk-rs/safe-cex-registry/scripts/check.mjs && node --test sdk-rs/safe-cex-registry/scripts/check.test.mjs');
});

test('registry oracle before root install is rejected', () => {
  const block = workflow.match(/      - name: Check live registry TypeScript equivalence\n[\s\S]*?(?=      - name:)/)[0];
  const reordered = workflow.replace(block, '').replace('      - name: Install dependencies for manifest coverage', block + '      - name: Install dependencies for manifest coverage');
  assert.throws(() => audit(reordered), /registry oracle must follow root dependency installation/);
});

test('registry step environment override is rejected', () => {
  assert.throws(() => audit(workflow.replace('      - name: Check live registry TypeScript equivalence\n', '      - name: Check live registry TypeScript equivalence\n        env:\n          CARGO: forged-cargo\n')), /registry oracle may not override environment/);
});

test('types toolchain setup cannot inject Cargo through GITHUB_ENV', () => {
  // The isolation step is gone with the shared box; what must stay impossible is
  // a job quietly repointing CARGO_HOME, which on a hosted runner can only mean
  // hiding the cache or reaching outside the workspace.
  const types = workflow.slice(workflow.indexOf('  types:'));
  const target = types.match(/      - name: Load workspace toolchain\n        run: rustup show\n/)[0];
  const injected =
    workflow.slice(0, workflow.indexOf('  types:')) +
    types.replace(
      target,
      '      - name: Load workspace toolchain\n        run: |\n          echo "CARGO_HOME=$RUNNER_TEMP/forged" >> "$GITHUB_ENV"\n          rustup show\n',
    );
  assert.notEqual(injected, workflow, 'control must change the actual toolchain setup');
  assert.throws(() => audit(injected), /hosted runners need no CARGO_HOME override/);
});
test('types root install commands are pinned', () => {
  const start = workflow.indexOf('  types:\n');
  const injected = workflow.slice(0, start) + workflow.slice(start).replace('          corepack yarn install --frozen-lockfile --ignore-scripts --non-interactive', '          echo skipped');
  assert.throws(() => audit(injected), /Install dependencies for manifest coverage must retain fail-fast commands/);
});

// Removing or masking this venue gate must itself break the CI audit.
test('removing Coinbase oracle is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check Coinbase live TypeScript oracle\n[\s\S]*?(?=      - name:)/, '')), /missing Check Coinbase/));
test('masking Coinbase oracle is rejected', () => assert.throws(() => audit(workflow.replace('run: corepack yarn venue-oracle:coinbase', 'run: corepack yarn venue-oracle:coinbase || true')), /Check Coinbase.*command/));
test('conditioning Coinbase oracle is rejected', () => assert.throws(() => audit(workflow.replace('      - name: Check Coinbase live TypeScript oracle\n', '      - name: Check Coinbase live TypeScript oracle\n        if: false\n')), /Check Coinbase.*condition/));
test('Coinbase root script retains Cargo gate', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:coinbase'], 'node sdk-rs/safe-cex-venue-coinbase/scripts/check.mjs');
});
// Venue oracles must remain mandatory after table/type migrations.
test('removing Bitget equivalence is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check Bitget TypeScript equivalence\n[\s\S]*?(?=      - name:)/, '')), /missing Check Bitget TypeScript equivalence/));
test('masking Bitget equivalence is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:bitget', 'run: yarn venue-oracle:bitget || true')), /Check Bitget TypeScript equivalence command/));
test('Bitget oracle command pins generation and ignored differential tests', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:bitget'], 'node sdk-rs/safe-cex-venue-bitget/scripts/types-oracle.cjs --check && node sdk-rs/safe-cex-venue-bitget/scripts/pure-oracle.cjs --check && node sdk-rs/safe-cex-venue-bitget/scripts/order-oracle.cjs --check && node sdk-rs/safe-cex-venue-bitget/scripts/signing-capture.cjs --check && node sdk-rs/safe-cex-venue-bitget/scripts/history-capture.cjs --check && cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets -- --include-ignored');
});
test('removing lighter oracle is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check live lighter TypeScript oracle\n[\s\S]*?(?=      - name:|$(?![\s\S]))/, '')), /missing Check live lighter TypeScript oracle/));
test('masking lighter oracle is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:lighter', 'run: yarn venue-oracle:lighter || true')), /Check live lighter TypeScript oracle command/));
test('skipping lighter oracle is rejected', () => assert.throws(() => audit(workflow.replace('      - name: Check live lighter TypeScript oracle\n', "      - name: Check live lighter TypeScript oracle\n        if: false\n")), /Check live lighter TypeScript oracle condition/));
test('lighter oracle script includes maintained TS tests', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:lighter'], 'cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets -- --include-ignored');
});

// Each venue oracle is mandatory and addressable independently of its future matrix.
test('removing Binance oracle is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check venue oracle binance\n[\s\S]*?(?=      - name:)/, '')), /missing Check venue oracle binance/));
test('masking Binance oracle failure is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:binance', 'run: yarn venue-oracle:binance || true')), /Check venue oracle binance command/));
test('conditional Binance oracle is rejected', () => assert.throws(() => audit(workflow.replace('      - name: Check venue oracle binance\n', '      - name: Check venue oracle binance\n        if: false\n')), /Check venue oracle binance condition/));
test('Binance script checks live generation and runs ignored tests', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:binance'], 'node sdk-rs/safe-cex-venue-binance/scripts/types.mjs --check && cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-binance --all-targets -- --include-ignored');
});
const bybitStep = '      - name: Check live venue bybit conformance\n        working-directory: .\n        run: yarn venue-oracle:bybit\n';
test('Bybit oracle cannot be removed or masked', () => {
  assert.throws(() => audit(workflow.replace(bybitStep, '')), /missing Check live venue bybit conformance/);
  assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:bybit', 'run: yarn venue-oracle:bybit || true')), /Check live venue bybit conformance command/);
  assert.throws(() => audit(workflow.replace(bybitStep, bybitStep.replace('        run:', '        if: false\n        run:'))), /Check live venue bybit conformance condition/);
  assert.throws(() => audit(workflow.replace(bybitStep, bybitStep.replace('        run:', '        continue-on-error: true\n        run:'))), /may not continue on error/);
});
test('Bybit oracle requires the frozen root installation first', () => {
  const reordered = workflow.replace(bybitStep, '').replace('      - name: Install dependencies for manifest coverage', bybitStep + '      - name: Install dependencies for manifest coverage');
  assert.throws(() => audit(reordered), /Bybit oracle must follow root dependency installation/);
});
test('Bybit script includes all live Cargo oracles', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:bybit'], 'cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets -- --include-ignored');
});
test('removing the push branch trigger is rejected by name', () => {
  assert.throws(() => audit(workflow.replace(/^    branches: \[[^\n]+\]\n/m, '')),
    /push branches must retain only master and feat\/safe-cex-rust; PRs cover feature branches/);
});
test('adding a feature lane to push is rejected', () => {
  assert.throws(() => audit(workflow.replace('branches: [master, feat/safe-cex-rust]', 'branches: [master, feat/safe-cex-rust, feat/rust-venue-example]')),
    /push branches must retain only master and feat\/safe-cex-rust; PRs cover feature branches/);
});
test('removing master from push is rejected', () => {
  assert.throws(() => audit(workflow.replace('branches: [master, feat/safe-cex-rust]', 'branches: [feat/safe-cex-rust]')),
    /push branches must retain only master and feat\/safe-cex-rust; PRs cover feature branches/);
});
test('removing integration from push is rejected', () => {
  assert.throws(() => audit(workflow.replace('branches: [master, feat/safe-cex-rust]', 'branches: [master]')),
    /push branches must retain only master and feat\/safe-cex-rust; PRs cover feature branches/);
});
// The source-derived WOO corpus must not drift while Rust fixtures stay green.
test('WOO oracle removal and masking fail the CI audit', () => {
  const block = workflow.match(/      - name: Check WOO venue TypeScript oracle\n(?:        [^\n]*\n)+/)[0];
  assert.throws(() => audit(workflow.replace(block, '')), /missing Check WOO/);
  assert.throws(() => audit(workflow.replace(block, block.replace('        run:', '        continue-on-error: true\n        run:'))), /may not continue on error/);
  assert.throws(() => audit(workflow.replace(block, block.replace('        run:', '        if: false\n        run:'))), /condition/);
});

test('WOO root command includes every source oracle without masking', () => {
  // Algo response membership and ownership witnesses run through the existing native target.
  // Account alias/freeze and guard witnesses use the account targets; pricing checks error fixtures.
  const pkg = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8'));
  assert.equal(pkg.scripts['venue-oracle:woo'], 'node sdk-rs/safe-cex-venue-woo/scripts/types-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/constants-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/signing-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/contracts-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/journal-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/public-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/fills-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/history-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/utils-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/pricing-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/response-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/public-dispatch-oracle.cjs --check && node sdk-rs/safe-cex-venue-woo/scripts/isolation-control.cjs && node sdk-rs/safe-cex-venue-woo/scripts/order-output-oracle.cjs --check && cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_lifecycle --test public_streams --jobs 2 --test class_construction --test socket_classes --test exchange_class --test response --test utils --test regular_writes --test algo_writes --test account_leverage --test position_mode --test pricing');
});

test('removing Hyperliquid state comparisons fails wiring',()=>assert.throws(()=>auditHl(hlWorkflow,hlCommand.replace('--lib exchange::tests','')),/required/));
test('production store and producer/model seam gate cannot be dropped', () => {
  const missing = workflow.replace(/      - name: Check live model and production store seams\n        run: [^\n]+\n/, '');
  assert.throws(() => audit(missing), /missing Check live model and production store seams/);
});
test('seam gate must include the live producer fixtures', () => {
  const narrowed = workflow.replace('run: cargo test --locked -p safe-cex-types -p safe-cex --all-targets -- --include-ignored','run: cargo test --locked -p safe-cex-types -p safe-cex --all-targets');
  assert.throws(() => audit(narrowed), /Check live model and production store seams command/);
});

// Worker source changes must preserve the live whole-row and rejected-input corpus.
test('removing worker equivalence is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check live worker TypeScript equivalence\n[\s\S]*$/, '')), /missing Check live worker TypeScript equivalence/));
test('masking worker equivalence is rejected', () => assert.throws(() => audit(workflow.replace('          yarn worker-oracle', '          yarn worker-oracle || true')), /Check live worker TypeScript equivalence command/));
test('worker equivalence is mandatory', () => assert.throws(() => audit(workflow.replace('      - name: Check live worker TypeScript equivalence\n', '      - name: Check live worker TypeScript equivalence\n        if: false\n')), /Check live worker TypeScript equivalence condition/));
test('worker oracle command and queue source triggers remain pinned', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['worker-oracle'], 'cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets');
  for (const file of ['Queue', 'QueueWithMaxSize', 'TimeLimitedQueue']) {
    assert.equal(workflow.split(`      - 'packages/data-structures/src/${file}.ts'`).length - 1, 2);
  }
});

test('worker equivalence cannot lose its isolated lodash dependency', () => assert.throws(() => audit(workflow.replaceAll('          node sdk-rs/safe-cex-webworker/ci-deps.cjs "$RUNNER_TEMP/worker-oracle-deps"\n', '')), /Check live worker TypeScript equivalence command/));
test('native worker oracle dependency omission is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Install isolated worker oracle dependencies\n[\s\S]*?(?=      - name: Run \$)/, '')), /missing Install isolated worker oracle dependencies/));
test('narrowing Lighter signing oracle inputs is rejected', () => {
  for (const path of ['packages/lighter-sdk/**', 'packages/poseidon-crypto/**']) {
    assert.throws(() => audit(workflow.replaceAll(`      - '${path}'\n`, '')), /push trigger missing/);
  }
});

for (const target of ['transaction_records', 'pure', 'signing', 'exchange']) {
  test(`Apexomni oracle cannot drop ${target}`, () => {
    const scripts = { ...packageScripts, 'venue-oracle:apexomni': packageScripts['venue-oracle:apexomni'].replace(` --test ${target}`, '') };
    assert.throws(() => audit(workflow, scripts), /Apexomni oracle must retain all eight test targets/);
  });
}

test('apexomni oracle cannot disappear or mask failures', () => {
  assert.throws(() => audit(workflow.replace(/      - name: Check apexomni live TypeScript conformance\n[\s\S]*?(?=      - name:)/, '')), /missing Check apexomni/);
  assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:apexomni', 'run: yarn venue-oracle:apexomni || true')), /apexomni.*command/);
});
test('moving Bitget oracle before root install is rejected', () => {
  const block = workflow.match(/      - name: Check Bitget TypeScript equivalence\n[\s\S]*?(?=      - name:)/)[0];
  const reordered = workflow.replace(block, '').replace('      - name: Install dependencies for manifest coverage\n', block + '      - name: Install dependencies for manifest coverage\n');
  assert.throws(() => audit(reordered), /Bitget oracle must follow root dependency installation/);
});
// Asterdex's independent live oracle must execute, including its isolated compiler.
test('removing Asterdex oracle is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check live Asterdex TypeScript equivalence\n[\s\S]*?(?=      - name:)/, '')), /missing Check live Asterdex/));
test('masking Asterdex oracle is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:asterdex', 'run: yarn venue-oracle:asterdex || true')), /Check live Asterdex.*command/));
test('Asterdex oracle script remains scoped and includes ignored tests', () => {
 const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
 assert.equal(scripts['venue-oracle:asterdex'], 'cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-asterdex --jobs 2 --all-targets -- --include-ignored');
});
// KuCoin's independent source oracle must remain mandatory, not a soft success.
test('removing KuCoin oracle fails', () => assert.throws(() => audit(workflow.replace(/      - name: Check live KuCoin TypeScript equivalence\n[\s\S]*?(?=      - name:)/, '')), /missing Check live KuCoin/));
test('masking KuCoin oracle fails', () => assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:kucoin', 'run: yarn venue-oracle:kucoin || true')), /Check live KuCoin.*command/));
function auditKucoinScript(command) {
  assert.equal(command, 'cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --lib --jobs 2 --test tables --test records --test account_mode --test broker --test api_capture --test query --test ws_helpers --test spot_helpers --test exchange_trait --test balance_reference --test order_reads_binding --test spot_io_binding --test numeric_ingress --test spot_fills_reference --test spot_orders_reference --test position_metadata --test private_dispose --test public_subscriptions --test public_lifecycle --test private_refresh --test message_routing --test socket_setup --test socket_boundaries --test rest_reads --test public_streams --test order_control --test bootstrap_reads --test spot_bodies --test order_lifecycle --test class_construction --test spot_private_class --test futures_private_class --test spot_public_class --test futures_public_class --test spot_native_accounts --test spot_native_policy --test spot_native_order_reads --test spot_exchange_class --test futures_native_helpers --test futures_native_accounts --test futures_native_order_reads --test futures_native_mapping_formatters', 'KuCoin oracle must retain lib and all reviewed test targets');
}
test('KuCoin script retains scoped oracle command', () => auditKucoinScript(packageScripts['venue-oracle:kucoin']));
for (const target of ['order_reads_binding', 'spot_io_binding', 'exchange_trait', 'position_metadata', 'private_dispose', 'public_subscriptions', 'public_lifecycle', 'private_refresh', 'message_routing', 'socket_setup', 'socket_boundaries', 'rest_reads', 'public_streams', 'order_control', 'bootstrap_reads', 'spot_bodies', 'order_lifecycle', 'class_construction', 'spot_private_class', 'futures_private_class', 'spot_public_class', 'futures_public_class', 'spot_native_accounts', 'spot_native_policy', 'spot_native_order_reads', 'spot_exchange_class', 'futures_native_helpers', 'futures_native_accounts', 'futures_native_order_reads', 'futures_native_mapping_formatters']) {
  test(`KuCoin oracle cannot drop ${target}`, () => {
    assert.throws(() => auditKucoinScript(packageScripts['venue-oracle:kucoin'].replace(` --test ${target}`, '')), /KuCoin oracle must retain lib and all reviewed test targets/);
  });
}
test('KuCoin oracle cannot drop lib', () => assert.throws(() => auditKucoinScript(packageScripts['venue-oracle:kucoin'].replace(' --lib', '')), /KuCoin oracle must retain lib and all reviewed test targets/));


// MEXC owns its live oracle; removal or shell masking must fail this gate.
test('mexc oracle removal is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check mexc live TypeScript oracle\n[\s\S]*?(?=      - name:)/, '')), /missing Check mexc live TypeScript oracle/));
test('mexc oracle failure masking is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn venue-oracle:mexc', 'run: yarn venue-oracle:mexc || true')), /Check mexc live TypeScript oracle command/));
test('mexc oracle script preserves scoped live execution', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['venue-oracle:mexc'], 'cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-mexc --all-targets --jobs 2 -- --include-ignored && cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --test mexc_reporting_worker --jobs 1');
});
function auditJsPrimitive(source, name, target) {
  const job = source.match(/^  js-primitives:\n([\s\S]*?)(?=^  [\w-]+:|$(?![\s\S]))/m)?.[1];
  assert.ok(job, 'shared primitive job missing');
  assert.doesNotMatch(job, /^    (?:if|continue-on-error):/m, 'shared primitive job bypass');
  assert.match(job, /uses: actions\/setup-node@v7\n        with:\n          node-version: 24\.20\.0/, 'primitive Node profile');
  const body = job.match(new RegExp(`^      - name: ${name}\\n([\\s\\S]*?)(?=^      - |$(?![\\s\\S]))`, 'm'))?.[1];
  assert.equal(body?.trimEnd(), `        run: cargo test --locked -p safe-cex-num --test ${target} --jobs 2`, `${name} mandatory exact command`);
}
test('shared collation gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared root collation', 'js_collation'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`collation gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared root collation\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared root collation\n', '      - name: Check shared root collation\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared root collation', 'js_collation'), /mandatory exact command/);
  });
}

test('shared Math extrema gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared Math extrema', 'js_math_extrema'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`Math extrema gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared Math extrema\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared Math extrema\n', '      - name: Check shared Math extrema\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared Math extrema', 'js_math_extrema'), /mandatory exact command/);
  });
}

test('shared Date gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared Date parsing', 'js_date'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`Date gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared Date parsing\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared Date parsing\n', '      - name: Check shared Date parsing\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared Date parsing', 'js_date'), /mandatory exact command/);
  });
}

test('shared lodash gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared lodash casing', 'lodash_case'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`lodash gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared lodash casing\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared lodash casing\n', '      - name: Check shared lodash casing\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared lodash casing', 'lodash_case'), /mandatory exact command/);
  });
}

function auditLodashInstall(source) {
  const job = source.split('  js-primitives:\n')[1];
  const install = job?.match(/      - name: Install shared primitive oracle dependencies\n([\s\S]*?)(?=      - name:)/)?.[1];
  const expected = `        working-directory: .
        run: |
          node sdk-rs/safe-cex-types/ci-deps.cjs "$RUNNER_TEMP/routing-typescript-deps"
          test "$(corepack yarn --version)" = "$(node -p "require('./package.json').packageManager.split('@')[1]")"
          corepack yarn --cwd "$RUNNER_TEMP/routing-typescript-deps" install --frozen-lockfile --ignore-scripts --non-interactive
          echo "SAFE_CEX_TYPESCRIPT_PATH=$RUNNER_TEMP/routing-typescript-deps/node_modules/typescript" >> "$GITHUB_ENV"
`;
  assert.equal(install, expected, 'lodash must retain locked isolated install');
  assert.ok(job.indexOf('Install shared primitive oracle dependencies') < job.indexOf('Check shared lodash casing'), 'lodash install precedes oracle');
}
test('shared lodash uses the existing isolated dependency selector', () => auditLodashInstall(workflow));
for (const from of ['--frozen-lockfile', 'SAFE_CEX_TYPESCRIPT_PATH=', 'node sdk-rs/safe-cex-types/ci-deps.cjs']) {
  test(`lodash install rejects mutation ${from}`, () => {
    const split = workflow.indexOf('  js-primitives:');
    const changed = workflow.slice(0, split) + workflow.slice(split).replace(from, 'missing');
    assert.throws(() => auditLodashInstall(changed), /locked isolated install/);
  });
}

test('shared inherited table gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared inherited table members', 'js_table_lookup'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`inherited table gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared inherited table members\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared inherited table members\n', '      - name: Check shared inherited table members\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared inherited table members', 'js_table_lookup'), /mandatory exact command/);
  });
}

// Retain P5 oracle controls while the integration workflow catches up.
const cryptoPreservationTest = test;
{
const test = (name, ...args) => cryptoPreservationTest(`Crypto preservation: ${name}`, ...args);
// The existing CI audit also executes the live Cargo layering suite.


const rootScripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
const lighterCommand = 'node sdk-rs/safe-cex-crypto/tests/fixtures/lighter/go-wasm.cjs && node sdk-rs/safe-cex-crypto/tests/fixtures/lighter/go-wasm.cjs --edges && node sdk-rs/safe-cex-crypto/tests/fixtures/lighter/go-wasm.cjs --noncanonical && cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-crypto --lib lighter::tests && node sdk-rs/safe-cex-crypto/scripts/check-independent-lighter.cjs && cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-crypto --test independent_lighter --test independent_lighter_field_identity';
function audit(source, scripts = rootScripts) {
  assert.equal(scripts['crypto:oracles:check'], 'node sdk-rs/safe-cex-crypto/scripts/oracles.cjs', 'crypto root script must execute all live oracles');
  assert.equal(scripts['crypto:lighter:check'], lighterCommand, 'Lighter root script must retain every oracle and Rust comparison');
  // Rely on GitHub's fail-fast default shell; overrides require an audited change.
  assert.doesNotMatch(source, /^\s*shell:/m, 'shell override may disable fail-fast execution');
  const job = (name) => {
    const body = source.match(new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [\\w-]+:|$(?![\\s\\S]))`, 'm'))?.[1];
    assert.ok(body, `missing ${name} job`);
    assert.doesNotMatch(body, /^    (?:if|continue-on-error):/m, `${name} job may not bypass gates`);
    return body;
  };
  const step = (body, name, command, condition) => {
    const block = body.match(new RegExp(`^      - name: ${name}\\n([\\s\\S]*?)(?=^      - |$(?![\\s\\S]))`, 'm'))?.[1];
    assert.ok(block, `missing ${name}`);
    assert.equal((block.match(/^        run:/gm) ?? []).length, 1, `${name} must have one run declaration`);
    assert.doesNotMatch(block, /^        continue-on-error:/m, `${name} may not continue on error`);
    const conditions = block.match(/^        if: (.*)$/m);
    assert.equal(conditions?.[1], condition, `${name} condition`);
    assert.match(block, command, `${name} command`);
    return block;
  };
  const native = job('native');
  const matrix = native.match(/^      matrix:\n([\s\S]*?)(?=^    steps:)/m)?.[1];
  assert.ok(matrix, 'native test matrix missing');
  assert.match(matrix, /^        include:\n(?:          - check: [a-z]+\n            command: [^\n]+\n)+$/, 'native matrix must be a literal include without exclusions');
  assert.match(matrix, /^          - check: test\n            command: cargo test --locked --no-fail-fast$/m, 'native matrix must satisfy matrix.check == test');
  const exactRun = (block, commands, label) => {
    assert.equal((block.match(/^        run:/gm) ?? []).length, 1, `${label} must have one run declaration`);
    const actual = block.match(/^        run: \|\n((?:          [^\n]*\n?)+)/m)?.[1]?.trimEnd().split('\n').map(line => line.slice(10));
    assert.deepEqual(actual, commands, `${label} must retain fail-fast commands`);
  };
  const parity = job('types');
  for (const [name, command] of [
    ['Check parity manifest coverage', /^        run: yarn parity:extract --check$/m],
    ['Check parity invention and status', /^        run: yarn parity:check$/m],
  ]) assert.match(step(parity, name, command), /^        working-directory: \.$/m);
  const routing = step(native, 'Run routing TypeScript equivalence gate',
    /^          cargo test --locked -p safe-cex-transport --all-targets -- --include-ignored$/m,
    "matrix.check == 'test'");
  assert.match(routing, /^          node safe-cex-transport\/scripts\/routing.mjs --check --equivalence$/m, 'live routing command');
  assert.match(routing, /^          cargo test --locked -p safe-cex-transport --doc$/m, 'typed site ID compile-fail gate');
  assert.match(routing, /^          node --test scripts\/rust-ci-wiring.test.mjs$/m, 'audit must invoke itself');
  exactRun(routing, [
    'node --test scripts/rust-ci-wiring.test.mjs',
    'node safe-cex-transport/scripts/routing.mjs --check --equivalence',
    'cargo test --locked -p safe-cex-transport --all-targets -- --include-ignored',
    'cargo test --locked -p safe-cex-transport --doc',
  ], 'routing gate');
  const lighter = step(native, 'Run Lighter Go-WASM vector oracle', /^        run: yarn crypto:lighter:check$/m, "matrix.check == 'test'");
  assert.match(lighter, /^        working-directory: \.$/m, 'Lighter oracle must run from root');
  assert.equal(lighter.match(/^        run: (.*)$/m)?.[1], 'yarn crypto:lighter:check', 'Lighter step must not mask failures');
  const crypto = step(native, 'Run all crypto signing oracles', /^        run: yarn crypto:oracles:check$/m, "matrix.check == 'test'");
  assert.match(crypto, /^        working-directory: \.$/m, 'crypto oracles must run from root');
  const setup = step(native, 'Install isolated crypto oracle dependencies', /^          node sdk-rs\/safe-cex-crypto\/scripts\/ci-deps.cjs "\$RUNNER_TEMP\/crypto-oracle-deps"$/m, "matrix.check == 'test'");
  assert.match(setup, /^        working-directory: \.$/m);
  exactRun(setup, [
    'node sdk-rs/safe-cex-crypto/scripts/ci-deps.cjs "$RUNNER_TEMP/crypto-oracle-deps"',
    'test "$(corepack yarn --version)" = "$(node -p "require(\'./package.json\').packageManager.split(\'@\')[1]")"',
    'corepack yarn --cwd "$RUNNER_TEMP/crypto-oracle-deps" install --frozen-lockfile --ignore-scripts --non-interactive',
    'echo "SAFE_CEX_CRYPTO_ORACLE_DEPS=$RUNNER_TEMP/crypto-oracle-deps" >> "$GITHUB_ENV"',
  ], 'crypto dependency setup');
  const nativeSdk = step(native, 'Prepare pinned zkLink native oracle', /^          node sdk-rs\/safe-cex-crypto\/scripts\/zklink-native.cjs --prepare "\$RUNNER_TEMP\/zklink-native-oracle"$/m, "matrix.check == 'test'");
  assert.match(nativeSdk, /^        working-directory: \.$/m);
  exactRun(nativeSdk, [
    'node sdk-rs/safe-cex-crypto/scripts/zklink-native.cjs --prepare "$RUNNER_TEMP/zklink-native-oracle"',
    'echo "SAFE_CEX_ZKLINK_NATIVE_ORACLE=$RUNNER_TEMP/zklink-native-oracle" >> "$GITHUB_ENV"',
  ], 'native SDK setup');
  const numeric = step(native, 'Run numeric TypeScript differential gate', /^          cargo test --locked -p safe-cex-num --all-targets -- --include-ignored --nocapture$/m, "matrix.check == 'test'");
  exactRun(numeric, ['node --test safe-cex-num/tests/gate.test.cjs', 'cargo test --locked -p safe-cex-num --all-targets -- --include-ignored --nocapture'], 'numeric gate');
  const warning = step(job('wasm'), 'Verify transport warnings in actual wasm', /^          node safe-cex-transport\/scripts\/warning-wasm.mjs$/m);
  assert.match(warning, /^          node safe-cex-ffi\/scripts\/setup.mjs$/m, 'locked wasm tooling setup');
  exactRun(warning, ['node safe-cex-ffi/scripts/setup.mjs', 'node safe-cex-transport/scripts/warning-wasm.mjs'], 'wasm warning gate');
  for (const event of ['push', 'pull_request']) {
    const block = source.match(new RegExp(`^  ${event}:\\n([\\s\\S]*?)(?=^  [\\w-]+:|^permissions:)`, 'm'))?.[1];
    if (event === 'pull_request') assert.doesNotMatch(block, /^    (?:branches|branches-ignore):/m, 'pull_request must not restrict target branches');
    if (event === 'push') assert.match(block, /^    branches: \[master, feat\/safe-cex-rust\]$/m, 'push must retain only master and integration; feature lanes run on PRs');
    for (const path of ['sdk-rs/**', 'packages/safe-cex/**', 'packages/poseidon-crypto/**', 'packages/lighter-sdk/src/signer/**', 'packages/script-api-core/src/order/**', 'packages/http-over-websocket/src/egressRegion.ts', 'packages/http-over-websocket/src/tunnelEndpoints.ts', 'sdk-rs/scripts/rust-ci-wiring.test.mjs']) {
      assert.ok(block?.split('\n').includes(`      - '${path}'`), `${event} trigger missing ${path}`);
    }
  }
}

test('Rust workflow enforces parity and live routing gates', () => audit(workflow));
test('removing coverage is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Check parity manifest coverage\n[\s\S]*?(?=      - name:)/, '')), /missing Check parity manifest coverage/));
test('continue-on-error is rejected', () => assert.throws(() => audit(workflow.replace('      - name: Check parity manifest coverage\n', '      - name: Check parity manifest coverage\n        continue-on-error: true\n')), /may not continue on error/));
test('narrowing source coverage is rejected', () => assert.throws(() => audit(workflow.replace("'packages/safe-cex/**'", "'packages/safe-cex/src/types/**'")), /push trigger missing packages\/safe-cex/));
test('omitting ignored routing oracle is rejected', () => assert.throws(() => audit(workflow.replace('--all-targets -- --include-ignored', '--all-targets')), /routing TypeScript equivalence gate command/));

test('omitting compile-fail gate is rejected', () => assert.throws(() => audit(workflow.replace('          cargo test --locked -p safe-cex-transport --doc\n', '')), /typed site ID compile-fail gate/));
test('omitting wasm warning probe is rejected', () => assert.throws(() => audit(workflow.replace('          node safe-cex-transport/scripts/warning-wasm.mjs', '          echo skipped')), /Verify transport warnings in actual wasm command/));

test('removing test matrix arm is rejected', () => assert.throws(() => audit(workflow.replace('          - check: test\n            command: cargo test --locked --no-fail-fast\n', '')), /native matrix must satisfy/));
test('removing audit self invocation is rejected', () => assert.throws(() => audit(workflow.replace('          node --test scripts/rust-ci-wiring.test.mjs\n', '')), /audit must invoke itself/));
test('masking a failed shell command is rejected', () => assert.throws(() => audit(workflow.replace('          node --test scripts/rust-ci-wiring.test.mjs', '          set +e\n          node --test scripts/rust-ci-wiring.test.mjs\n          exit 0')), /routing gate must retain fail-fast commands/));
test('overriding the step shell is rejected', () => assert.throws(() => audit(workflow.replace('      - name: Run routing TypeScript equivalence gate\n', '      - name: Run routing TypeScript equivalence gate\n        shell: bash --noprofile --norc {0}\n')), /shell override/));
test('overriding the default shell is rejected', () => assert.throws(() => audit(workflow.replace('defaults:\n  run:\n', 'defaults:\n  run:\n    shell: bash {0}\n')), /shell override/));

test('removing Lighter oracle step is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Run Lighter Go-WASM vector oracle\n[\s\S]*?(?=      - name:)/, '')), /missing Run Lighter/));
test('masking Lighter oracle failure is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn crypto:lighter:check', 'run: yarn crypto:lighter:check || true')), /Lighter/));
test('bypassing Lighter oracle condition is rejected', () => assert.throws(() => audit(workflow.replace("- name: Run Lighter Go-WASM vector oracle\n        if: matrix.check == 'test'", "- name: Run Lighter Go-WASM vector oracle\n        if: false")), /condition/));
test('removing Lighter root script is rejected', () => { const scripts = { ...rootScripts }; delete scripts['crypto:lighter:check']; assert.throws(() => audit(workflow, scripts), /Lighter root script/); });
test('masking Lighter root script is rejected', () => assert.throws(() => audit(workflow, { ...rootScripts, 'crypto:lighter:check': `${lighterCommand} || true` }), /Lighter root script/));
for (const fragment of [' --edges', ' --noncanonical', ' && cargo test']) {
  test(`omitting Lighter oracle component ${fragment} is rejected`, () => assert.throws(() => audit(workflow, { ...rootScripts, 'crypto:lighter:check': lighterCommand.replace(fragment, '') }), /Lighter root script/));
}

test('restricting pull_request branches is rejected', () => assert.throws(() => audit(workflow.replace('  pull_request:\n', '  pull_request:\n    branches: [never-a-real-branch]\n')), /pull_request must not restrict/));
test('ignoring pull_request branches is rejected', () => assert.throws(() => audit(workflow.replace('  pull_request:\n', "  pull_request:\n    branches-ignore: ['**']\n")), /pull_request must not restrict/));
test('adding P5a branch to push is rejected', () => assert.throws(() => audit(workflow.replace('branches: [master, feat/safe-cex-rust]', 'branches: [master, feat/safe-cex-rust, feat/rust-p5a-crypto]')), /push must retain only master and integration/));

test('removing all crypto oracle step is rejected', () => assert.throws(() => audit(workflow.replace(/      - name: Run all crypto signing oracles\n[\s\S]*?(?=      - name:)/, '')), /missing Run all crypto/));
test('masking all crypto oracle command is rejected', () => assert.throws(() => audit(workflow.replace('run: yarn crypto:oracles:check','run: yarn crypto:oracles:check || true')), /Run all crypto/));
test('disabling crypto oracle step is rejected', () => assert.throws(() => audit(workflow.replace("- name: Run all crypto signing oracles\n        if: matrix.check == 'test'", "- name: Run all crypto signing oracles\n        if: false")), /condition/));
test('masking crypto root script is rejected', () => assert.throws(() => audit(workflow, { ...rootScripts, 'crypto:oracles:check': 'echo skipped' }), /crypto root script/));
test('removing frozen oracle install is rejected', () => assert.throws(() => audit(workflow.replace('crypto-oracle-deps" install --frozen-lockfile','crypto-oracle-deps" install')), /crypto dependency setup/));

const runnerDirectory = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../safe-cex-crypto/scripts');
const oracleRunner = readFileSync(path.join(runnerDirectory, 'oracles.cjs'), 'utf8');
const expectedOracles = [
 ['generate.cjs'], ['lighter/go-wasm.cjs'], ['lighter/go-wasm.cjs','--edges'],
 ['lighter/go-wasm.cjs','--noncanonical'], ['lighter/go-wasm.cjs','--security'], ['lighter/go-wasm.cjs','--lengths'], ['lighter/go-wasm.cjs','--two-noncanonical'],
 ['../../scripts/check-independent-lighter.cjs'],
 ['ethereum/generate.cjs'], ['ethereum/primitives.cjs'], ['ethereum/icap-maximum.cjs'], ['ethereum/typed-validation.cjs'], ['ethereum/aster-preimages.cjs'], ['../../scripts/check-ethereum-isolation.cjs'], ['msgpack/generate.cjs'], ['ed25519/node.cjs'],
 ['starknet/sdk.cjs'], ['starknet/typed-data.cjs'], ['starknet/domain-field-identities.cjs'], ['starknet/typed-boundaries.cjs'], ['starknet/typed-shapes.cjs'], ['starknet/typed-compositions.cjs'], ['starknet/typed-closed-kinds.cjs'], ['starknet/typed-exempt-controls.cjs'], ['starknet/typed-composite-shapes.cjs'], ['zklink/npm-wasm.cjs'],
];
function probeRunner(source, fail = -1) {
 const calls = [];
 vm.runInNewContext(source, { __dirname: runnerDirectory, process: { execPath: '/node-test', env: {} }, require(name) {
  if(name === 'node:assert/strict') return assert;
  if(name === 'node:path') return path;
  if(name === 'node:child_process') return {spawnSync(command,args,options) { assert.equal(options.shell, undefined); calls.push({command,args:[...args]}); return {status:calls.length-1===fail?1:0}; }};
  throw Error(`unexpected runner dependency ${name}`);
 } });
 return calls;
}
function auditRunner(source) {
 const calls = probeRunner(source);
 assert.equal(calls.length,expectedOracles.length+3,'all live oracle and drift calls required');
 for(const [i,expected] of expectedOracles.entries()) {
  assert.equal(calls[i].command,'/node-test');
  assert.deepEqual(calls[i].args, [path.join(runnerDirectory,'../tests/fixtures',expected[0]),...expected.slice(1)], 'live oracle command');
  assert.throws(()=>probeRunner(source,i), /live oracle failed/, `failure of ${expected[0]} must propagate`);
 }
 assert.equal(calls.at(-3).command,'/node-test');
 assert.deepEqual(calls.at(-3).args,[path.join(runnerDirectory,'zklink-native.cjs')]);
 assert.throws(()=>probeRunner(source,expectedOracles.length), /native SDK boundary oracle failed/);
 assert.deepEqual(calls.at(-2),{command:'git',args:['diff','--exit-code','--','sdk-rs/safe-cex-crypto/tests/fixtures/*.json']});
 assert.throws(()=>probeRunner(source,expectedOracles.length+1), /fixture regeneration drift/);
 assert.deepEqual(calls.at(-1),{command:'cargo',args:['test','--manifest-path','sdk-rs/Cargo.toml','--locked','-p','safe-cex-crypto','--test','stark_enum_cardinality','--test','field_identities','--test','independent_lighter_field_identity','--test','live_poseidon_types','--jobs','1','--','--include-ignored']}, 'native identity and ignored Poseidon tests must execute');
 assert.throws(()=>probeRunner(source,expectedOracles.length+2), /native cryptographic identity supplements failed/);
}
test('all seven oracle families execute and propagate every failure',()=>auditRunner(oracleRunner));
test('removing independent Lighter venue oracle is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  ['../../scripts/check-independent-lighter.cjs'],\n",'')),/required/));
test('removing Ethereum live oracle is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  ['ethereum/generate.cjs'],\n",'')),/required/));
test('removing Ethereum primitive oracle is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  ['ethereum/primitives.cjs'],\n",'')),/required/));
test('removing Ethereum validation oracle is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  ['ethereum/typed-validation.cjs'],\n",'')),/required/));

test('removing Ethereum isolated runtime check is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  ['../../scripts/check-ethereum-isolation.cjs'],\n",'')),/required/));
test('removing actual Aster TS preimage oracle is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  ['ethereum/aster-preimages.cjs'],\n",'')),/required/));
test('masking child oracle failures is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  assert.equal(result.status,0,`live oracle failed: ${script} ${args.join(' ')}`);",'')),/must propagate/));

test('removing native SDK oracle setup is rejected',()=>assert.throws(()=>audit(workflow.replace(/      - name: Prepare pinned zkLink native oracle\n[\s\S]*?(?=      - name:)/,'')),/missing Prepare pinned/));
test('masking native SDK oracle failure is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("assert.equal(native.status,0,'native SDK boundary oracle failed');",'')),/Missing expected exception/));
test('removing Stark identity SDK oracle is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("  ['starknet/domain-field-identities.cjs'],\n",'')),/required/));
test('masking native identity failure is rejected',()=>assert.throws(()=>auditRunner(oracleRunner.replace("assert.equal(identity.status,0,'native cryptographic identity supplements failed');",'')),/Missing expected exception/));
test('removing ignored Poseidon record target is rejected',()=>{
 const changed=oracleRunner.replace(",'--test','live_poseidon_types'",'');
 assert.notEqual(changed,oracleRunner,'Poseidon target mutation must apply');
 assert.throws(()=>auditRunner(changed),/native identity and ignored Poseidon tests must execute/);
});
test('omitting include-ignored for Poseidon records is rejected',()=>{
 const changed=oracleRunner.replace(",'--','--include-ignored'",'');
 assert.notEqual(changed,oracleRunner,'include-ignored mutation must apply');
 assert.throws(()=>auditRunner(changed),/native identity and ignored Poseidon tests must execute/);
});

}

function auditPrimitivePush(source) {
  const push = source.match(/^  push:\n([\s\S]*?)(?=^  pull_request:)/m)?.[1];
  const pullRequest = source.match(/^  pull_request:\n([\s\S]*?)(?=^permissions:)/m)?.[1];
  assert.match(push ?? '', /^    branches: \[master, feat\/safe-cex-rust\]$/m, 'push must retain only master and integration');
  assert.ok(pullRequest?.split('\n').includes("      - 'sdk-rs/**'"), 'PR must cover Rust primitive changes');
}
test('isolated P5 primitives branch is covered by pull requests, not duplicate pushes', () => auditPrimitivePush(workflow));
test('removing PR Rust coverage for P5 primitives fails', () => {
  assert.throws(() => auditPrimitivePush(workflow.replace(/(  pull_request:\n[\s\S]*?)      - 'sdk-rs\/\*\*'\n/, '$1')), /PR must cover Rust primitive changes/);
});


function auditEthereumPush(source) {
  const push = source.match(/^  push:\n([\s\S]*?)(?=^  pull_request:)/m)?.[1];
  const pullRequest = source.match(/^  pull_request:\n([\s\S]*?)(?=^permissions:)/m)?.[1];
  assert.match(push ?? '', /^    branches: \[master, feat\/safe-cex-rust\]$/m, 'push must retain only master and integration');
  assert.ok(pullRequest?.split('\n').includes("      - 'sdk-rs/**'"), 'PR must cover Ethereum primitive changes');
}
test('Ethereum primitives branch is covered by pull requests, not duplicate pushes', () => auditEthereumPush(workflow));
test('removing PR Rust coverage for Ethereum primitives fails', () => {
  assert.throws(() => auditEthereumPush(workflow.replace(/(  pull_request:\n[\s\S]*?)      - 'sdk-rs\/\*\*'\n/, '$1')), /PR must cover Ethereum primitive changes/);
});
const hlWorkflow = workflowSource(2);
const hlScripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
const hlCommand = 'node sdk-rs/safe-cex-venue-hyperliquid/scripts/types.cjs --check && node sdk-rs/safe-cex-venue-hyperliquid/scripts/oracle.cjs --check && node sdk-rs/safe-cex-venue-hyperliquid/scripts/literal-unions.cjs && node sdk-rs/safe-cex-venue-hyperliquid/scripts/record-shapes.cjs && node sdk-rs/safe-cex-venue-hyperliquid/scripts/numeric-helpers.cjs && node sdk-rs/safe-cex-venue-hyperliquid/scripts/numeric-helpers.cjs --refusals && node sdk-rs/safe-cex-venue-hyperliquid/scripts/order-history.cjs && node sdk-rs/safe-cex-venue-hyperliquid/scripts/order-history.cjs --boundaries && node sdk-rs/safe-cex-venue-hyperliquid/scripts/order-history.cjs --helpers && cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-hyperliquid --test literal_unions --test record_shapes --test numeric_helpers --test order_history --test typed_numbers --test typed_ingress_audit --test typed_null_boundaries --test field_identities --test nan_capture_policy --jobs 2 && cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-hyperliquid --lib exchange::tests --jobs 2';
function auditHl(source,command=hlScripts['venue-oracle:hyperliquid']) {
 assert.equal(command,hlCommand,'Hyperliquid live children and Rust accepted-set comparisons required');
 const block=source.match(/      - name: Run Hyperliquid TypeScript accepted-set oracle\n[\s\S]*?(?=      - name:|$)/)?.[0];
 assert.ok(block,'mandatory Hyperliquid oracle step missing');
 assert.match(block,/^        if: matrix.check == 'test'$/m);
 assert.match(block,/^        working-directory: \.$/m);
 assert.match(block,/^        run: yarn venue-oracle:hyperliquid$/m,'Hyperliquid oracle failure must propagate');
 assert.doesNotMatch(block,/continue-on-error|always\(\)/);
}
test('Hyperliquid accepted-set oracle is mandatory in native test CI',()=>auditHl(hlWorkflow));
test('removing Hyperliquid oracle fails wiring',()=>assert.throws(()=>auditHl(hlWorkflow.replace(/      - name: Run Hyperliquid TypeScript accepted-set oracle\n[\s\S]*?(?=      - name:)/,'')),/missing/));
test('masking Hyperliquid oracle fails wiring',()=>assert.throws(()=>auditHl(hlWorkflow.replace('run: yarn venue-oracle:hyperliquid','run: yarn venue-oracle:hyperliquid || true')),/propagate/));
test('removing a Hyperliquid child or comparison fails wiring',()=>{for(const child of ['types.cjs --check','oracle.cjs --check','literal-unions.cjs','record-shapes.cjs','--test literal_unions','--test record_shapes','numeric-helpers.cjs','--test numeric_helpers','order-history.cjs','order-history.cjs --boundaries','order-history.cjs --helpers','--test order_history','--test typed_numbers','--test typed_ingress_audit','--test typed_null_boundaries','--jobs 2']) assert.throws(()=>auditHl(hlWorkflow,hlCommand.replace(child,'')),/required/);});
test('production store and producer/model seam gate cannot be dropped', () => {
  const missing = workflow.replace(/      - name: Check live model and production store seams\n        run: [^\n]+\n/, '');
  assert.throws(() => audit(missing), /missing Check live model and production store seams/);
});
test('seam gate must include the live producer fixtures', () => {
  const narrowed = workflow.replace('run: cargo test --locked -p safe-cex-types -p safe-cex --all-targets -- --include-ignored','run: cargo test --locked -p safe-cex-types -p safe-cex --all-targets');
  assert.throws(() => audit(narrowed), /Check live model and production store seams command/);
});
test('shared string replacement gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared string replacement', 'js_replace'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`string replacement gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared string replacement\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared string replacement\n', '      - name: Check shared string replacement\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared string replacement', 'js_replace'), /mandatory exact command/);
  });
}

test('shared JSON property entries gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared JSON property entries', 'js_property'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`JSON property gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared JSON property entries\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared JSON property entries\n', '      - name: Check shared JSON property entries\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared JSON property entries', 'js_property'), /mandatory exact command/);
  });
}

test('shared lodash random gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared lodash random', 'lodash_random'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`lodash random gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared lodash random\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared lodash random\n', '      - name: Check shared lodash random\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared lodash random', 'lodash_random'), /mandatory exact command/);
  });
}

for (const mutation of ['delete', 'skip', 'continue', 'export-only']) {
  test(`cancellation classification CI mutation fails: ${mutation}`, () => {
    const block = '      - name: Audit cancellable future classifications\n';
    let changed;
    if (mutation === 'delete') changed = workflow.replace(/      - name: Audit cancellable future classifications\n[\s\S]*?(?=      - name:)/, '');
    if (mutation === 'skip') changed = workflow.replace(block + "        if: matrix.check == 'test'", block + '        if: false');
    if (mutation === 'continue') changed = workflow.replace(block, block + '        continue-on-error: true\n');
    if (mutation === 'export-only') changed = workflow.replace('cargo test --locked -p safe-cex-exchange --all-features --test cancellable_future_inventory --jobs 2', 'cargo test --locked -p safe-cex-exchange --test cancellable_future_inventory export_cancellable_future_discovery --jobs 2 -- --include-ignored');
    assert.throws(() => audit(changed), /cancellable|cancellation/i);
  });
}

function auditNumericIngress(source) {
  const header = '      - name: Check shared numeric ingress\n';
  const env = '        env:\n          CARGO_BUILD_JOBS: 2\n';
  assert.ok(source.includes(header + env), 'numeric ingress mandatory exact command and nested build job bound');
  auditJsPrimitive(source.replace(header + env, header), 'Check shared numeric ingress', 'numeric_ingress --lib');
}
test('shared numeric ingress gate is mandatory', () => auditNumericIngress(workflow));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`numeric ingress gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared numeric ingress\n        env:\n          CARGO_BUILD_JOBS: 2\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared numeric ingress\n', '      - name: Check shared numeric ingress\n' + replacement);
    assert.throws(() => auditNumericIngress(changed), /mandatory exact command/);
  });
}

function auditNumericIngressControls(source) {
  assert.deepEqual([...source.matchAll(/cases\.compile_fail\("([^"]+)"\)/g)].map(match => match[1]), [
    'tests/numeric_ingress_ui/generic_value.rs',
    'tests/numeric_ingress_ui/skipped_value.rs',
    'tests/numeric_ingress_ui/renamed_value.rs',
    'tests/numeric_ingress_ui/omitted_field.rs',
  ], 'numeric ingress exact four eligibility controls');
  assert.match(source, /cases\.pass\("tests\/numeric_ingress_ui\/shared_buffer\.rs"\)/, 'numeric ingress positive compiling control');
  assert.doesNotMatch(source, /#\[ignore/, 'numeric ingress controls cannot be ignored');
}
const numericIngressTests = readFileSync(new URL('../safe-cex-num/tests/numeric_ingress.rs', import.meta.url), 'utf8');
test('numeric ingress compiler control registry is pinned', () => auditNumericIngressControls(numericIngressTests));
test('numeric ingress deleted compiler control fails by name', () => {
  assert.throws(() => auditNumericIngressControls(numericIngressTests.replace(/\s*cases\.compile_fail\("tests\/numeric_ingress_ui\/skipped_value\.rs"\);/, '')), /exact four eligibility controls/);
});

test('Hyperliquid native field identity and NaN policy tests remain mandatory', () => {
  const command = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts['venue-oracle:hyperliquid'];
  assert.match(command, /--test field_identities/);
  assert.match(command, /--test nan_capture_policy/);
});

test('Crypto Stark identity supplement runs actual SDK and native consumer', () => {
  const oracle = readFileSync(new URL('../safe-cex-crypto/scripts/oracles.cjs', import.meta.url), 'utf8');
  assert.match(oracle, /starknet\/domain-field-identities\.cjs/);
  assert.match(oracle, /'--test','field_identities'/);
});



test('Lighter shared field supplement remains mandatory in both crypto gates',()=>{
 const scripts=JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
 assert.match(scripts['crypto:lighter:check'],/--test independent_lighter_field_identity/);
 assert.match(readFileSync(new URL('../safe-cex-crypto/scripts/oracles.cjs', import.meta.url), 'utf8'),/'--test','independent_lighter_field_identity'/);
});

test('shared unknown-value get-key gate is mandatory', () => auditJsPrimitive(workflow, 'Check shared unknown-value get-key', 'get_key_unknown --test get_key'));
for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: echo skipped\n']) {
  test(`unknown-value get-key gate mutation fails: ${JSON.stringify(replacement)}`, () => {
    const changed = replacement === ''
      ? workflow.replace(/      - name: Check shared unknown-value get-key\n        run: [^\n]+\n/, '')
      : workflow.replace('      - name: Check shared unknown-value get-key\n', '      - name: Check shared unknown-value get-key\n' + replacement);
    assert.throws(() => auditJsPrimitive(changed, 'Check shared unknown-value get-key', 'get_key_unknown --test get_key'), /mandatory exact command/);
  });
}

// Keep a positive scoped audit even when unrelated venue pins are broken.
function auditCryptocom(source) {
  const job = source.match(/^  types:\n([\s\S]*?)(?=^  [\w-]+:|$(?![\s\S]))/m)?.[1];
  assert.ok(job, 'missing Crypto.com types job');
  assert.doesNotMatch(job, /^    (?:if|continue-on-error|env):/m, 'Crypto.com job may not bypass gates');
  const name = 'Check Crypto.com live TypeScript oracle';
  const block = job.match(/^      - name: Check Crypto\.com live TypeScript oracle\n([\s\S]*?)(?=^      - |$(?![\s\S]))/m)?.[1];
  assert.ok(block, 'missing Check Crypto.com');
  assert.equal((job.match(/^      - name: Check Crypto\.com live TypeScript oracle$/gm) ?? []).length, 1);
  assert.doesNotMatch(block, /^        (?:if|continue-on-error|env|shell):/m, 'Check Crypto.com condition or override');
  assert.equal((block.match(/^        run:/gm) ?? []).length, 1);
  assert.match(block, /^        run: yarn venue-oracle:cryptocom$/m, 'Check Crypto.com command');
  assert.match(block, /^        working-directory: \.$/m);
  const install = job.indexOf('- name: Install isolated oracle dependencies from the repo lockfile');
  assert.ok(install >= 0 && install < job.indexOf('- name: ' + name), 'Crypto.com must follow isolated install');
  assert.equal((source.match(/      - 'packages\/safe-cex\/src\/exchanges\/cryptocom\/\*\*'/g) ?? []).length, 2, 'cryptocom source triggers');
}
test('Crypto.com mandatory wiring passes scoped audit', () => auditCryptocom(workflow));
test('Crypto.com oracle removal is rejected', () => assert.throws(() => auditCryptocom(workflow.replace(/      - name: Check Crypto.com live TypeScript oracle\n[\s\S]*?(?=      - name:)/, '')), /missing Check Crypto.com/));
test('Crypto.com oracle masking is rejected', () => assert.throws(() => auditCryptocom(workflow.replace('run: yarn venue-oracle:cryptocom', 'run: yarn venue-oracle:cryptocom || true')), /Check Crypto.com.*command/));
test('Crypto.com oracle cannot be conditional', () => assert.throws(() => auditCryptocom(workflow.replace('      - name: Check Crypto.com live TypeScript oracle\n', '      - name: Check Crypto.com live TypeScript oracle\n        if: false\n')), /Check Crypto.com.*condition/));
test('Crypto.com root command retains the full scoped suite', () => assert.equal(JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts['venue-oracle:cryptocom'], 'cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --jobs 1 --all-targets -- --include-ignored'));
for (const name of ['Check production accepted sets', 'Check accepted-set negative controls']) {
  test(`removing ${name} is rejected`, () => assert.throws(() => audit(workflow.replace(new RegExp(`      - name: ${name}\\n[\\s\\S]*?(?=      - name:)`), '')), /missing/));
  test(`masking ${name} is rejected`, () => assert.throws(() => audit(workflow.replace(`      - name: ${name}\n`, `      - name: ${name}\n        continue-on-error: true\n`)), /may not continue/));
}
test('accepted-set scripts execute enforcement and assertion controls', () => {
  const scripts = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8')).scripts;
  assert.equal(scripts['parity:accepted'], 'node sdk-rs/safe-cex-parity/scripts/accepted-matrix.mjs');
  assert.equal(scripts['parity:accepted:controls'], 'node sdk-rs/safe-cex-parity/scripts/accepted-controls-runner.mjs');
});

test('parity extractor root wiring and isolated smol-toml resolution fixtures', () => {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const packageJson = JSON.parse(readFileSync(manifestFor(workflowUrlUsed), 'utf8'));
  assert.ok(packageJson.workspaces.includes('sdk-rs/parity/extract-ts'));
  assert.match(packageJson.scripts['parity:extract'], /^node sdk-rs\/parity\/extract-ts\/scripts\/check-dependency\.mjs &&/);
  assert.match(packageJson.scripts['parity:extract:check'], /^node sdk-rs\/parity\/extract-ts\/scripts\/check-dependency\.mjs &&/);

  // These are pure metadata/resolution fixtures, not a real parser install or
  // decode proof. The separate clean-install CLI owns that measured obligation.
  const temporary = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'parity-extractor-guard-fixtures-')));
  const env = { ...process.env };
  delete env.NODE_PATH;
  delete env.NODE_OPTIONS;
  const child = `
    import assert from 'node:assert/strict';
    import path from 'node:path';
    import { createRequire } from 'node:module';
    const { checkExtractorDependency } = await import(process.argv[1]);
    const fixture = process.argv[2];
    const mode = process.argv[3];
    const require = createRequire(path.join(fixture, 'sdk-rs/parity/extract-ts/package.json'));
    const ancestorRequire = createRequire(path.join(path.dirname(fixture), 'package.json'));
    assert.equal(ancestorRequire.resolve('smol-toml'), path.join(path.dirname(fixture), 'node_modules/smol-toml/ancestor.cjs'), 'ancestor decoy must be resolvable independently');
    if (mode === 'installed') {
      assert.equal(require.resolve('smol-toml'), path.join(fixture, 'node_modules/smol-toml/index.cjs'), 'provider must be the fixture-local stub');
      checkExtractorDependency(fixture);
      console.log('fixture-local resolution: pass; provider execution: none');
    } else {
      assert.equal(mode, 'missing-entrypoint');
      assert.throws(() => require.resolve('smol-toml'), { code: 'MODULE_NOT_FOUND' }, 'missing fixture target must not resolve an ancestor provider');
      assert.throws(() => checkExtractorDependency(fixture), /parity extractor dependency: smol-toml is unavailable after install/, 'guard must reject fixture missing provider entrypoint');
      console.log('fixture missing entrypoint: refused; ancestor fallback: none');
    }
  `;
  const writeJson = (name, value) => {
    fs.mkdirSync(path.dirname(name), { recursive: true });
    fs.writeFileSync(name, JSON.stringify(value));
  };
  try {
    // A real decoy ancestor makes accidental ancestor resolution observable.
    const ancestor = path.join(temporary, 'node_modules/smol-toml');
    writeJson(path.join(ancestor, 'package.json'), { name: 'smol-toml', version: '1.4.2', exports: './ancestor.cjs' });
    fs.writeFileSync(path.join(ancestor, 'ancestor.cjs'), 'throw new Error("ancestor provider must not execute");\n');
    for (const mode of ['installed', 'missing-entrypoint']) {
      const fixture = path.join(temporary, mode);
      writeJson(path.join(fixture, 'package.json'), { workspaces: ['sdk-rs/parity/extract-ts'] });
      writeJson(path.join(fixture, 'sdk-rs/parity/extract-ts/package.json'), { dependencies: { 'smol-toml': '1.4.2' } });
      fs.writeFileSync(path.join(fixture, 'yarn.lock'), 'smol-toml@1.4.2:\n  version "1.4.2"\n');
      const provider = path.join(fixture, 'node_modules/smol-toml');
      // An explicit absent export target prevents any host ancestor from
      // satisfying the negative fixture. No previously resolved file is removed.
      writeJson(path.join(provider, 'package.json'), { name: 'smol-toml', version: '1.4.2', exports: mode === 'installed' ? './index.cjs' : './missing-provider.cjs' });
      if (mode === 'installed') fs.writeFileSync(path.join(provider, 'index.cjs'), 'throw new Error("stub parser must not execute");\n');
      const result = spawnSync(process.execPath, ['--no-global-search-paths', '--input-type=module', '-e', child, new URL('../parity/extract-ts/scripts/check-dependency.mjs', import.meta.url).href, fixture, mode], { encoding: 'utf8', env });
      assert.equal(result.status, 0, result.stderr);
      assert.equal(result.stderr, '');
      assert.equal(result.stdout, mode === 'installed' ? 'fixture-local resolution: pass; provider execution: none\n' : 'fixture missing entrypoint: refused; ancestor fallback: none\n');
    }
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});


test('accepted-set control registry pins files and executed mutations', async () => {
  const { verifyControls } = await import('../safe-cex-parity/scripts/accepted-controls-runner.mjs');
  const registry = JSON.parse(readFileSync(new URL('../safe-cex-parity/scripts/accepted-control-registry.json', import.meta.url), 'utf8'));
  assert.match(readFileSync(new URL('../safe-cex-parity/scripts/accepted-controls-runner.mjs', import.meta.url), 'utf8'), /'--test-concurrency=1'/, 'scanner rebuilds and history probes must run serially');
  // a68c3d9e05 added pure/native callable-alias controls in PR 7951; keep
  // both files load-bearing here alongside the registry runner. 40e183a874
  // also added its renamed-registry-artifact test; no old control is retired.
  // Native portfolio, record, field and authored-order probes add four tests;
  // socket/provider coverage adds one test and four compiled production mutations.
  assert.deepEqual(Object.keys(registry.tests).sort(), ["accepted-aliases.test.mjs", "accepted-binance-inherited-constant-output.test.mjs", "accepted-breakout-portfolio-native.test.mjs", "accepted-callable-alias-native.test.mjs", "accepted-callable-alias.test.mjs", "accepted-constant-output-native.test.mjs", "accepted-construction-chain-native.test.mjs", "accepted-contract-files.test.mjs", "accepted-contract-order-native.test.mjs", "accepted-contracts.test.mjs", "accepted-controls.test.mjs", "accepted-defaults.test.mjs", "accepted-exchange-future.test.mjs", "accepted-feature-contracts-native.test.mjs", "accepted-feature-contracts.test.mjs", "accepted-function-pointer-native.test.mjs", "accepted-future-alias-native.test.mjs", "accepted-future-alias.test.mjs", "accepted-glob-exports.test.mjs", "accepted-grvt-fields.test.mjs", "accepted-hyperliquid-projection-native.test.mjs", "accepted-hyperliquid-projection.test.mjs", "accepted-identity-alias.test.mjs", "accepted-inherited-constant-output-native.test.mjs", "accepted-inherited-constant-output.test.mjs", "accepted-inventory.test.mjs", "accepted-json-source-native.test.mjs", "accepted-loader-future-native.test.mjs", "accepted-loader-future.test.mjs", "accepted-local-helpers.test.mjs", "accepted-macro-origins.test.mjs", "accepted-measure-scope.test.mjs", "accepted-model-field-native.test.mjs", "accepted-model-record-native.test.mjs", "accepted-napi-alias.test.mjs", "accepted-node-probes.test.mjs", "accepted-number-output-exports-native.test.mjs", "accepted-number-output-native.test.mjs", "accepted-number-output.test.mjs", "accepted-object-map-construction-native.test.mjs", "accepted-policy-files.test.mjs", "accepted-ratchet.test.mjs", "accepted-reviewed-aliases.test.mjs", "accepted-scanner.test.mjs", "accepted-telemetry-defaults.test.mjs", "accepted-transport-socket-native.test.mjs", "accepted-weex-kline-alias.test.mjs", "accepted-woo-alias.test.mjs", "accepted-ws-auth-helper.test.mjs", "accepted-ws-auth-native.test.mjs"]);
  // 19b18f40d0 adds LocalFuture pure/native controls; retain both files and
  // their named root-identity mutation alongside all existing controls.
  // Numeric registration adds source/compiler identity controls and four real
  // recipe/route/export refusal processes, including the actual SDK compiler path; retain every previous control.
  // Object-map classification adds a compiler-owned missing-entry and wrong-trait
  // control, with restored passes; every earlier test and mutation stays pinned.
  // The complete candle-cache construction chain adds a positive full inventory
  // and four error-policy mutation controls; preserve all earlier registrations.
  // LoaderFuture adds bounded grammar/compiler-identity tests and an executed
  // removed-coverage control; retain every earlier test and mutation.
  // Per-entry observed dispatch adds one refusal/coverage test; all prior
  // registered files, tests and executed mutation processes remain required.
  // Open-JSON native witnesses and supplemental method execution add one
  // refusal/coverage test; retain every previous control and mutation.
  // A self-only Visitor::visit_unit has one constant-input witness, guarded by
  // its exact compiler signature; its refusal test retires no earlier control.
  // Native binary harness coverage adds one test and four executed entry/receipt
  // mutations; production I/O remains unexecuted and no prior control is retired.
  // Ten finite host I/O/config bindings add a body/package/span refusal test;
  // actual cache parser and wrapper execution remain independently required.
  // Sixteen specifically reviewed Exchange defaults add one finite-scope test
  // and two executed source/candidate refusal processes; no prior control retires.
  // Three fixed exchange aliases add eight grammar/identity refusal tests;
  // this registration preserves every earlier control and process mutation.
  // Metadata map guards add one actual SDK test and three widening mutations;
  // all prior named tests and executed mutation receipts remain required.
  // Alias-default expansion adds one lexical-scope test and a concrete-type
  // removed-default control; every preceding test and mutation stays pinned.
  // Reviewed aliases and exchange runtime coverage retain every earlier control.
  // Two exact serializer metadata constants add policy and production-native
  // controls plus six refusal mutations; all prior controls remain required.
  // Exact direct-provider capability constants add two tests and three
  // executed mutations; all serializer and inherited-method controls remain.
  // Eight exact Binance inherited methods add three tests and three executed
  // provenance controls; no existing test or mutation is retired.
  // Glob-export coverage adds three tests and two mutations; literal-expression
  // macro coverage adds two mutations. Retain all earlier registrations.
  assert.equal(Object.values(registry.tests).flat().length, 179, 'registered accepted-set test count');
  // Provider self coverage adds a same-file nested-definition refusal control;
  // consumer recipe/route controls and every earlier mutation remain required.
  assert.equal(registry.mutations.length, 196, 'registered accepted-set mutation count');
  const output = [...Object.values(registry.tests).flat().map((name, i) => `ok ${i + 1} - ${name}`), ...registry.mutations.map(name => `# ${name}: named assertion; process exit 1`), '# fail 0', '# skipped 0', '# todo 0'].join('\n');
  verifyControls(output, registry);
  for (const name of registry.mutations) assert.throws(() => verifyControls(output.replace(`# ${name}: named assertion; process exit 1`, ''), registry), /mutation inventory/);
  for (const name of Object.values(registry.tests).flat()) assert.throws(() => verifyControls(output.replace(`- ${name}`, '- removed'), registry), /test inventory/);
  assert.throws(() => verifyControls(output.replace('# skipped 0', '# skipped 1'), registry), /skipped test/);
});


// Runtime fixture proof is supplemental; the full root install and real semantic
// coverage remain mandatory through audit()'s existing exact expectations.
test('isolated extractor runtime install is mandatory before complete source coverage', () => auditExtractorInstallWiring(workflow));
const extractorProofStep = '      - name: Check isolated parity extractor runtime install\n        working-directory: .\n        run: node sdk-rs/parity/extract-ts/scripts/check-clean-install.mjs\n';
for (const [name, replacement] of [
  ['removed', ''],
  ['conditioned', extractorProofStep.replace('        working-directory:', '        if: false\n        working-directory:')],
  ['failure-masked', extractorProofStep.replace('check-clean-install.mjs\n', 'check-clean-install.mjs || true\n')],
  ['continue-on-error', extractorProofStep.replace('        working-directory:', '        continue-on-error: true\n        working-directory:')],
  ['wrong directory', extractorProofStep.replace('working-directory: .', 'working-directory: sdk-rs')],
  ['extra environment', extractorProofStep.replace('        working-directory:', '        env:\n          NODE_PATH: forged\n        working-directory:')],
  ['duplicate', extractorProofStep + extractorProofStep],
]) test(`isolated extractor install rejects ${name}`, () => assert.throws(() => auditExtractorInstallWiring(workflow.replace(extractorProofStep, replacement)), /extractor install wiring:/));
test('isolated extractor install cannot follow semantic coverage', () => {
  const changed = workflow.replace(extractorProofStep, '').replace('      - name: Check Coinbase live TypeScript oracle', extractorProofStep + '      - name: Check Coinbase live TypeScript oracle');
  assert.throws(() => auditExtractorInstallWiring(changed), /must precede complete root install and semantic coverage/);
});
for (const event of ['push', 'pull_request']) for (const name of ['mirrors/tealstreet-sdk/.github/workflows/rust.yml', 'packages/safe-cex-web/src/candleCache/**', 'packages/safe-cex-server/src/candleCache/**', 'packages/safe-cex-react-native/src/candleCache/**', 'packages/json-parse/src/**']) {
  test(`extractor ${event} trigger retains ${name}`, () => {
    const start = workflow.indexOf(`  ${event}:\n`);
    const changed = workflow.slice(0, start) + workflow.slice(start).replace(`      - '${name}'\n`, '');
    assert.throws(() => auditExtractorInstallWiring(changed), new RegExp(`${event} missing`));
  });
}
