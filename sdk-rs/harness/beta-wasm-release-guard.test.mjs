import assert from 'node:assert/strict';
import test from 'node:test';

import { assertFreshTag, createAndVerifyTag } from './beta-wasm-release-guard.mjs';

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
