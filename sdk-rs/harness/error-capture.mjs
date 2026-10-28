// Records real venue error replies as corpus fixtures for the beta error
// classification (docs/plans/RUST_ERROR_CLASSIFICATION.md, "Recorded-payload
// corpus"). LOCAL ONLY: it sends real requests, so it never runs in CI, and it
// accepts no credentials. Ruling R4 allows only no-credential captures (public
// endpoints with bad parameters, bogus-key auth replies), few and paced.
//
//   node sdk-rs/harness/error-capture.mjs <cases.json> <venue tests/fixtures/errors dir>
//
// cases.json is [{ name, method, url, headers?, body?, expect: { kind, outcome, path } }].
// Every fixture is redacted with the same field list the Rust loader
// (safe_cex_engine::test_support::error_corpus) re-checks.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export const REDACTED = '<redacted>';
export const MAX_CASES = 12;
export const PACE_MS = 1_500;

/** Field names whose values identify an account, a credential or an order (error_corpus SENSITIVE). */
export const SENSITIVE = new Set([
  'apikey', 'api_key', 'key', 'secret', 'signature', 'sign', 'passphrase', 'token', 'jwt',
  'listenkey', 'uid', 'userid', 'user_id', 'accountid', 'account_id', 'subaccount',
  'address', 'wallet', 'orderid', 'order_id', 'ordid', 'clordid', 'clientoid',
  'clientorderid', 'client_order_id',
]);

const CREDENTIAL_HEADER = /key|sign|secret|passphrase|token|auth|cookie/i;
const CREDENTIAL_FIELD = (name) => SENSITIVE.has(name.toLowerCase()) || name.toLowerCase() === 'password';
const bogus = (value) => String(value).startsWith('bogus-');

/** The first credential-named field at any depth whose value is not a bogus- placeholder. */
const realCredential = (value) => {
  if (Array.isArray(value)) return value.map(realCredential).find(Boolean);
  if (value === null || typeof value !== 'object') return undefined;
  for (const [key, inner] of Object.entries(value)) {
    if (CREDENTIAL_FIELD(key) && (inner === null || typeof inner !== 'object') && !bogus(inner)) return key;
    const nested = realCredential(inner);
    if (nested) return nested;
  }
  return undefined;
};
const HEX_RUN = /[0-9a-fA-F]{40,}/g;

// A JSON "key": value pair; the value is a string or a bare number.
const PAIR = /("([A-Za-z_]+)"\s*:\s*)("(?:[^"\\]|\\.)*"|-?\d[\d.eE+-]*)/g;

/**
 * The reply text verbatim, except that sensitive JSON values and long hex runs
 * become <redacted>. It never re-serializes, so spacing, key order and large
 * integers reach the venue codec exactly as the venue sent them.
 */
export const redactBody = (text) => text
  .replace(PAIR, (pair, prefix, key, value) => (
    SENSITIVE.has(key.toLowerCase()) && value !== '""' ? `${prefix}"${REDACTED}"` : pair
  ))
  .replace(HEX_RUN, REDACTED);

/** Path and query with sensitive parameter values replaced; the host is dropped. */
export const redactEndpoint = (url) => {
  const parsed = new URL(url);
  for (const key of [...parsed.searchParams.keys()]) {
    if (SENSITIVE.has(key.toLowerCase())) parsed.searchParams.set(key, REDACTED);
  }
  const query = parsed.searchParams.toString().replaceAll(encodeURIComponent(REDACTED), REDACTED);
  return (parsed.pathname + (query ? `?${query}` : '')).replace(HEX_RUN, REDACTED);
};

/** Refuses anything outside R4: too many cases, non-https, or a credential-looking header that is not bogus. */
export const checkCases = (cases) => {
  if (!Array.isArray(cases) || cases.length === 0) throw new Error('no capture cases');
  if (cases.length > MAX_CASES) throw new Error(`at most ${MAX_CASES} captures per run`);
  const names = new Set();
  for (const capture of cases) {
    if (!/^[a-z0-9-]+$/.test(capture.name ?? '')) throw new Error(`case name must be kebab-case: ${capture.name}`);
    if (names.has(capture.name)) throw new Error(`duplicate case ${capture.name}`);
    names.add(capture.name);
    if (!String(capture.url).startsWith('https://')) throw new Error(`${capture.name}: https only`);
    for (const [header, value] of Object.entries(capture.headers ?? {})) {
      if (CREDENTIAL_HEADER.test(header) && !bogus(value)) {
        throw new Error(`${capture.name}: header ${header} must carry a bogus- value; no real credentials`);
      }
    }
    for (const [param, value] of new URL(capture.url).searchParams) {
      if (CREDENTIAL_FIELD(param) && !bogus(value)) {
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

export const capture = async (cases, outDir, { fetch = globalThis.fetch, pace = PACE_MS } = {}) => {
  checkCases(cases);
  mkdirSync(outDir, { recursive: true });
  const written = [];
  for (const [index, item] of cases.entries()) {
    if (index > 0) await sleep(pace);
    const response = await fetch(item.url, {
      method: item.method ?? 'GET',
      headers: item.headers ?? {},
      body: item.body === undefined ? undefined : JSON.stringify(item.body),
      // A redirect could carry the case off its https host.
      redirect: 'error',
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
