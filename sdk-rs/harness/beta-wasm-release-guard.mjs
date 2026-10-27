// Tag custody for the Rust beta wasm release. Only an authenticated 404 counts as
// absent (any other API answer fails), and the tag is created, then read back and
// peeled to the exact source commit, before any release is published under it.
// A release also needs no hold, a successful Rust SDK master push run for its commit,
// and inputs no earlier release already carries.
//   RUST_BETA_HOLD=<reason> node sdk-rs/harness/beta-wasm-release-guard.mjs held
//   node sdk-rs/harness/beta-wasm-release-guard.mjs fresh     <owner/repo> <tag> <sha>
//   node sdk-rs/harness/beta-wasm-release-guard.mjs create    <owner/repo> <tag> <sha>
//   node sdk-rs/harness/beta-wasm-release-guard.mjs certified <owner/repo> <sha>
//   node sdk-rs/harness/beta-wasm-release-guard.mjs published <owner/repo> <fingerprint>
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

const REQUIRED_JOB = 'Rust / required coverage';

/** A set RUST_BETA_HOLD variable stops every release, naming why. */
export const assertNotHeld = (hold) => {
  if (hold?.trim()) throw new Error(`beta wasm releases are held: ${hold.trim()}`);
};

/**
 * `sha` must have a completed Rust SDK master push run whose "Rust / required coverage"
 * job (the correctness aggregate branch protection requires) succeeded. CPU
 * certification is non-blocking, so its noise must not stop a release.
 */
export const assertCertified = async ({ api, repo, sha }) => {
  if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error('source must be a full commit SHA');
  const { status, json } = await api('GET', `/repos/${repo}/actions/workflows/rust.yml/runs?head_sha=${sha}&event=push&branch=master&per_page=20`);
  if (status !== 200) throw new Error(`Rust SDK run lookup returned HTTP ${status}`);
  const runs = (json?.workflow_runs ?? []).filter((run) => run.head_sha === sha && run.head_branch === 'master' && run.event === 'push' && run.status === 'completed');
  for (const run of runs) {
    for (let page = 1; page <= 5; page += 1) {
      const jobs = await api('GET', `/repos/${repo}/actions/runs/${run.id}/jobs?filter=latest&per_page=100&page=${page}`);
      if (jobs.status !== 200) throw new Error(`Rust SDK jobs lookup returned HTTP ${jobs.status}`);
      const required = (jobs.json?.jobs ?? []).find((job) => job.name === REQUIRED_JOB);
      if (required) {
        if (required.status === 'completed' && required.conclusion === 'success') return;
        break;
      }
      if ((jobs.json?.jobs ?? []).length < 100) break;
    }
  }
  throw new Error(`no Rust SDK master push run for ${sha} with ${REQUIRED_JOB} success; refusing to publish uncertified source`);
};

/** The tag of a published beta release whose pin carries `fingerprint`, or undefined. */
export const publishedFingerprint = async ({ api, repo, fingerprint, fetchImpl = fetch }) => {
  const { status, json } = await api('GET', `/repos/${repo}/releases?per_page=30`);
  if (status !== 200) throw new Error(`release listing returned HTTP ${status}`);
  for (const release of json ?? []) {
    if (release.draft || !/^rust-beta-wasm-[0-9a-f]{11}$/.test(release.tag_name ?? '')) continue;
    const asset = release.assets?.find((candidate) => candidate.name === 'rust-beta-wasm.pin.json');
    if (!asset) continue;
    const response = await fetchImpl(asset.browser_download_url, { redirect: 'follow' });
    if (!response.ok) throw new Error(`${release.tag_name} pin returned HTTP ${response.status}`);
    if ((await response.json()).inputs?.fingerprint === fingerprint) return release.tag_name;
  }
  return undefined;
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
  const [mode, repo, ...args] = process.argv.slice(2);
  const token = process.env.GH_TOKEN;
  try {
    assertNotHeld(process.env.RUST_BETA_HOLD);
    if (mode === 'held') process.exit(0);
    if (!token) throw new Error('GH_TOKEN is required; an unauthenticated 404 is not proof of absence');
    const api = githubApi(token);
    if (mode === 'fresh') await assertFreshTag({ api, repo, tag: args[0] });
    else if (mode === 'create') await createAndVerifyTag({ api, repo, tag: args[0], sha: args[1] });
    else if (mode === 'certified') await assertCertified({ api, repo, sha: args[0] });
    else if (mode === 'published') console.log((await publishedFingerprint({ api, repo, fingerprint: args[0] })) ?? '');
    else throw new Error(`unknown mode ${mode}`);
  } catch (error) {
    console.error(`beta wasm release guard: ${error.message}`);
    process.exit(1);
  }
}
