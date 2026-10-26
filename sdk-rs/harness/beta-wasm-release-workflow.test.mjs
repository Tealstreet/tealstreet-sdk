// Lint-level pins on .github/workflows/rust-beta-wasm-release.yml: it can only be
// dispatched, it proves the source is on origin/master before any SDK script runs,
// and nothing builds or publishes until that proof (and the roster) succeeded.
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

test('the workflow can only be dispatched by hand', () => {
  const on = workflow.slice(workflow.indexOf('\non:\n'), workflow.indexOf('\npermissions:\n'));
  assert.match(on, /^\non:\n {2}workflow_dispatch:\n/);
  for (const trigger of ['push', 'pull_request', 'pull_request_target', 'schedule', 'workflow_run', 'release']) {
    assert.doesNotMatch(on, new RegExp(`\\n {2}${trigger}:`), trigger);
  }
  assert.match(workflow, /\npermissions:\n {2}contents: read\n/);
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
