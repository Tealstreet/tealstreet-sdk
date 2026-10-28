// Records real venue error replies as corpus fixtures for the beta error
// classification (docs/plans/RUST_ERROR_CLASSIFICATION.md, "Recorded-payload
// corpus"). LOCAL ONLY: it sends real requests, so it never runs in CI, and it
// accepts no credentials. Ruling R4 allows only no-credential captures (public
// endpoints with bad parameters, bogus-key auth replies), few and paced, and
// claude-ddi2il approves each list before it runs.
//
//   node sdk-rs/harness/error-capture.mjs <cases.json> <venue tests/fixtures/errors dir>
//
// cases.json is [{ name, method, url, headers?, body?, expect: { kind, outcome, path } }].
// "{now}" anywhere in the url, a header value or a body string is replaced by the
// send-time epoch milliseconds, so a signed timestamp is never stale on arrival.
// Redaction follows sdk-rs/engine/src/error_corpus_redaction.json, the same rules
// the Rust loader (safe_cex_engine::test_support::error_corpus) re-checks with.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export const REDACTED = '<redacted>';
// Ruling R4's live-request budget: 12 per run, and never more than that to one host.
export const MAX_CASES = 12;
export const MAX_PER_HOST = 12;
export const PACE_MS = 1_500;
export const TIMEOUT_MS = 10_000;

export const RULES = JSON.parse(
  readFileSync(new URL('../engine/src/error_corpus_redaction.json', import.meta.url), 'utf8'),
);

/** Lowercased with _ and - removed, a listed name or a key ending with a listed suffix. */
export const isSensitive = (key) => {
  const normal = key.replace(/[_-]/g, '').toLowerCase();
  return RULES.names.includes(normal) || RULES.suffixes.some((suffix) => normal.endsWith(suffix));
};

// ---- Redaction works in place on the text, so everything else stays byte-exact. ----

const skipSpace = (text, at) => {
  let index = at;
  while (index < text.length && /\s/.test(text[index])) index += 1;
  return index;
};

/** The index just past the JSON string that opens at `at`. */
const stringEnd = (text, at) => {
  let index = at + 1;
  while (index < text.length && text[index] !== '"') index += text[index] === '\\' ? 2 : 1;
  return Math.min(index + 1, text.length);
};

/** The index just past the JSON value that starts at `at`. */
const valueEnd = (text, at) => {
  if (text[at] === '"') return stringEnd(text, at);
  if (text[at] !== '{' && text[at] !== '[') {
    let index = at;
    while (index < text.length && !/[\s,}\]]/.test(text[index])) index += 1;
    return index;
  }
  let depth = 0;
  let index = at;
  while (index < text.length) {
    if (text[index] === '"') {
      index = stringEnd(text, index);
      continue;
    }
    if (text[index] === '{' || text[index] === '[') depth += 1;
    if (text[index] === '}' || text[index] === ']') depth -= 1;
    index += 1;
    if (depth === 0) break;
  }
  return index;
};

/** A string holding JSON is redacted inside, and re-encoded only when that changed it. */
const nestedString = (token) => {
  let inner;
  try {
    inner = JSON.parse(token);
  } catch {
    return token;
  }
  if (typeof inner !== 'string' || !/^\s*[[{]/.test(inner)) return token;
  const redacted = redactJson(inner);
  return redacted === inner ? token : JSON.stringify(redacted);
};

/** Every sensitive key's whole value (string, number, array or object) becomes "<redacted>". */
const redactJson = (text) => {
  let out = '';
  let index = 0;
  while (index < text.length) {
    if (text[index] !== '"') {
      out += text[index];
      index += 1;
      continue;
    }
    const end = stringEnd(text, index);
    const token = text.slice(index, end);
    const colon = skipSpace(text, end);
    if (text[colon] !== ':') {
      out += nestedString(token);
      index = end;
      continue;
    }
    let key = '';
    try {
      key = JSON.parse(token);
    } catch {
      // Not a well-formed key: copied as it is.
    }
    const start = skipSpace(text, colon + 1);
    out += text.slice(index, start);
    const stop = valueEnd(text, start);
    const value = text.slice(start, stop);
    if (isSensitive(key) && value !== '""' && value !== 'null') out += `"${REDACTED}"`;
    else if (value.startsWith('"')) out += nestedString(value);
    else if (value.startsWith('{') || value.startsWith('[')) out += redactJson(value);
    else out += value;
    index = stop;
  }
  return out;
};

const ipv4 = (run) => {
  const parts = run.replace(/^\.+|\.+$/g, '').split('.');
  return parts.length === 4 && parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) <= 255);
};
const ipv6 = (run) => {
  const groups = run.split(':');
  const filled = groups.filter(Boolean).length;
  return groups.every((group) => group.length <= 4)
    && ((groups.length === 8 && filled === 8) || (run.includes('::') && filled >= 2));
};
const uuid = (run) => run.split('-').map((part) => part.length).join() === '8,4,4,4,12';
const base58 = (run) => run.length >= 32 && !/[0OIl]/.test(run)
  && /\d/.test(run) && /[A-Z]/.test(run) && /[a-z]/.test(run);

/** Identifying free text (the loader's identifying_text), replaced run by run. */
export const redactText = (text) => text
  .replace(/[0-9a-fA-F]+/g, (run) => (run.length >= 40 ? REDACTED : run))
  .replace(/[0-9.]+/g, (run) => (ipv4(run) ? run.replace(/^(\.*).*?(\.*)$/, `$1${REDACTED}$2`) : run))
  .replace(/[0-9a-fA-F:]+/g, (run) => (ipv6(run) ? REDACTED : run))
  .replace(/[0-9a-fA-F-]+/g, (run) => (uuid(run) ? REDACTED : run))
  .replace(/[0-9A-Za-z]+/g, (run) => (base58(run) ? REDACTED : run));

/** The reply text with sensitive JSON values and identifying text replaced; nothing re-serialized. */
export const redactBody = (text) => redactText(redactJson(text));

/** Path and query with sensitive parameter values replaced; the host is dropped. */
export const redactEndpoint = (url) => {
  const parsed = new URL(url);
  for (const key of [...parsed.searchParams.keys()]) {
    if (isSensitive(key)) parsed.searchParams.set(key, REDACTED);
  }
  const query = parsed.searchParams.toString().replaceAll(encodeURIComponent(REDACTED), REDACTED);
  return redactText(parsed.pathname + (query ? `?${query}` : ''));
};

// ---- R4 guard: every case is checked before any request is sent. ----

const CREDENTIAL_HEADER = /key|sign|secret|passphrase|token|auth|cookie|session|bearer|jwt/i;
const placeholder = (value) => value === '' || /^(Bearer |Basic )?bogus-/.test(String(value));

/** The first credential-named field at any depth that is not a bogus- (or empty) scalar. */
const realCredential = (value) => {
  if (Array.isArray(value)) return value.map(realCredential).find(Boolean);
  if (value === null || typeof value !== 'object') return undefined;
  for (const [key, inner] of Object.entries(value)) {
    if (isSensitive(key) && (inner === null || typeof inner === 'object' || !placeholder(inner))) return key;
    const nested = realCredential(inner);
    if (nested) return nested;
  }
  return undefined;
};

export const checkCases = (cases) => {
  if (!Array.isArray(cases) || cases.length === 0) throw new Error('no capture cases');
  if (cases.length > MAX_CASES) throw new Error(`at most ${MAX_CASES} captures per run`);
  const names = new Set();
  const perHost = new Map();
  for (const capture of cases) {
    if (!/^[a-z0-9-]+$/.test(capture.name ?? '')) throw new Error(`case name must be kebab-case: ${capture.name}`);
    if (names.has(capture.name)) throw new Error(`duplicate case ${capture.name}`);
    names.add(capture.name);
    if (!String(capture.url).startsWith('https://')) throw new Error(`${capture.name}: https only`);
    const url = new URL(capture.url);
    perHost.set(url.host, (perHost.get(url.host) ?? 0) + 1);
    if (perHost.get(url.host) > MAX_PER_HOST) throw new Error(`at most ${MAX_PER_HOST} captures per host per run`);
    for (const [header, value] of Object.entries(capture.headers ?? {})) {
      if (CREDENTIAL_HEADER.test(header) && !placeholder(value)) {
        throw new Error(`${capture.name}: header ${header} must carry a bogus- value; no real credentials`);
      }
    }
    for (const [param, value] of url.searchParams) {
      if (isSensitive(param) && !placeholder(value)) {
        throw new Error(`${capture.name}: query ${param} must carry a bogus- value; no real credentials`);
      }
    }
    const field = realCredential(capture.body);
    if (field) throw new Error(`${capture.name}: body ${field} must carry a bogus- value; no real credentials`);
    if (!capture.expect?.path) throw new Error(`${capture.name}: expect.path is required`);
  }
  return cases;
};

/** One fixture for the Rust loader, from a reply already received. */
export const fixture = (capture, status, text, now = new Date()) => ({
  source: 'live',
  captured_at: now.toISOString(),
  endpoint: redactEndpoint(capture.url),
  http_status: status,
  body: redactBody(text),
  expect: capture.expect,
});

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const NOW = /\{now\}/g;

/** The case with every "{now}" replaced by `now`, taken immediately before sending. */
export const stamped = (item, now) => {
  const stamp = (value) => {
    if (typeof value === 'string') return value.replace(NOW, String(now));
    if (Array.isArray(value)) return value.map(stamp);
    if (value !== null && typeof value === 'object') {
      return Object.fromEntries(Object.entries(value).map(([key, inner]) => [key, stamp(inner)]));
    }
    return value;
  };
  return { ...item, url: stamp(item.url), headers: stamp(item.headers), body: stamp(item.body) };
};

export const capture = async (
  cases,
  outDir,
  { fetch = globalThis.fetch, pace = PACE_MS, now = Date.now } = {},
) => {
  checkCases(cases);
  mkdirSync(outDir, { recursive: true });
  const written = [];
  for (const [index, original] of cases.entries()) {
    if (index > 0) await sleep(pace);
    const item = stamped(original, now());
    const response = await fetch(item.url, {
      method: item.method ?? 'GET',
      headers: item.headers ?? {},
      body: item.body === undefined ? undefined : JSON.stringify(item.body),
      // A 3xx is recorded as itself; following it could carry the case off its host.
      redirect: 'manual',
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    const path = join(outDir, `${item.name}.json`);
    writeFileSync(path, `${JSON.stringify(fixture(item, response.status, await response.text()), null, 2)}\n`);
    written.push(path);
  }
  return written;
};

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [casesPath, outDir] = process.argv.slice(2);
  if (!casesPath || !outDir) {
    console.error('usage: node sdk-rs/harness/error-capture.mjs <cases.json> <out dir>');
    process.exit(2);
  }
  const written = await capture(JSON.parse(readFileSync(casesPath, 'utf8')), outDir);
  for (const path of written) console.log(path);
}
