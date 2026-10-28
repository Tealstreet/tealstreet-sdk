// Lint-level pins on .github/workflows/rust-beta-wasm-release.yml: it runs only
// after a Rust SDK master push run (certified by its required coverage) or by dispatch, it proves the source
// is on origin/master before any SDK script runs, honours RUST_BETA_HOLD, and nothing
// builds or publishes until those proofs (and the roster) succeeded.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const workflow = readFileSync(new URL('../../.github/workflows/rust-beta-wasm-release.yml', import.meta.url), 'utf8');

/** The text of one top-level job, from its key to the next job key. */
const job = (name) => {
  const start = workflow.indexOf(`\n  ${name}:\n`);
  assert.ok(start >= 0, `job ${name}`);
  const rest = workflow.slice(start + 1);
  const next = rest.slice(1).search(/\n  [a-z][a-z0-9_-]*:\n/);
  return next < 0 ? rest : rest.slice(0, next + 1);
};

test('the workflow runs only after a Rust SDK push run on master, or by hand', () => {
  const on = workflow.slice(workflow.indexOf('\non:\n'), workflow.indexOf('\npermissions:\n'));
  assert.match(on, /^\non:\n {2}workflow_run:\n {4}workflows: \[Rust SDK\]\n {4}types: \[completed\]\n {4}branches: \[master\]\n {2}workflow_dispatch:\n/);
  for (const trigger of ['push', 'pull_request', 'pull_request_target', 'schedule', 'release']) {
    assert.doesNotMatch(on, new RegExp(`\\n {2}${trigger}:`), trigger);
  }
  assert.match(workflow, /\npermissions:\n {2}contents: read\n/);
  const roster = job('roster');
  assert.doesNotMatch(roster, /workflow_run\.conclusion == 'success'/, 'CPU-only reds must not suppress a release; certified decides');
  for (const superseded of ["github.event.workflow_run.conclusion != 'cancelled'", "github.event.workflow_run.conclusion != 'skipped'"]) {
    assert.ok(roster.includes(superseded), `${superseded}: a superseded master run skips instead of failing`);
  }
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
