const assert = require('node:assert/strict');
const { existsSync, readFileSync } = require('node:fs');
const { join } = require('node:path');
const test = require('node:test');
const { execFileSync } = require('node:child_process');
const { mkdtempSync, mkdirSync, writeFileSync, chmodSync, symlinkSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { pathToFileURL } = require('node:url');

const workflowPath = join(__dirname, '../workflows/nx-integration.yml');
const workflow = existsSync(workflowPath) ? readFileSync(workflowPath, 'utf8') : '';
const caller = readFileSync(join(__dirname, '../workflows/ci.yml'), 'utf8');

test('SDK PRs run the NX caller gate on free hosted runners without publication authority', () => {
  assert.match(caller, /^  pull_request:/m);
  assert.match(caller, /uses: \.\/\.github\/workflows\/nx-integration.yml/);
  assert.match(workflow, /^  workflow_call:/m);
  assert.match(workflow, /runs-on: ubuntu-latest/);
  assert.match(workflow, /cancel-in-progress: true/);
  assert.doesNotMatch(workflow, /pull_request_target|self-hosted|: write\b|continue-on-error/);
});

test('NX checkout has an immutable resolved revision and no persistent credentials', () => {
  assert.match(workflow, /repository: Tealstreet\/tealstreet-next/);
  assert.match(workflow, /ref: \$\{\{ steps\.nx\.outputs\.sha \}\}/);
  assert.match(workflow, /persist-credentials: false/);
  assert.match(workflow, /NX_INTEGRATION_READ_TOKEN/);
});

test('caller validation includes reader/runbook checks and actual NX gates', () => {
  for (const command of ['unrolled-server-changes.test.mjs', 'rust-release-runbook.test.mjs',
    'yarn typecheck:ci', 'yarn lint:ci', 'yarn test:ci', 'nx-integration.mjs build']) {
    assert.ok(workflow.includes(command), `missing actual gate: ${command}`);
  }
});

test('the NX diagnostic consumer runs only after its TypeScript/Jotai dependencies are installed', () => {
  const install = workflow.indexOf('- name: Install frozen NX dependencies');
  const consumer = workflow.indexOf('scripts/sdk-diagnostic-observability.test.cjs');
  assert.ok(install >= 0 && consumer > install, 'diagnostic consumer requires the frozen NX install first');
});

test('native production Dockerfiles already have a cache-only PR build', () => {
  const image = readFileSync(join(__dirname, '../workflows/docker-native-server-pr.yml'), 'utf8');
  assert.match(image, /dockerfile: sdk-rs\/hosts\/server\/Dockerfile/);
  assert.match(image, /dockerfile: sdk-rs\/hosts\/candle-server\/Dockerfile/);
  assert.match(image, /--output type=cacheonly/);
  assert.doesNotMatch(image, /--push\b|--cache-to\b|: write\b/);
});

const modulePath = pathToFileURL(join(__dirname, '../scripts/nx-overlay.mjs')).href;
const config = `ROOT_SCAFFOLDING = glob(["**"], exclude = ["sdk-rs/**", "packages/**"])
TEALSTREET_SDK_FILES = glob(["mirrors/tealstreet-sdk/**", "sdk-rs/**", "packages/safe-cex/**"])
TEALSTREET_SDK_MIRROR_FILES = glob(["**"], exclude = ["packages/studies/**"])
`;
const git = (root, ...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
const put = (root, path, value) => {
  const file = join(root, path);
  mkdirSync(join(file, '..'), { recursive: true });
  writeFileSync(file, value);
};
const commit = (root) => {
  git(root, 'add', '.');
  git(root, 'commit', '-qm', 'fixture');
  return git(root, 'rev-parse', 'HEAD');
};
function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'nx-integration-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const sdk = join(directory, 'sdk');
  const nx = join(directory, 'nx');
  for (const root of [sdk, nx]) {
    mkdirSync(root);
    git(root, 'init', '-q', '-b', 'master');
    git(root, 'config', 'user.name', 'fixture');
    git(root, 'config', 'user.email', 'fixture@example.invalid');
    git(root, 'config', 'commit.gpgsign', 'false');
    put(root, 'sdk-rs/value.txt', Array.from({ length: 40 }, (_, i) => `line ${i}`).join('\n') + '\n');
    put(root, 'packages/safe-cex/removed.txt', 'remove me\n');
    put(root, 'packages/safe-cex/run.sh', '#!/bin/sh\nexit 0\n');
    put(root, 'packages/safe-cex/data.bin', Buffer.from([0, 1, 2, 3]));
  }
  put(sdk, 'README.md', 'SDK base\n');
  put(nx, 'README.md', 'NX private root stays\n');
  put(nx, 'mirrors/tealstreet-sdk/README.md', 'SDK base\n');
  put(nx, 'mirrors/copy.bara.sky', config);
  put(nx, 'apps/web/caller.ts', 'NX-only caller\n');
  const base = commit(sdk);
  commit(nx);
  return { sdk, nx, base };
}

test('real Git overlay maps edits, deletion, binary and mode changes without replacing NX root', async (t) => {
  const { overlaySdk } = await import(modulePath);
  const f = fixture(t);
  put(f.sdk, 'README.md', 'SDK updated\n');
  put(f.sdk, 'sdk-rs/value.txt', 'updated\n');
  rmSync(join(f.sdk, 'packages/safe-cex/removed.txt'));
  put(f.sdk, 'packages/safe-cex/data.bin', Buffer.from([0, 4, 5, 6]));
  chmodSync(join(f.sdk, 'packages/safe-cex/run.sh'), 0o755);
  const head = commit(f.sdk);
  const result = overlaySdk({ ...f, head });
  assert.equal(result.sdkHead, head);
  assert.equal(result.changed, 5);
  assert.equal(readFileSync(join(f.nx, 'sdk-rs/value.txt'), 'utf8'), 'updated\n');
  assert.equal(readFileSync(join(f.nx, 'mirrors/tealstreet-sdk/README.md'), 'utf8'), 'SDK updated\n');
  assert.equal(readFileSync(join(f.nx, 'README.md'), 'utf8'), 'NX private root stays\n');
  assert.equal(readFileSync(join(f.nx, 'apps/web/caller.ts'), 'utf8'), 'NX-only caller\n');
  assert.equal(existsSync(join(f.nx, 'packages/safe-cex/removed.txt')), false);
  assert.deepEqual(readFileSync(join(f.nx, 'packages/safe-cex/data.bin')), Buffer.from([0, 4, 5, 6]));
  assert.match(git(f.nx, 'ls-tree', 'HEAD', 'packages/safe-cex/run.sh'), /^100755 /);
  assert.equal(git(f.nx, 'status', '--porcelain'), '');
});

test('three-way overlay preserves newer nonoverlapping NX edits to the same file', async (t) => {
  const { overlaySdk } = await import(modulePath);
  const f = fixture(t);
  const file = 'sdk-rs/value.txt';
  put(f.sdk, file, readFileSync(join(f.sdk, file), 'utf8').replace('line 1\n', 'SDK edit\n'));
  put(f.nx, file, readFileSync(join(f.nx, file), 'utf8').replace('line 35\n', 'new NX edit\n'));
  commit(f.nx);
  overlaySdk({ ...f, head: commit(f.sdk) });
  const actual = readFileSync(join(f.nx, file), 'utf8');
  assert.ok(actual.includes('SDK edit\n'));
  assert.ok(actual.includes('new NX edit\n'));
});

test('already-present SDK edits/deletions need no empty synthetic commit', async (t) => {
  const { overlaySdk } = await import(modulePath);
  const f = fixture(t);
  for (const root of [f.sdk, f.nx]) {
    put(root, 'sdk-rs/value.txt', 'already shared\n');
    rmSync(join(root, 'packages/safe-cex/removed.txt'));
  }
  const head = commit(f.sdk);
  const nxHead = commit(f.nx);
  const result = overlaySdk({ ...f, head });
  assert.equal(result.changed, 2);
  assert.equal(result.candidateSha, nxHead);
  assert.equal(git(f.nx, 'status', '--porcelain'), '');
});

test('literal unusual file names survive Git patch mapping', async (t) => {
  const { overlaySdk } = await import(modulePath);
  const f = fixture(t);
  const file = 'packages/safe-cex/space [x]*.txt';
  put(f.sdk, file, 'literal path\n');
  overlaySdk({ ...f, head: commit(f.sdk) });
  assert.equal(readFileSync(join(f.nx, file), 'utf8'), 'literal path\n');
});

test('conflicting SDK/NX edits refuse without a replacement fallback', async (t) => {
  const { overlaySdk } = await import(modulePath);
  const f = fixture(t);
  put(f.sdk, 'sdk-rs/value.txt', 'SDK changed the same line\n');
  put(f.nx, 'sdk-rs/value.txt', 'NX changed the same line\n');
  commit(f.nx);
  const old = git(f.nx, 'rev-parse', 'HEAD');
  assert.throws(() => overlaySdk({ ...f, head: commit(f.sdk) }), /git apply/);
  assert.equal(git(f.nx, 'rev-parse', 'HEAD'), old);
});

for (const [name, path] of [['uncovered', 'packages/new-package/input.txt'], ['excluded', 'packages/studies/input.txt']]) {
  test(`${name} SDK changes refuse rather than disappearing from the consumer check`, async (t) => {
    const { overlaySdk } = await import(modulePath);
    const f = fixture(t);
    put(f.sdk, path, 'new source\n');
    assert.throws(() => overlaySdk({ ...f, head: commit(f.sdk) }), /no NX destination|excluded from its mirror/);
  });
}

test('symlink changes and dirty NX checkouts cannot authorize an overlay', async (t) => {
  const { overlaySdk } = await import(modulePath);
  const f = fixture(t);
  symlinkSync('/outside', join(f.sdk, 'sdk-rs/link'));
  const head = commit(f.sdk);
  assert.throws(() => overlaySdk({ ...f, head }), /regular files/);
  put(f.nx, 'untracked.txt', 'dirty\n');
  assert.throws(() => overlaySdk({ ...f, head }), /dirty/);
});

test('unknown Copybara syntax refuses while literal exclusions are honored', async () => {
  const { pathSet } = await import(modulePath);
  assert.throws(() => pathSet('PATHS = glob(["packages/*"])', 'PATHS'), /Unsupported/);
  assert.throws(() => pathSet('PATHS = dynamic_paths()', 'PATHS'), /Unsupported/);
  const contains = pathSet('PATHS = glob(["**"], exclude=["private/**"])', 'PATHS');
  assert.equal(contains('sdk-rs/value.txt'), true);
  assert.equal(contains('private/secret.txt'), false);
});

test('private checkout credential is restricted to admission, resolution and checkout', () => {
  assert.equal([...workflow.matchAll(/secrets\.NX_INTEGRATION_READ_TOKEN/g)].length, 3);
  assert.match(workflow, /SAME_REPO: \$\{\{ github\.event\.pull_request\.head\.repo\.full_name == github\.repository \}\}/);
  assert.match(workflow, /test -n "\$NX_TOKEN"/);
  assert.doesNotMatch(workflow, /upload-artifact|cache\/save|--push\b|id-token:/);
  assert.doesNotMatch(workflow, /uses: \.\/\.github\/actions\/(?:prepare-yarn|install-yarn)/);
  assert.match(workflow, /--cache-folder "\$RUNNER_TEMP\/nx-yarn"/);
  for (const name of ['Typecheck', 'Lint', 'Test', 'Build']) {
    assert.match(workflow, new RegExp(`- name: ${name}\\n        if: \\$\\{\\{ !cancelled\\(\\) && steps\\.types\\.outcome == 'success' \\}\\}`));
  }
});

test('required ci cannot accept a failed, skipped or cancelled PR consumer gate', () => {
  assert.match(caller, /ci:\n    name: ci\n    needs: \[checks, nx-integration\]\n    if: \$\{\{ always\(\) \}\}/);
  const body = caller.match(/name: Require SDK and NX caller gates[\s\S]*?        run: \|\n([\s\S]*?)(?=\n\n)/)?.[1];
  assert.ok(body);
  const script = body.replace(/^          /gm, '');
  for (const event of ['pull_request', 'push', 'workflow_dispatch']) {
    for (const result of ['success', 'failure', 'skipped', 'cancelled']) {
      for (const checks of ['success', 'failure', 'skipped', 'cancelled']) {
        const { spawnSync } = require('node:child_process');
        const out = spawnSync('bash', ['-e', '-c', script], { env: { ...process.env, CI_EVENT: event, NX_RESULT: result, CHECKS_RESULT: checks } });
        assert.equal(out.status === 0, checks === 'success' && (event === 'pull_request' ? result === 'success' : result === 'skipped'), `${event}/${result}/${checks}`);
      }
    }
  }
});

function wasmFixture(t, { fingerprint = 'published', families = ['alpha'], failBuild = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'nx-build-boundary-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const input = { fingerprint, files: 1, toolchain: { wasmBindgen: '0.2.129', wasmOpt: 'required', profile: 'release', target: 'wasm32-unknown-unknown' } };
  put(root, 'input.json', JSON.stringify(input));
  put(root, 'scripts/rust-beta-wasm.pin.json', JSON.stringify({ status: 'published', inputs: { ...input, fingerprint: 'published' }, families: { alpha: {} } }));
  put(root, 'sdk-rs/hosts/wasm/js/beta-venues.mjs', `export const betaVenueFamilies = ${JSON.stringify(families)};`);
  put(root, 'sdk-rs/harness/beta-wasm-inputs.mjs', `import {readFileSync} from 'node:fs'; export const betaWasmInputs = root => JSON.parse(readFileSync(root+'/input.json'));`);
  put(root, 'scripts/fetch-rust-beta-wasm.mjs', `
    import {mkdirSync,writeFileSync} from 'node:fs';
    export function validatePin(pin,families,inputs) {
      if (pin.status !== 'published' || pin.inputs.fingerprint !== inputs.fingerprint ||
        Object.keys(pin.families).sort().join() !== [...families].sort().join()) throw Error('strict pin refusal');
    }
    export async function installBetaWasm({root,pin,families,inputs}) {
      validatePin(pin,families,inputs);
      for(const family of families){const dir=root+'/sdk-rs/hosts/wasm/pkg/web/'+family;mkdirSync(dir,{recursive:true});writeFileSync(dir+'/safe_cex_wasm_bg.wasm','fixture');writeFileSync(dir+'/safe_cex_wasm.js','fixture');}
    }
  `);
  put(root, 'scripts/build-rust-beta-wasm.mjs', failBuild ? 'process.exit(23)' : `
    import {mkdirSync,writeFileSync} from 'node:fs';
    for(const family of ${JSON.stringify(families)}){const dir='sdk-rs/hosts/wasm/pkg/web/'+family;mkdirSync(dir,{recursive:true});writeFileSync(dir+'/safe_cex_wasm_bg.wasm','candidate');writeFileSync(dir+'/safe_cex_wasm.js','candidate');}
  `);
  put(root, 'mirrors/tealstreet-sdk/.github/workflows/rust.yml', 'WASM_BINDGEN_VERSION: 0.2.129\n');
  put(root, 'apps/web/package.json', JSON.stringify({ scripts: { prebuild: 'exit 73', build: 'printf built > build-proof.txt' } }));
  return { root, receipt: join(root, 'receipt.json') };
}

test('published preparation stays strict; the exact build body runs without the release lifecycle', async (t) => {
  const { prepareWasm, buildNx } = await import(pathToFileURL(join(__dirname, '../scripts/nx-integration.mjs')).href);
  const { root, receipt } = wasmFixture(t);
  const prepared = await prepareWasm(root, receipt, root);
  assert.equal(prepared.mode, 'published');
  put(root, 'tools/corepack', '#!/bin/sh\n[ "$1" = yarn ] && [ "$2" = exec ] && [ "$3" = -- ] || exit 91\nshift 3\nexec "$@"\n');
  chmodSync(join(root, 'tools/corepack'), 0o755);
  const oldPath = process.env.PATH;
  process.env.PATH = `${join(root, 'tools')}:${oldPath}`;
  t.after(() => { process.env.PATH = oldPath; });
  await buildNx(root, receipt, root);
  assert.equal(readFileSync(join(root, 'apps/web/build-proof.txt'), 'utf8'), 'built');
  put(root, 'sdk-rs/hosts/wasm/pkg/web/alpha/safe_cex_wasm.js', 'changed');
  await assert.rejects(buildNx(root, receipt, root), /inputs\/assets changed/);
  put(root, 'sdk-rs/hosts/wasm/pkg/web/alpha/safe_cex_wasm.js', 'fixture');
  const manifest = JSON.parse(readFileSync(join(root, 'apps/web/package.json')));
  manifest.scripts.build = 'exit 1';
  put(root, 'apps/web/package.json', JSON.stringify(manifest));
  await assert.rejects(buildNx(root, receipt, root), /build script changed/);
});

test('changed inputs build fresh families, including a new roster, without rewriting a published pin', async (t) => {
  const { prepareWasm } = await import(pathToFileURL(join(__dirname, '../scripts/nx-integration.mjs')).href);
  const { root, receipt } = wasmFixture(t, { fingerprint: 'candidate', families: ['beta'] });
  const before = readFileSync(join(root, 'scripts/rust-beta-wasm.pin.json'));
  assert.equal((await prepareWasm(root, receipt, root)).mode, 'candidate');
  assert.deepEqual(readFileSync(join(root, 'scripts/rust-beta-wasm.pin.json')), before);
  assert.equal(readFileSync(join(root, 'sdk-rs/hosts/wasm/pkg/web/beta/safe_cex_wasm_bg.wasm'), 'utf8'), 'candidate');
});

test('candidate builder failure cannot produce a prepared receipt', async (t) => {
  const { prepareWasm } = await import(pathToFileURL(join(__dirname, '../scripts/nx-integration.mjs')).href);
  const { root, receipt } = wasmFixture(t, { fingerprint: 'candidate', failBuild: true });
  await assert.rejects(prepareWasm(root, receipt, root), /candidate-wasm failed \(23\)/);
  assert.equal(existsSync(receipt), false);
});

test('matching-input roster mismatch and malformed publication both refuse, rather than selecting fresh build', async (t) => {
  const { planWasm } = await import(pathToFileURL(join(__dirname, '../scripts/nx-integration.mjs')).href);
  const a = wasmFixture(t, { families: ['beta'] });
  await assert.rejects(planWasm(a.root), /strict pin refusal/);
  const b = wasmFixture(t, { fingerprint: 'candidate' });
  const pin = JSON.parse(readFileSync(join(b.root, 'scripts/rust-beta-wasm.pin.json')));
  pin.status = 'pending';
  put(b.root, 'scripts/rust-beta-wasm.pin.json', JSON.stringify(pin));
  await assert.rejects(planWasm(b.root), /strict pin refusal/);
});

test('private command output is withheld while a nonzero child stays a failure', async (t) => {
  const { runNxCommand } = await import(pathToFileURL(join(__dirname, '../scripts/nx-command.mjs')).href);
  const directory = mkdtempSync(join(tmpdir(), 'nx-command-output-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  assert.throws(() => runNxCommand(directory, 'test', process.execPath,
    ['-e', 'console.error("PRIVATE_FIXTURE_OUTPUT");process.exit(17)'], directory), /test failed \(17\)/);
  assert.match(readFileSync(join(directory, 'nx-test.log'), 'utf8'), /PRIVATE_FIXTURE_OUTPUT/);
  assert.equal(require('node:fs').statSync(join(directory, 'nx-test.log')).mode & 0o777, 0o600);
});
