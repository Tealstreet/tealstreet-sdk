// Same-workflow physical evidence only. No lifetime history scan or retry API.
import { createHash } from 'node:crypto';
export const CPU_COHORT_STEPS = Object.freeze({ first: 'Retain CPU runtime capsule', second: 'Retain CPU remeasurement results', plan: 'Retain CPU remeasurement plan', final: 'Retain CPU certificate', strict: 'Retain strict gate reports and artifact budgets', runtime: 'Retain the eligible original runtime bytes' });
const CPU_WORKFLOW_ID = 386369161;
const WORKFLOW_PATH = '.github/workflows/rust.yml';
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const check = (ok, message) => { if (!ok) throw new Error(`CPU cohort: ${message}`); };
const integer = (value) => Number.isSafeInteger(value) && value > 0;
const hash = (value) => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const gitId = (value) => typeof value === 'string' && /^[a-f0-9]{40}$/.test(value);
const bytes = (value) => { check(value instanceof Uint8Array, 'missing original bytes'); return Buffer.from(value); };
const utf8 = (value) => new TextDecoder('utf-8', { fatal: true }).decode(bytes(value));
const canonical = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  check(value === null || ['string', 'boolean'].includes(typeof value) || typeof value === 'number' && Number.isFinite(value), 'non-JSON identity');
  return JSON.stringify(value);
};
const same = (left, right) => canonical(left) === canonical(right);
const id = (value) => { check(integer(value), 'invalid API identity'); return String(value); };
const safePath = (path) => typeof path === 'string' && path.length > 0 && !path.startsWith('/') && !/[\\\0]/.test(path) && path.split('/').every((part) => part && !['.', '..'].includes(part));

const runtimePath = (path) => {
  if (path.startsWith('docs/') || /\.(?:md|rst|adoc)$/i.test(path) || path.startsWith('.git/') || path.startsWith('.aimux/')) return false;
  if (path.startsWith('sdk-rs/harness/out/') || path.startsWith('sdk-rs/target/') || path.startsWith('sdk-rs/hosts/wasm/pkg/')) return false;
  if (/\.(?:test|integration)\.[cm]?[jt]s$/.test(path)) return false;
  return /^(?:sdk-rs|packages|thirdparty|apps)\//.test(path) || path === WORKFLOW_PATH || /^\.github\/actions\/(?:prepare-yarn|install-yarn)\//.test(path) || /^(?:package\.json|yarn\.lock|tsconfig[^/]*\.json|\.yarnrc[^/]*|\.node-version|rust-toolchain(?:\.toml)?)$/.test(path);
};

export function selectRuntimeInputs(gitTreeEntries) {
  check(Array.isArray(gitTreeEntries), 'missing tracked source projection');
  const seen = new Set(), projection = [];
  for (const entry of gitTreeEntries) {
    check(entry && safePath(entry.path) && !seen.has(entry.path), 'invalid/duplicate tracked path');
    seen.add(entry.path);
    if (entry.type === 'tree') continue;
    if (!runtimePath(entry.path)) continue;
    const oid = entry.oid ?? entry.sha;
    check(['100644', '100755', '120000'].includes(entry.mode) && (entry.type === undefined || entry.type === 'blob') && typeof oid === 'string' && /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(oid), 'invalid tracked runtime blob');
    check(entry.oid === undefined || entry.sha === undefined || entry.oid === entry.sha, 'conflicting content identities');
    projection.push([entry.path, entry.mode, oid]);
  }
  check(projection.length > 0, 'empty tracked runtime projection');
  projection.sort((a, b) => Buffer.compare(Buffer.from(a[0]), Buffer.from(b[0])));
  return projection.map(([path, mode, oid]) => ({ path, mode, oid }));
}

export function sourceInputFingerprint(gitTreeEntries) {
  return sha(canonical(selectRuntimeInputs(gitTreeEntries).map(({ path, mode, oid }) => [path, mode, oid])));
}

function caseIdentity(value) {
  check(value && !Array.isArray(value) && typeof value === 'object', 'invalid public case');
  const keys = ['venue', 'tape', 'tapeSha256', 'books', 'private', 'correctnessRuns', 'runs'];
  check(Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key)), 'incomplete/extra case fields');
  check(typeof value.venue === 'string' && /^[a-z0-9-]+$/.test(value.venue) && safePath(value.tape) && hash(value.tapeSha256) && typeof value.books === 'string' && value.private === false && value.correctnessRuns === 1 && value.runs === 9, 'invalid public case identity');
  return Object.fromEntries(keys.map((key) => [key, value[key]]));
}

export function cohortKey({ policySha256, inputsSha256, suite, case: caseValue }) {
  check(hash(policySha256) && hash(inputsSha256) && typeof suite === 'string' && /^[a-z0-9-]+$/.test(suite), 'invalid cohort inputs');
  return sha(canonical({ schema: 1, policySha256, inputsSha256, suite, case: caseIdentity(caseValue) }));
}

// JSON.parse alone silently overwrites duplicate keys. Retain the original bytes
// and refuse ambiguous/nonfinite API or claim JSON before using any identity.
export function parseCohortJson(raw) {
  const text = typeof raw === 'string' ? raw : utf8(raw); let offset = 0;
  const space = () => { while (/[ \t\r\n]/.test(text[offset] ?? '') && offset < text.length) offset++; };
  const string = () => {
    const start = offset++;
    while (offset < text.length) {
      const ch = text[offset++];
      if (ch === '\\') offset++;
      else if (ch === '"') return JSON.parse(text.slice(start, offset));
    }
    throw new Error('CPU cohort: unterminated JSON string');
  };
  const value = () => {
    space(); const ch = text[offset];
    if (ch === '"') return string();
    if (ch === '{') {
      offset++; space(); const result = {}, keys = new Set();
      if (text[offset] === '}') { offset++; return result; }
      while (offset < text.length) {
        space(); check(text[offset] === '"', 'invalid JSON object key'); const key = string();
        check(!keys.has(key), 'duplicate JSON key'); keys.add(key); space(); check(text[offset++] === ':', 'invalid JSON colon');
        Object.defineProperty(result, key, { value: value(), enumerable: true, writable: true, configurable: true }); space();
        const next = text[offset++]; if (next === '}') return result; check(next === ',', 'invalid JSON object');
      }
    } else if (ch === '[') {
      offset++; space(); const result = []; if (text[offset] === ']') { offset++; return result; }
      while (offset < text.length) { result.push(value()); space(); const next = text[offset++]; if (next === ']') return result; check(next === ',', 'invalid JSON array'); }
    } else {
      const match = /^(?:true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)/.exec(text.slice(offset));
      check(match, 'invalid JSON value'); offset += match[0].length; const parsed = JSON.parse(match[0]);
      check(typeof parsed !== 'number' || Number.isFinite(parsed), 'nonfinite JSON number'); return parsed;
    }
    throw new Error('CPU cohort: incomplete JSON');
  };
  const result = value(); space(); check(offset === text.length, 'trailing JSON data'); return result;
}

const instant = (value) => { check(typeof value === 'string' && /^\d{4}-\d\d-\d\dT/.test(value), 'missing execution time'); const result = Date.parse(value); check(Number.isFinite(result), 'invalid execution time'); return result; };
const logLines = (raw) => utf8(raw).split(/\r?\n/).map((line) => line.replace(/^\ufeff/, '')).map((line) => {
  const found = /^(\d{4}-\d\d-\d\dT\S+) (.*)$/.exec(line);
  return found ? { at: instant(found[1]), text: found[2] } : { at: null, text: line };
});
const successfulStep = (job, name) => {
  const steps = job.steps?.filter((step) => step.name === name);
  check(steps?.length === 1 && steps[0].status === 'completed' && steps[0].conclusion === 'success', `missing unique successful step ${name}`);
  const step = steps[0], start = instant(step.started_at), end = instant(step.completed_at) + 1000;
  check(start < end && instant(job.started_at) <= start && end <= instant(job.completed_at) + 1000, 'step outside physical job');
  return { start, end };
};

function loggedCheckout(job, raw) {
  const window = successfulStep(job, 'Run actions/checkout@v7'), lines = logLines(raw), found = [];
  for (let index = 0; index + 1 < lines.length; index++) if (/(?:^|\/)git log -1 --format=%H$/.test(lines[index].text.replace(/^\[command\]/, ''))) {
    check(gitId(lines[index + 1].text.trim()), 'malformed checkout log SHA');
    check([lines[index].at, lines[index + 1].at].every((at) => at !== null && at >= window.start && at < window.end), 'checkout receipt outside checkout step');
    found.push(lines[index + 1].text.trim());
  }
  check(found.length === 1, 'missing/ambiguous logged checkout'); return found[0];
}

function validateRun(run, workflowId) {
  check(run && integer(run.id) && integer(run.run_number) && integer(run.run_attempt) && run.workflow_id === workflowId && run.path === WORKFLOW_PATH && gitId(run.head_sha) && typeof run.head_branch === 'string' && ['push', 'pull_request', 'workflow_dispatch'].includes(run.event), 'invalid workflow run identity');
}

function validateJob(job, run, name, requireSuccess = false) {
  check(job && integer(job.id) && job.run_id === run.id && job.head_sha === run.head_sha && job.head_branch === run.head_branch && integer(job.run_attempt) && job.run_attempt <= run.run_attempt && job.name === name, 'physical job/source mismatch');
  check(job.status === 'completed' && ['success', 'failure', 'cancelled', 'timed_out'].includes(job.conclusion) && integer(job.runner_id) && instant(job.started_at) <= instant(job.completed_at), 'incomplete physical execution');
  if (requireSuccess) check(job.conclusion === 'success' && job.run_attempt === 1, 'measurement not successful attempt1');
}

function artifactIdentity(artifact, run, name) {
  check(artifact && integer(artifact.id) && artifact.name === name && artifact.expired === false && Number.isSafeInteger(artifact.size_in_bytes) && artifact.size_in_bytes > 0 && /^sha256:[a-f0-9]{64}$/.test(artifact.digest), 'invalid exact artifact');
  check(artifact.workflow_run?.id === run.id && artifact.workflow_run.head_sha === run.head_sha && artifact.workflow_run.head_branch === run.head_branch, 'artifact belongs to another run/head');
}

function uploaded(artifact, job, raw, stepName) {
  const window = successfulStep(job, stepName), lines = logLines(raw);
  for (const value of [artifact.created_at, artifact.updated_at]) check(instant(value) >= window.start && instant(value) < window.end, 'artifact outside physical upload step');
  const finalized = `Artifact ${artifact.name} successfully finalized. Artifact ID ${artifact.id}`;
  const success = `Artifact ${artifact.name} has been successfully uploaded! Final size is ${artifact.size_in_bytes} bytes. Artifact ID is ${artifact.id}`;
  const finals = lines.map((line, index) => ({ ...line, index })).filter((line) => line.text === finalized);
  const done = lines.map((line, index) => ({ ...line, index })).filter((line) => line.text === success);
  check(finals.length === 1 && done.length === 1 && finals[0].index < done[0].index, 'missing/duplicate exact artifact upload');
  const prior = lines.slice(0, finals[0].index).map((line, index) => ({ ...line, index })).filter((line) => /^SHA256 digest of uploaded artifact is [a-f0-9]{64}$/.test(line.text));
  const digest = prior.at(-1);
  check(digest?.text === `SHA256 digest of uploaded artifact is ${artifact.digest.slice(7)}`, 'upload digest belongs to different bytes');
  check(!lines.slice(digest.index + 1, finals[0].index).some((line) => line.text.includes(' successfully finalized. Artifact ID ')), 'upload digest belongs to another artifact');
  check([digest, finals[0], done[0]].every((line) => line.at !== null && line.at >= window.start && line.at < window.end), 'upload log outside exact physical step');
}

/** Shared exact upload/physical-execution binding for planner/certifier ZIPs. */
export function authenticateArtifactMetadata({ artifact, run, job, logBytes, stepName, jobName, expectedCheckoutSha }) {
  validateRun(run, CPU_WORKFLOW_ID);
  check(run.run_attempt === 1 && jobName === job?.name, 'artifact authority requires exact attempt1 job');
  let expectedArtifact, expectedStep;
  if (jobName === 'Rust / CPU remeasurement plan') {
    expectedArtifact = 'cpu-plan'; expectedStep = CPU_COHORT_STEPS.plan;
  } else if (jobName === 'Rust / CPU certification audit') {
    throw new Error('CPU cohort: no all-suite audit in this rollout');
  } else {
    const match = /^Rust \/ (replay measurement|CPU remeasurement|replay gate) \(([a-z0-9-]+)\)$/.exec(jobName);
    check(match, 'unknown artifact authority job name');
    const kind = match[1] === 'replay measurement' ? 'first' : match[1] === 'CPU remeasurement' ? 'second' : 'final';
    expectedArtifact = `${kind === 'first' ? 'cpu-cohort' : kind === 'second' ? 'cpu-second' : 'cpu-cert'}-${match[2]}`;
    expectedStep = CPU_COHORT_STEPS[kind];
    if (kind === 'first' && artifact?.name === `strict-replay-${match[2]}`) {
      expectedArtifact = artifact.name; expectedStep = CPU_COHORT_STEPS.strict;
    } else if (kind === 'first' && artifact?.name === `cpu-runtime-${match[2]}`) {
      expectedArtifact = artifact.name; expectedStep = CPU_COHORT_STEPS.runtime;
    }
  }
  check(stepName === expectedStep, 'unknown artifact authority upload step');
  validateJob(job, run, jobName, true); artifactIdentity(artifact, run, expectedArtifact);
  uploaded(artifact, job, logBytes, expectedStep);
  const measuredCheckoutSha = loggedCheckout(job, logBytes);
  check(gitId(expectedCheckoutSha) && measuredCheckoutSha === expectedCheckoutSha, 'artifact checkout differs from authenticated expected source');
  if (run.event !== 'pull_request') check(measuredCheckoutSha === run.head_sha, 'artifact non-PR checkout differs from source');
  return { schema: 1, artifactId: artifact.id, artifactSha256: artifact.digest.slice(7), runId: run.id, attempt: 1, jobId: job.id, runnerId: job.runner_id, measuredCheckoutSha, logSha256: sha(bytes(logBytes)) };
}

/** Downloaded bundles additionally require the exact archive bytes. Metadata-only
 * runtime refs are useful after a successful second producer verified restoration. */
export function authenticateArtifactExecution(input) {
  const receipt = authenticateArtifactMetadata(input), raw = bytes(input.artifactBytes);
  check(raw.length === input.artifact.size_in_bytes && `sha256:${sha(raw)}` === input.artifact.digest, 'artifact ZIP digest/length mismatch');
  return receipt;
}

export function formatMeasurementMarker(kind, key, at) {
  check(['BEGIN', 'END'].includes(kind) && hash(key) && key.length === 64, 'invalid physical measurement marker');
  check(at === new Date(instant(at)).toISOString(), 'invalid physical marker timestamp');
  return `\nCPU_CASE_${kind} ${key} ${at}`;
}

/** Bind a case's payload clock window to literal markers in its physical log. */
export function authenticateMeasurementWindow({ execution, key, job, logBytes }) {
  check(hash(key) && execution && job && /^Rust \/ (?:replay measurement|CPU remeasurement) \([a-z0-9-]+\)$/.test(job.name), 'invalid physical measurement job/key');
  check(integer(job.id) && integer(job.run_id) && integer(job.runner_id) && job.run_attempt === 1 && job.status === 'completed' && job.conclusion === 'success' && execution.attempt === 1 && execution.runId === String(job.run_id) && execution.jobId === String(job.id) && execution.runnerId === String(job.runner_id), 'measurement execution identity differs from API job');
  const start = instant(execution.startedAt), end = instant(execution.completedAt);
  check(start < end && execution.measuredCheckoutSha === loggedCheckout(job, logBytes), 'measurement time/source differs from physical checkout');
  const lines = logLines(logBytes), markers = ['BEGIN', 'END'].map((kind) => lines.map((line, index) => ({ ...line, index })).filter((line) => line.text === `CPU_CASE_${kind} ${key} ${kind === 'BEGIN' ? execution.startedAt : execution.completedAt}`));
  check(markers.every((items) => items.length === 1), 'missing/duplicate exact public case marker');
  // Also reject extra markers for this key with a substituted payload clock.
  check(lines.filter((line) => line.text.startsWith(`CPU_CASE_BEGIN ${key} `) || line.text.startsWith(`CPU_CASE_END ${key} `)).length === 2, 'substituted/duplicate case marker payload');
  const begin = markers[0][0], finish = markers[1][0];
  check(begin.index < finish.index && begin.at !== null && finish.at !== null && start <= begin.at && begin.at <= end && end <= finish.at, 'public case marker ordering/time differs');
  const candidates = (job.steps ?? []).filter((step) => /public/i.test(step.name ?? '') && step.status === 'completed' && step.conclusion === 'success').filter((step) => {
    const window = successfulStep(job, step.name);
    return [start, end, begin.at, finish.at].every((at) => at >= window.start && at < window.end);
  });
  check(candidates.length === 1, 'case markers are outside one unique successful public step');
  return { schema: 1, key, runId: job.run_id, jobId: job.id, runnerId: job.runner_id, stepName: candidates[0].name, startedAt: execution.startedAt, completedAt: execution.completedAt, beginLoggedAt: new Date(begin.at).toISOString(), endLoggedAt: new Date(finish.at).toISOString(), logSha256: sha(bytes(logBytes)) };
}

export function requireGithubResponseSize(length, kind = 'metadata') {
  check(['metadata', 'log', 'artifact', 'runtime-artifact'].includes(kind), 'invalid GitHub response kind');
  const limit = kind === 'runtime-artifact' ? 4 * 1024 ** 3 : 512 * 1024 ** 2;
  check(integer(length) && length <= limit, 'empty/oversized GitHub response');
}

export function createGithubReader({ repository, token, fetch: fetchImpl = globalThis.fetch }) {
  check(typeof repository === 'string' && /^[A-Za-z0-9_-]+\/[A-Za-z0-9_.-]+$/.test(repository) && typeof fetchImpl === 'function', 'invalid GitHub reader');
  check(token === undefined || typeof token === 'string', 'invalid read token');
  const snapshots = [], snapshotKeys = new Set(), logs = new Map(), logIndices = new Map();
  const immutableResponses = new Map(), completedJobs = new Set();
  const get = async (path, binary = false, immutable = false, kind = 'metadata') => {
    if (immutable && immutableResponses.has(path)) {
      const raw = immutableResponses.get(path); requireGithubResponseSize(raw.length, kind);
      return binary ? Buffer.from(raw) : parseCohortJson(raw);
    }
    const url = `https://api.github.com/repos/${repository}/${path}`;
    const headers = { Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28', ...(token ? { Authorization: `Bearer ${token}` } : {}) };
    let response, initialStatus, raw;
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        response = await fetchImpl(url, { method: 'GET', headers, redirect: 'manual', cache: 'no-store' });
        initialStatus = response.status;
        if (binary && response.status === 302) {
          const location = new URL(response.headers.get('location'));
          check(location.protocol === 'https:' && !location.username && !location.password && /(?:\.githubusercontent\.com|\.blob\.core\.windows\.net)$/.test(location.hostname), 'untrusted artifact/log redirect');
          response = await fetchImpl(location.href, { method: 'GET', redirect: 'error', cache: 'no-store' });
        }
        check(response.status === 200, `GitHub evidence unavailable (${response.status})`);
        raw = Buffer.from(await response.arrayBuffer());
        if (!raw.length && immutable && ['artifact', 'runtime-artifact'].includes(kind) && attempt === 0) {
          console.warn(`Retrying immutable artifact read after empty HTTP 200: ${path}`);
          continue;
        }
        requireGithubResponseSize(raw.length, kind);
        break;
      } catch (error) {
        // Retrieval of the same immutable bytes can recover one closed socket.
        // This never repeats a measurement or accepts an HTTP refusal.
        if (!(immutable && binary && attempt === 0 && error instanceof TypeError && error.cause?.code === 'UND_ERR_SOCKET')) throw error;
        console.warn(`Retrying immutable evidence read after socket closure: ${path}`);
      }
    }
    const contentSha256 = sha(raw), snapshotKey = `${path}\0${contentSha256}`;
    if (!snapshotKeys.has(snapshotKey)) {
      snapshots.push({ path, status: initialStatus, finalStatus: response.status, sha256: contentSha256, raw: Buffer.from(raw) }); snapshotKeys.add(snapshotKey);
    }
    if (immutable) immutableResponses.set(path, Buffer.from(raw));
    return binary ? raw : parseCohortJson(raw);
  };
  const list = async (base, field, extra = '', maxPages = 128) => {
    const result = [], seen = new Set(); let total;
    for (let page = 1; page <= maxPages; page++) {
      const data = await get(`${base}?${extra}per_page=100&page=${page}`);
      check(Number.isSafeInteger(data.total_count) && data.total_count >= 0 && Array.isArray(data[field]) && data[field].length <= 100, 'invalid paginated response');
      if (total === undefined) total = data.total_count;
      check(total === data.total_count && total <= maxPages * 100, 'pagination changed/exceeds hard limit');
      for (const row of data[field]) { check(integer(row.id) && !seen.has(row.id), 'duplicate paginated identity'); seen.add(row.id); result.push(row); }
      check(result.length <= total, 'pagination count mismatch');
      if (result.length === total) return result;
      check(data[field].length === 100, 'truncated pagination');
    }
    throw new Error('CPU cohort: pagination hard limit');
  };
  const noteJobs = (jobs) => {
    for (const job of jobs) {
      if (integer(job.id) && job.status === 'completed' && typeof job.completed_at === 'string' && job.conclusion) completedJobs.add(job.id);
      else completedJobs.delete(job.id);
    }
    return jobs;
  };
  const reader = {
    getRun: (runId, attempt) => get(`actions/runs/${id(runId)}${attempt === undefined ? '' : `/attempts/${id(attempt)}`}`),
    getJob: async (jobId) => (noteJobs([await get(`actions/jobs/${id(jobId)}`)]))[0],
    listJobs: async (runId) => noteJobs(await list(`actions/runs/${id(runId)}/jobs`, 'jobs', 'filter=all&')),
    listArtifacts: (runId) => list(`actions/runs/${id(runId)}/artifacts`, 'artifacts'),
    getArtifact: (artifactId) => get(`actions/artifacts/${id(artifactId)}`),
    getArtifactBytes: (artifactId, kind = 'artifact') => {
      check(['artifact', 'runtime-artifact'].includes(kind), 'invalid artifact response kind');
      return get(`actions/artifacts/${id(artifactId)}/zip`, true, true, kind);
    },
    getCommit: (commit) => { check(gitId(commit), 'invalid commit identity'); return get(`git/commits/${commit}`, false, true); },
    getTree: (tree) => { check(gitId(tree), 'invalid tree identity'); return get(`git/trees/${tree}?recursive=1`, false, true); },
    getBlob: (blob) => { check(gitId(blob), 'invalid blob identity'); return get(`git/blobs/${blob}`, false, true); },
    getLog: async (jobId) => {
      const raw = await get(`actions/jobs/${id(jobId)}/logs`, true, completedJobs.has(jobId), 'log');
      if (!logIndices.has(jobId)) logIndices.set(jobId, Math.max(0, snapshots.length - 1));
      logs.set(jobId, Buffer.from(raw)); return raw;
    },
    snapshotCursor: () => snapshots.length,
    snapshots: ({ since = 0 } = {}) => {
      check(Number.isSafeInteger(since) && since >= 0 && since <= snapshots.length, 'invalid snapshot cursor');
      return { responses: snapshots.slice(since).map((entry) => ({ ...entry, raw: Buffer.from(entry.raw) })), logs: [...logs].filter(([jobId]) => logIndices.get(jobId) >= since).map(([jobId, raw]) => ({ jobId, raw: Buffer.from(raw), sha256: sha(raw) })) };
    },
  };
  return Object.freeze(reader);
}
