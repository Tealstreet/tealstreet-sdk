import assert from 'node:assert/strict';
import test from 'node:test';

import { assertCertified, assertFreshTag, assertNotHeld, createAndVerifyTag, publishedFingerprint } from './beta-wasm-release-guard.mjs';

const REPO = 'Tealstreet/tealstreet-sdk';
const TAG = 'rust-beta-wasm-aaaaaaaaaaa';
const SHA = 'a'.repeat(40);
const OTHER = 'b'.repeat(40);

/** A scripted GitHub API: `routes` maps "METHOD path" to a reply; anything else is 404. */
const fakeApi = (routes) => {
  const calls = [];
  const api = async (method, path, body) => {
    calls.push(`${method} ${path}`);
    const reply = routes[`${method} ${path}`];
    if (typeof reply === 'function') return reply(body);
    return reply ?? { status: 404, json: null };
  };
  return { api, calls };
};
const refPath = `/repos/${REPO}/git/ref/tags/${TAG}`;

test('a fresh tag needs authenticated 404s for both the tag and the release', async () => {
  const { api, calls } = fakeApi({});
  await assertFreshTag({ api, repo: REPO, tag: TAG });
  assert.deepEqual(calls, [`GET ${refPath}`, `GET /repos/${REPO}/releases/tags/${TAG}`]);
});

test('an existing tag (no release) is refused, as is an existing release', async () => {
  const tag = fakeApi({ [`GET ${refPath}`]: { status: 200, json: { object: { type: 'commit', sha: OTHER } } } });
  await assert.rejects(assertFreshTag({ api: tag.api, repo: REPO, tag: TAG }), /tag .* already exists/);
  const release = fakeApi({ [`GET /repos/${REPO}/releases/tags/${TAG}`]: { status: 200, json: {} } });
  await assert.rejects(assertFreshTag({ api: release.api, repo: REPO, tag: TAG }), /release .* already exists/);
});

test('any API error other than 404 fails instead of reading as absent', async () => {
  for (const status of [401, 403, 500, 502]) {
    const { api } = fakeApi({ [`GET ${refPath}`]: { status, json: null } });
    await assert.rejects(assertFreshTag({ api, repo: REPO, tag: TAG }), new RegExp(`HTTP ${status}`));
  }
});

test('the tag is created at the exact source and read back before publishing', async () => {
  let created;
  const { api } = fakeApi({
    [`POST /repos/${REPO}/git/refs`]: (body) => {
      created = body;
      return { status: 201, json: {} };
    },
    [`GET ${refPath}`]: () => ({ status: 200, json: { object: { type: 'commit', sha: created.sha } } }),
  });
  await createAndVerifyTag({ api, repo: REPO, tag: TAG, sha: SHA });
  assert.deepEqual(created, { ref: `refs/tags/${TAG}`, sha: SHA });
});

test('a tag that peels to a different commit fails, including through an annotated tag', async () => {
  const lightweight = fakeApi({
    [`POST /repos/${REPO}/git/refs`]: { status: 201, json: {} },
    [`GET ${refPath}`]: { status: 200, json: { object: { type: 'commit', sha: OTHER } } },
  });
  await assert.rejects(createAndVerifyTag({ api: lightweight.api, repo: REPO, tag: TAG, sha: SHA }), /names b+, not the source/);
  const annotated = fakeApi({
    [`POST /repos/${REPO}/git/refs`]: { status: 201, json: {} },
    [`GET ${refPath}`]: { status: 200, json: { object: { type: 'tag', sha: 'c'.repeat(40) } } },
    [`GET /repos/${REPO}/git/tags/${'c'.repeat(40)}`]: { status: 200, json: { object: { type: 'commit', sha: OTHER } } },
  });
  await assert.rejects(createAndVerifyTag({ api: annotated.api, repo: REPO, tag: TAG, sha: SHA }), /not the source/);
});

test('a refused or failed tag creation never proceeds', async () => {
  for (const status of [422, 403, 500]) {
    const { api } = fakeApi({ [`POST /repos/${REPO}/git/refs`]: { status, json: null } });
    await assert.rejects(createAndVerifyTag({ api, repo: REPO, tag: TAG, sha: SHA }), new RegExp(`HTTP ${status}`));
  }
  await assert.rejects(createAndVerifyTag({ api: fakeApi({}).api, repo: REPO, tag: TAG, sha: 'main' }), /full commit SHA/);
});

test('a set hold stops the release and names its reason; an empty one does not', () => {
  assert.throws(() => assertNotHeld('BingX interest fix pending'), /held: BingX interest fix pending/);
  for (const hold of [undefined, '', '  ']) assertNotHeld(hold);
});

const runsPath = `/repos/${REPO}/actions/workflows/rust.yml/runs?head_sha=${SHA}&event=push&branch=master&per_page=20`;
const run = (overrides) => ({ head_sha: SHA, head_branch: 'master', event: 'push', status: 'completed', conclusion: 'success', ...overrides });

test('only a successful Rust SDK master push run for the exact commit certifies it', async () => {
  const ok = fakeApi({ [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run({ conclusion: 'failure' }), run()] } } });
  await assertCertified({ api: ok.api, repo: REPO, sha: SHA });
  for (const runs of [[], [run({ conclusion: 'failure' })], [run({ status: 'in_progress', conclusion: null })],
    [run({ head_sha: OTHER })], [run({ head_branch: 'feat/x' })], [run({ event: 'pull_request' })]]) {
    const { api } = fakeApi({ [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: runs } } });
    await assert.rejects(assertCertified({ api, repo: REPO, sha: SHA }), /no successful Rust SDK master push run/);
  }
  const error = fakeApi({ [`GET ${runsPath}`]: { status: 502, json: null } });
  await assert.rejects(assertCertified({ api: error.api, repo: REPO, sha: SHA }), /HTTP 502/);
});

test('a release whose pin already carries the fingerprint is found, so the same inputs publish once', async () => {
  const releases = [
    { tag_name: 'rust-beta-wasm-ccccccccccc', draft: true, assets: [{ name: 'rust-beta-wasm.pin.json', browser_download_url: 'u/c' }] },
    { tag_name: 'exchange-mcp-v1', assets: [{ name: 'rust-beta-wasm.pin.json', browser_download_url: 'u/x' }] },
    { tag_name: 'rust-beta-wasm-bbbbbbbbbbb', assets: [{ name: 'rust-beta-wasm.pin.json', browser_download_url: 'u/b' }] },
    { tag_name: TAG, assets: [{ name: 'rust-beta-wasm.pin.json', browser_download_url: 'u/a' }] },
  ];
  const pins = { 'u/b': { inputs: { fingerprint: 'old' } }, 'u/a': { inputs: { fingerprint: 'new' } } };
  const fetched = [];
  const fetchImpl = async (url) => { fetched.push(url); return Response.json(pins[url]); };
  const { api } = fakeApi({ [`GET /repos/${REPO}/releases?per_page=30`]: { status: 200, json: releases } });
  assert.equal(await publishedFingerprint({ api, repo: REPO, fingerprint: 'new', fetchImpl }), TAG);
  assert.deepEqual(fetched, ['u/b', 'u/a'], 'drafts and foreign tags are never read');
  assert.equal(await publishedFingerprint({ api, repo: REPO, fingerprint: 'none', fetchImpl }), undefined);
  const error = fakeApi({ [`GET /repos/${REPO}/releases?per_page=30`]: { status: 403, json: null } });
  await assert.rejects(publishedFingerprint({ api: error.api, repo: REPO, fingerprint: 'new', fetchImpl }), /HTTP 403/);
});
