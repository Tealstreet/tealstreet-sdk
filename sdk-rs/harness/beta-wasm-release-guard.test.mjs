import assert from 'node:assert/strict';
import test from 'node:test';

import { assertCertified, assertFreshTag, assertNotHeld, createAndVerifyTag, publishedFingerprint, supersededBy } from './beta-wasm-release-guard.mjs';

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
const run = (overrides) => ({ id: 7, head_sha: SHA, head_branch: 'master', event: 'push', status: 'completed', conclusion: 'failure', ...overrides });
const jobsPath = (id, page = 1) => `/repos/${REPO}/actions/runs/${id}/jobs?filter=latest&per_page=50&page=${page}`;
const noSleep = async () => {};
const job = (name, conclusion = 'success') => ({ name, status: 'completed', conclusion });
const filler = Array.from({ length: 50 }, (_, i) => job(`Rust / replay gate (${i})`, 'failure'));

test('the required coverage job certifies a master push run, even when CPU jobs are red', async () => {
  const ok = fakeApi({
    [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run()] } },
    [`GET ${jobsPath(7)}`]: { status: 200, json: { jobs: filler } },
    [`GET ${jobsPath(7, 2)}`]: { status: 200, json: { jobs: [job('Rust / shared CPU certification', 'failure'), job('Rust / required coverage')] } },
  });
  await assertCertified({ api: ok.api, repo: REPO, sha: SHA, sleep: noSleep });
});

test('successful required coverage certifies a running master push while a CPU job is unfinished', async () => {
  const { api, calls } = fakeApi({
    [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run({ status: 'in_progress', conclusion: null })] } },
    [`GET ${jobsPath(7)}`]: { status: 200, json: { jobs: filler } },
    [`GET ${jobsPath(7, 2)}`]: {
      status: 200,
      json: { jobs: [job('Rust / required coverage'), { name: 'Rust / Bitget counterbalanced steady CPU', status: 'in_progress', conclusion: null }] },
    },
  });
  await assertCertified({ api, repo: REPO, sha: SHA, sleep: noSleep });
  assert.deepEqual(calls, [`GET ${runsPath}`, `GET ${jobsPath(7)}`, `GET ${jobsPath(7, 2)}`]);
});

test('a running workflow refuses absent, queued, unfinished or failed required coverage', async () => {
  for (const required of [
    undefined,
    { name: 'Rust / required coverage', status: 'queued', conclusion: null },
    { name: 'Rust / required coverage', status: 'in_progress', conclusion: null },
    job('Rust / required coverage', 'failure'),
  ]) {
    const { api } = fakeApi({
      [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run({ status: 'in_progress', conclusion: null })] } },
      [`GET ${jobsPath(7)}`]: {
        status: 200,
        json: { jobs: [{ name: 'Rust / Bitget counterbalanced steady CPU', status: 'in_progress', conclusion: null }, ...(required ? [required] : [])] },
      },
    });
    await assert.rejects(assertCertified({ api, repo: REPO, sha: SHA, sleep: noSleep }), /required coverage success; refusing/);
  }
});

test('a pending second attempt cannot use successful required coverage from the first attempt', async () => {
  const { api, calls } = fakeApi({
    [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run({ status: 'in_progress', conclusion: null, run_attempt: 2 })] } },
    [`GET ${jobsPath(7)}`]: { status: 200, json: { jobs: [{ name: 'Rust / required coverage', status: 'queued', conclusion: null, run_attempt: 2 }] } },
    [`GET ${jobsPath(7).replace('filter=latest', 'filter=all')}`]: { status: 200, json: { jobs: [{ ...job('Rust / required coverage'), run_attempt: 1 }] } },
  });
  await assert.rejects(assertCertified({ api, repo: REPO, sha: SHA, sleep: noSleep }), /required coverage success; refusing/);
  assert.deepEqual(calls, [`GET ${runsPath}`, `GET ${jobsPath(7)}`]);
});

test('a red, missing or unfinished required coverage job, or a run of the wrong kind, refuses', async () => {
  for (const [runs, jobs] of [
    [[run()], [job('Rust / required coverage', 'failure')]],
    [[run()], [job('Rust / native test')]],
    [[run()], [{ name: 'Rust / required coverage', status: 'in_progress', conclusion: null }]],
    [[run({ head_sha: OTHER })], [job('Rust / required coverage')]],
    [[run({ head_branch: 'feat/x' })], [job('Rust / required coverage')]],
    [[run({ event: 'pull_request' })], [job('Rust / required coverage')]],
    [[], []],
  ]) {
    const { api } = fakeApi({
      [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: runs } },
      [`GET ${jobsPath(7)}`]: { status: 200, json: { jobs } },
    });
    await assert.rejects(assertCertified({ api, repo: REPO, sha: SHA, sleep: noSleep }), /required coverage success; refusing/);
  }
  for (const routes of [
    { [`GET ${runsPath}`]: { status: 502, json: null } },
    { [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run()] } }, [`GET ${jobsPath(7)}`]: { status: 403, json: null } },
  ]) {
    const { api } = fakeApi(routes);
    await assert.rejects(assertCertified({ api, repo: REPO, sha: SHA, sleep: noSleep }), /HTTP (502|403)/);
  }
});

test('transient 5xx lookups are retried with backoff; a 4xx or a persistent 5xx still refuses', async () => {
  let jobsCalls = 0;
  const waits = [];
  const flaky = fakeApi({
    [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run()] } },
    [`GET ${jobsPath(7)}`]: () => (++jobsCalls < 3 ? { status: 504, json: null } : { status: 200, json: { jobs: [job('Rust / required coverage')] } }),
  });
  await assertCertified({ api: flaky.api, repo: REPO, sha: SHA, sleep: async (ms) => waits.push(ms) });
  assert.equal(jobsCalls, 3);
  assert.deepEqual(waits, [2000, 4000]);

  const down = fakeApi({
    [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run()] } },
    [`GET ${jobsPath(7)}`]: { status: 502, json: null },
  });
  await assert.rejects(assertCertified({ api: down.api, repo: REPO, sha: SHA, sleep: noSleep }), /HTTP 502/);
  assert.equal(down.calls.filter((call) => call === `GET ${jobsPath(7)}`).length, 3);

  const denied = fakeApi({
    [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run()] } },
    [`GET ${jobsPath(7)}`]: { status: 403, json: null },
  });
  await assert.rejects(assertCertified({ api: denied.api, repo: REPO, sha: SHA, sleep: noSleep }), /HTTP 403/);
  assert.equal(denied.calls.filter((call) => call === `GET ${jobsPath(7)}`).length, 1);
});

test('a large re-run is paged at 50 jobs until the required coverage job appears', async () => {
  const page = (n) => Array.from({ length: 50 }, (_, i) => job(`Rust / wasm family (${n}-${i})`, 'failure'));
  const big = fakeApi({
    [`GET ${runsPath}`]: { status: 200, json: { workflow_runs: [run()] } },
    [`GET ${jobsPath(7)}`]: { status: 200, json: { jobs: page(1) } },
    [`GET ${jobsPath(7, 2)}`]: { status: 200, json: { jobs: page(2) } },
    [`GET ${jobsPath(7, 3)}`]: { status: 200, json: { jobs: [job('Rust / required coverage')] } },
  });
  await assertCertified({ api: big.api, repo: REPO, sha: SHA, sleep: noSleep });
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

// rust-beta-wasm-f5ea907ebd5: its master run finished at 09:46, after 18d192329cd was released.
test('a release built from a descendant supersedes an older run; equal, older or diverged ones do not', async () => {
  const releases = [
    { tag_name: 'rust-beta-wasm-ccccccccccc', draft: true },
    { tag_name: 'rust-beta-wasm-bbbbbbbbbbb' },
    { tag_name: 'rust-beta-wasm-ddddddddddd' },
    { tag_name: 'not-a-beta-release' },
  ];
  const compareTo = (tag) => `GET /repos/${REPO}/compare/${SHA}...${tag}?per_page=1`;
  const scripted = (statuses) =>
    fakeApi({
      [`GET /repos/${REPO}/releases?per_page=30`]: { status: 200, json: releases },
      [compareTo('rust-beta-wasm-bbbbbbbbbbb')]: { status: 200, json: { status: statuses[0] } },
      [compareTo('rust-beta-wasm-ddddddddddd')]: { status: 200, json: { status: statuses[1] } },
    });
  const superseded = scripted(['behind', 'ahead']);
  assert.equal(await supersededBy({ api: superseded.api, repo: REPO, sha: SHA }), 'rust-beta-wasm-ddddddddddd');
  assert.ok(!superseded.calls.some((call) => call.includes('ccccccccccc')), 'drafts are not releases');
  for (const statuses of [['identical', 'behind'], ['diverged', 'behind']]) {
    const { api } = scripted(statuses);
    assert.equal(await supersededBy({ api, repo: REPO, sha: SHA }), undefined, statuses.join());
  }
});

test('a failed release listing or comparison fails instead of reading as not superseded', async () => {
  const listing = fakeApi({ [`GET /repos/${REPO}/releases?per_page=30`]: { status: 502, json: null } });
  await assert.rejects(supersededBy({ api: listing.api, repo: REPO, sha: SHA }), /HTTP 502/);
  const comparison = fakeApi({
    [`GET /repos/${REPO}/releases?per_page=30`]: { status: 200, json: [{ tag_name: 'rust-beta-wasm-bbbbbbbbbbb' }] },
  });
  await assert.rejects(supersededBy({ api: comparison.api, repo: REPO, sha: SHA }), /HTTP 404/);
  await assert.rejects(supersededBy({ api: comparison.api, repo: REPO, sha: 'main' }), /full commit SHA/);
});
