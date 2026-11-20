// Lint-level pins on .github/workflows/rust-beta-wasm-release.yml: it runs only
// after a Rust SDK master push run (certified by its required coverage) or by dispatch, it proves the source
// is on origin/master before any SDK script runs, honours RUST_BETA_HOLD, and nothing
// builds or publishes until those proofs (and the roster) succeeded.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

import { assertCertified } from './beta-wasm-release-guard.mjs';

const rustWorkflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const workflow = readFileSync(new URL('../../.github/workflows/rust-beta-wasm-release.yml', import.meta.url), 'utf8');

/** The text of one top-level job, from its key to the next job key. */
const job = (name) => {
  const start = workflow.indexOf(`\n  ${name}:\n`);
  assert.ok(start >= 0, `job ${name}`);
  const rest = workflow.slice(start + 1);
  const next = rest.slice(1).search(/\n  [a-z][a-z0-9_-]*:\n/);
  return next < 0 ? rest : rest.slice(0, next + 1);
};

test('the workflow accepts master push releases and manual cuts, with a completion fallback', () => {
  const on = workflow.slice(workflow.indexOf('\non:\n'), workflow.indexOf('\npermissions:\n'));
  assert.match(on, /^\non:\n {2}workflow_run:\n {4}workflows: \[Rust SDK\]\n {4}types: \[completed\]\n {4}branches: \[master\]\n {2}workflow_dispatch:\n/);
  for (const trigger of ['push', 'pull_request', 'pull_request_target', 'schedule', 'release']) {
    assert.doesNotMatch(on, new RegExp(`\\n {2}${trigger}:`), trigger);
  }
  assert.match(workflow, /\npermissions:\n {2}contents: read\n/);
  const roster = job('roster');
  assert.doesNotMatch(roster, /workflow_run\.conclusion/, 'CPU-only reds must not suppress a release; certified decides');
  for (const condition of ["github.event.workflow_run.event == 'push'",
    "github.event.workflow_run.head_branch == 'master'"]) assert.ok(roster.includes(condition), condition);
});

test('an automatic run builds the certified commit under the tag that names it', () => {
  const roster = job('roster');
  assert.match(roster, /SDK_SHA: \$\{\{ inputs\.sdk_sha \|\| github\.event\.workflow_run\.head_sha \}\}/);
  assert.match(roster, /\[ -n "\$TAG" \] \|\| TAG="rust-beta-wasm-\$\{SDK_SHA:0:11\}"/);
  assert.ok(roster.indexOf('tag must name sdk_sha') < roster.indexOf('echo "sdk_sha=$SDK_SHA"'), 'outputs only after validation');
  for (const name of ['build', 'release']) assert.doesNotMatch(job(name), /\$\{\{ inputs\./, `${name} reads only roster outputs`);
});

test('a hold, an uncertified commit or already-published inputs stop the build and release', () => {
  assert.match(workflow, /\nenv:\n {2}RUST_BETA_HOLD: \$\{\{ vars\.RUST_BETA_HOLD \}\}\n/);
  const roster = job('roster');
  const held = roster.indexOf('beta-wasm-release-guard.mjs held');
  const certified = roster.indexOf('beta-wasm-release-guard.mjs certified');
  const published = roster.indexOf('beta-wasm-release-guard.mjs published');
  const fresh = roster.indexOf('beta-wasm-release-guard.mjs fresh');
  assert.ok(held > 0 && held < certified && certified < published && published < fresh, 'hold, certify, dedupe, then fresh');
  assert.match(job('build'), /\n {4}if: needs\.roster\.outputs\.publish == 'true'\n/);
  assert.match(job('release'), /\n {4}if: needs\.roster\.outputs\.publish == 'true'\n/);
  assert.match(job('release'), /beta-wasm-release-guard\.mjs fresh[\s\S]*beta-wasm-release-guard\.mjs create/, 'the hold is re-read before the tag');
});

test('the roster job proves sdk_sha is on origin/master before any SDK script runs', () => {
  const roster = job('roster');
  assert.match(roster, /fetch-depth: 0/);
  const ancestry = roster.indexOf('git merge-base --is-ancestor "$SDK_SHA" origin/master');
  assert.ok(ancestry >= 0, 'ancestry check against origin/master');
  assert.doesNotMatch(roster, /--is-ancestor "\$SDK_SHA" origin\/(?!master)/, 'master is the only protected line');
  const firstScript = roster.search(/node sdk-rs\//);
  assert.ok(firstScript > ancestry, 'no SDK script runs before the ancestry proof');
});

test('nothing builds or publishes without the roster job, and only the release job can write', () => {
  assert.match(job('build'), /\n {4}needs: roster\n/);
  assert.match(job('release'), /\n {4}needs: \[roster, build\]\n/);
  assert.doesNotMatch(job('roster'), /contents: write/);
  assert.doesNotMatch(job('build'), /contents: write/);
  assert.match(job('release'), /contents: write/);
  assert.match(job('release'), /beta-wasm-release-guard\.mjs create/);
  assert.match(job('release'), /gh release create "\$TAG" --repo "\$GITHUB_REPOSITORY" --verify-tag/);
});

test('an ordinary automatic source superseded by a newer release publishes nothing; a manual cut is not checked', () => {
  const roster = job('roster');
  const certified = roster.indexOf('beta-wasm-release-guard.mjs certified');
  const superseded = roster.indexOf('beta-wasm-release-guard.mjs superseded "$GITHUB_REPOSITORY" "$SDK_SHA"');
  const published = roster.indexOf('beta-wasm-release-guard.mjs published');
  assert.ok(certified < superseded && superseded < published, 'certify, then superseded, then dedupe');
  const check = roster.slice(roster.lastIndexOf('\n', superseded - 120), published);
  assert.match(check, /if \[ "\$\{\{ github\.event_name \}\}" != workflow_dispatch \]; then/);
  assert.match(check, /echo "superseded by \$superseded"\n\s+echo "publish=false" >> "\$GITHUB_OUTPUT"\n\s+exit 0/);
});

test('a landed master push calls the release directly after required coverage, without CPU dependencies', () => {
  const release = rustWorkflow.slice(rustWorkflow.indexOf('\n  beta-wasm-release:\n'));
  assert.match(release, /^\n  beta-wasm-release:\n/);
  assert.match(release, /\n {4}needs: result\n/);
  assert.match(release, /!cancelled\(\)/);
  for (const condition of ["github.event_name == 'push'", "github.ref == 'refs/heads/master'", "needs.result.result == 'success'"])
    assert.ok(release.includes(condition), condition);
  assert.match(release, /uses: \.\/\.github\/workflows\/rust-beta-wasm-release\.yml/);
  assert.match(release, /sdk_sha: \$\{\{ github\.sha \}\}/);
  assert.match(release, /permissions:\n {6}actions: read\n {6}contents: write/);
  assert.doesNotMatch(release, /skip_certification|needs:.*cpu/);
  const required = rustWorkflow.slice(rustWorkflow.indexOf('\n  result:\n'), rustWorkflow.indexOf('\n  beta-wasm-release:\n'));
  assert.match(required, /name: Rust \/ required coverage/);
  assert.doesNotMatch(required, /needs:.*cpu|needs:.*beta-wasm/);
});

test('the reusable entry point binds the master push SHA and exposes no certification override', () => {
  const on = workflow.slice(workflow.indexOf('\non:\n'), workflow.indexOf('\npermissions:\n'));
  const call = on.slice(on.indexOf('\n  workflow_call:\n'));
  assert.match(call, /^\n  workflow_call:\n {4}inputs:\n {6}sdk_sha:\n {8}required: true\n {8}type: string/);
  assert.doesNotMatch(call, /skip_certification|tag:/);
  const roster = job('roster');
  assert.ok(roster.includes("github.event_name == 'push' && github.ref == 'refs/heads/master'"));
  assert.ok(roster.includes('[ "$SDK_SHA" = "$PUSH_SHA" ]'));
  assert.match(roster, /PUSH_SHA: \$\{\{ github\.sha \}\}/);
  assert.match(roster, /if \[ "\$\{\{ github\.event_name \}\}" = workflow_dispatch \] && \[ "\$\{\{ inputs\.skip_certification \}\}" = true \]; then/);
  assert.match(roster, /if \[ "\$\{\{ github\.event_name \}\}" != workflow_dispatch \]; then/, 'both automatic entry points check supersession');
});

test('the early release is eligible only for successful uncancelled master pushes, independent of CPU status', () => {
  const release = rustWorkflow.slice(rustWorkflow.indexOf('\n  beta-wasm-release:\n'));
  const expression = release.match(/\n {4}if: >-\n([\s\S]*?)\n {4}permissions:/)?.[1].trim()
    .replace(/^\$\{\{\s*|\s*\}\}$/g, '');
  assert.ok(expression);
  const eligible = (event, ref, result, cancelled = false) => runInNewContext(expression, {
    github: { event_name: event, ref }, needs: { result: { result }, 'cpu-certify': { result: 'failure' } },
    cancelled: () => cancelled,
  });
  assert.equal(eligible('push', 'refs/heads/master', 'success'), true);
  for (const result of ['failure', 'cancelled', 'skipped', undefined])
    assert.equal(eligible('push', 'refs/heads/master', result), false, String(result));
  for (const event of ['pull_request', 'pull_request_target', 'workflow_dispatch'])
    assert.equal(eligible(event, 'refs/heads/master', 'success'), false, event);
  assert.equal(eligible('push', 'refs/heads/feature', 'success'), false);
  assert.equal(eligible('push', 'refs/heads/master', 'success', true), false);
});

test('the reusable source step rejects a different SHA before exposing build outputs', () => {
  const script = job('roster').match(/run: \|\n([\s\S]*?)(?=\n {6}- uses: actions\/checkout)/)?.[1]
    .replace(/^ {10}/gm, '').replaceAll('${{ github.event_name }}', 'push');
  assert.ok(script);
  const head = 'a'.repeat(40);
  const source = (sha, tag = '') => spawnSync('bash', ['-e', '-c', script], {
    encoding: 'utf8', env: { SDK_SHA: sha, PUSH_SHA: head, TAG: tag, GITHUB_OUTPUT: '/dev/null' },
  });
  assert.equal(source(head).status, 0);
  const mismatch = source('b'.repeat(40));
  assert.equal(mismatch.status, 1);
  assert.match(mismatch.stdout, /sdk_sha must match the master push/);
  assert.equal(source(head, 'rust-beta-wasm-bbbbbbbbbbb').status, 1);
});

// M58: a staged source survived in NX while the one-pending-run collapse
// cancelled its SDK push run and a later descendant published first.
const stagedSdk = '335cab44d92b958de3d9967b7b43d3872f225cfd';
const laterTag = 'rust-beta-wasm-79c0a92fbec';
const publishDecision = (event, { staged = stagedSdk, superseded = laterTag, existing = '', stageError = false } = {}) => {
  const script = job('roster').match(/- name: Publish only new sdk-rs inputs[\s\S]*?run: \|\n([\s\S]*?)(?=\n {6}# Only an authenticated)/)?.[1]
    .replace(/^ {10}/gm, '').replaceAll('${{ github.event_name }}', event);
  assert.ok(script, 'run the real workflow selection block');
  const fakeNode = `node() {
    if [ "$2" = - ]; then
      printf 'guard:staged\\n' >&2
      [ "$STAGE_ERROR" != true ] || return 1
      printf '%s\\n' "$STAGED"
      return
    fi
    printf 'guard:%s\\n' "$2" >&2
    case "$2" in
      superseded) printf '%s\\n' "$SUPERSEDED" ;;
      published) printf '%s\\n' "$EXISTING" ;;
      *) [ "$1" = --input-type=module ] || return 99; printf '%s\\n' fixture-fingerprint ;;
    esac
  }\n`;
  return spawnSync('bash', ['-e', '-o', 'pipefail', '-c', fakeNode + script], {
    encoding: 'utf8', env: { SDK_SHA: stagedSdk, GITHUB_REPOSITORY: 'Tealstreet/tealstreet-sdk',
      GITHUB_OUTPUT: '/dev/stdout', SUPERSEDED: superseded, STAGED: staged, EXISTING: existing,
      STAGE_ERROR: String(stageError) },
  });
};

test('M58: the currently staged 335cab44 remains publishable after descendant release 79c0a92fbec', () => {
  const result = publishDecision('workflow_run');
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^publish=true$/m);
  assert.doesNotMatch(result.stdout, /^publish=false$/m);
});

test('an ordinary superseded source remains skipped and a manual cut remains deliberate', () => {
  const ordinary = publishDecision('workflow_run', { staged: 'a'.repeat(40) });
  assert.equal(ordinary.status, 0, ordinary.stderr);
  assert.match(ordinary.stdout, /^publish=false$/m);
  const manual = publishDecision('workflow_dispatch', { stageError: true });
  assert.equal(manual.status, 0, manual.stderr);
  assert.match(manual.stdout, /^publish=true$/m);
  assert.doesNotMatch(manual.stderr, /guard:(?:superseded|staged)/);
});

// AGT-21: only supersession is exempted; input deduplication and command failures remain authoritative.
test('matching staged inputs still deduplicate and an unproven exemption never publishes', () => {
  const duplicate = publishDecision('workflow_run', { existing: 'rust-beta-wasm-already-published' });
  assert.equal(duplicate.status, 0, duplicate.stderr);
  assert.match(duplicate.stdout, /^publish=false$/m);
  const unproven = publishDecision('workflow_run', { stageError: true });
  assert.notEqual(unproven.status, 0);
  assert.doesNotMatch(unproven.stdout, /^publish=true$/m);
});


test('a latest automatic source needs no staging lookup', () => {
  const result = publishDecision('push', { superseded: '', stageError: true });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^publish=true$/m);
  assert.doesNotMatch(result.stderr, /guard:staged/);
});


const stagedProgram = workflow.match(/<<'STAGED'\n([\s\S]*?)\n {10}STAGED/)?.[1].replace(/^ {10}/gm, '');
const lookupStage = async (reply, { throws = false } = {}) => {
  assert.ok(stagedProgram);
  const logs = [], errors = [], calls = [];
  await runInNewContext(`(async () => { ${stagedProgram} })()`, {
    process: { argv: ['node', '-', 'Tealstreet/tealstreet-sdk', stagedSdk], env: { GH_TOKEN: 'synthetic-read-token' } },
    console: { log: value => logs.push(value), error: value => errors.push(value) },
    fetch: async (url, init) => {
      calls.push({ url, init });
      if (throws) throw new Error('synthetic-private-network-error');
      return { status: reply.status, json: async () => reply.json };
    },
  });
  return { logs, errors, calls };
};

test('the actual inline lookup grants only an exact commit ref using the existing read token', async () => {
  const ref = { ref: 'refs/heads/copybara/sdk-import-staged', object: { type: 'commit', sha: stagedSdk } };
  const valid = await lookupStage({ status: 200, json: ref });
  assert.deepEqual(valid.logs, [stagedSdk]);
  assert.equal(valid.calls.length, 1);
  assert.equal(valid.calls[0].url, 'https://api.github.com/repos/Tealstreet/tealstreet-sdk/git/ref/heads/copybara/sdk-import-staged');
  assert.equal(valid.calls[0].init.method, undefined, 'default GET, never write');
  for (const invalid of [{}, { ...ref, ref: 'refs/heads/master' },
    { ...ref, object: { type: 'tag', sha: stagedSdk } }, { ...ref, object: { type: 'commit', sha: 'a'.repeat(40) } }]) {
    assert.deepEqual((await lookupStage({ status: 200, json: invalid })).logs, []);
  }
});

for (const status of [404, 401, 403, 429, 500, 502]) test(`an absent or unreadable SDK ref grants no exception (${status})`, async () => {
  const result = await lookupStage({ status });
  assert.deepEqual(result.logs, []);
  assert.equal(result.calls.length, 1);
  if (status !== 404) assert.match(result.errors.join(''), new RegExp(`HTTP ${status}`));
});

test('a ref transport failure reveals no raw exception and grants no exception', async () => {
  const result = await lookupStage({}, { throws: true });
  assert.deepEqual(result.logs, []);
  assert.doesNotMatch(result.errors.join(''), /synthetic-private-network-error/);
});

test('M58: the cancelled push is uncertified until its real required-coverage job succeeds on rerun', async () => {
  let coverage = 'cancelled';
  const api = async (_method, path) => {
    if (path.includes('/actions/workflows/rust.yml/runs?')) return { status: 200, json: { workflow_runs: [
      { id: 44586023187, head_sha: stagedSdk, head_branch: 'master', event: 'push', status: 'completed', conclusion: 'cancelled' },
    ] } };
    if (path.includes('/actions/runs/44586023187/jobs?')) return { status: 200, json: { jobs: [
      { name: 'Rust / required coverage', status: 'completed', conclusion: coverage },
    ] } };
    throw new Error(`unexpected route ${path}`);
  };
  await assert.rejects(assertCertified({ api, repo: 'Tealstreet/tealstreet-sdk', sha: stagedSdk }), /refusing to publish uncertified/);
  coverage = 'success';
  await assertCertified({ api, repo: 'Tealstreet/tealstreet-sdk', sha: stagedSdk });
  assert.match(publishDecision('workflow_run').stdout, /^publish=true$/m);
});
