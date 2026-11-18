// Lint-level pins on .github/workflows/rust-beta-wasm-release.yml: it runs only
// after a Rust SDK master push run (certified by its required coverage) or by dispatch, it proves the source
// is on origin/master before any SDK script runs, honours RUST_BETA_HOLD, and nothing
// builds or publishes until those proofs (and the roster) succeeded.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

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

test('an automatic run superseded by a newer release publishes nothing; a manual cut is not checked', () => {
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
