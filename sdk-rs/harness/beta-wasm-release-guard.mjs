// Tag custody for the Rust beta wasm release. Only an authenticated 404 counts as
// absent (any other API answer fails), and the tag is created, then read back and
// peeled to the exact source commit, before any release is published under it.
//   node sdk-rs/harness/beta-wasm-release-guard.mjs fresh   <owner/repo> <tag> <sha>
//   node sdk-rs/harness/beta-wasm-release-guard.mjs create  <owner/repo> <tag> <sha>
import { pathToFileURL } from 'node:url';

/** `api(method, path, body?)` resolves to `{ status, json }`; it never throws for an HTTP status. */
const absent = async (api, path, what) => {
  const { status } = await api('GET', path);
  if (status === 404) return;
  if (status === 200) throw new Error(`${what} already exists; releases are immutable, choose a new commit`);
  throw new Error(`${what} lookup returned HTTP ${status}; refusing to treat it as absent`);
};

/** Neither the tag nor a release under it may exist yet. */
export const assertFreshTag = async ({ api, repo, tag }) => {
  await absent(api, `/repos/${repo}/git/ref/tags/${encodeURIComponent(tag)}`, `tag ${tag}`);
  await absent(api, `/repos/${repo}/releases/tags/${encodeURIComponent(tag)}`, `release ${tag}`);
};

/** Resolve a tag ref to the commit it names, peeling an annotated tag object. */
export const peeledTarget = async ({ api, repo, tag }) => {
  const ref = await api('GET', `/repos/${repo}/git/ref/tags/${encodeURIComponent(tag)}`);
  if (ref.status !== 200) throw new Error(`tag ${tag} lookup returned HTTP ${ref.status}`);
  let object = ref.json.object;
  for (let depth = 0; object?.type === 'tag'; depth += 1) {
    if (depth > 4) throw new Error(`tag ${tag} peels through too many tag objects`);
    const peeled = await api('GET', `/repos/${repo}/git/tags/${object.sha}`);
    if (peeled.status !== 200) throw new Error(`tag object for ${tag} returned HTTP ${peeled.status}`);
    object = peeled.json.object;
  }
  if (object?.type !== 'commit') throw new Error(`tag ${tag} does not name a commit`);
  return object.sha;
};

/** Create the lightweight tag at `sha`, then read it back; a mismatch fails before publishing. */
export const createAndVerifyTag = async ({ api, repo, tag, sha }) => {
  if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error('source must be a full commit SHA');
  const created = await api('POST', `/repos/${repo}/git/refs`, { ref: `refs/tags/${tag}`, sha });
  if (created.status !== 201) throw new Error(`creating tag ${tag} returned HTTP ${created.status}`);
  const target = await peeledTarget({ api, repo, tag });
  if (target !== sha) throw new Error(`tag ${tag} names ${target}, not the source ${sha}`);
};

const githubApi = (token) => async (method, path, body) => {
  const response = await fetch(`https://api.github.com${path}`, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
      ...(body ? { 'Content-Type': 'application/json' } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await response.text();
  return { status: response.status, json: text ? JSON.parse(text) : null };
};

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const [mode, repo, tag, sha] = process.argv.slice(2);
  const token = process.env.GH_TOKEN;
  try {
    if (!token) throw new Error('GH_TOKEN is required; an unauthenticated 404 is not proof of absence');
    const api = githubApi(token);
    if (mode === 'fresh') await assertFreshTag({ api, repo, tag });
    else if (mode === 'create') await createAndVerifyTag({ api, repo, tag, sha });
    else throw new Error(`unknown mode ${mode}`);
  } catch (error) {
    console.error(`beta wasm release guard: ${error.message}`);
    process.exit(1);
  }
}
