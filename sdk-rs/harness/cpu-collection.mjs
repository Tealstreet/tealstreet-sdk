// First-run collection only. Numerical eligibility/certification belongs to the
// separate CPU policy and provenance modules; strict report bytes stay intact.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { gunzipSync } from 'node:zlib';
import { parseArgs, isDeepStrictEqual } from 'node:util';
import { expectedGateCases, gateCaseKey } from './ci-gate-suites.mjs';
import { venueOf } from './venues.mjs';
import { captureCache } from './cpu-cache.mjs';
import { performanceOrder } from './performance-policy.mjs';
import { cohortKey, parseCohortJson, selectRuntimeInputs, sourceInputFingerprint } from './cpu-cohort.mjs';
import { sealRuntime, verifyRuntime, cacheDigest } from './cpu-runtime.mjs';

const OUTPUT = 'sdk-rs/harness/out/cpu-collection';
const CACHE = `${OUTPUT}/tmp`;
const POLICY = 'sdk-rs/harness/cpu-remeasure.mjs';
const MODULE = 'sdk-rs/hosts/wasm/pkg/node';
const MODULE_FILES = ['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'package.json'];
const sha = (value) => createHash('sha256').update(value).digest('hex');
const fail = (message) => { throw new Error(`CPU collection: ${message}`); };
const check = (condition, message) => { if (!condition) fail(message); };
const positive = (value) => Number.isSafeInteger(value) && value > 0;
const canonical = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  check(value === null || ['string', 'boolean'].includes(typeof value) || typeof value === 'number' && Number.isFinite(value), 'non-JSON metadata');
  return JSON.stringify(value);
};
const json = (value) => `${canonical(value)}\n`;
const within = (name, directory) => name === directory || name.startsWith(`${directory}/`);
const rootIdentity = (root) => {
  check(typeof root === 'string' && path.isAbsolute(root) && path.resolve(root) === root && fs.realpathSync(root) === root, 'workspace root must be canonical');
  return root;
};
const relative = (name) => {
  check(typeof name === 'string' && name.length > 0 && !name.includes('\\') && !name.includes('\0') && !name.includes(':') && !path.isAbsolute(name) && name.split('/').every(part => part && !['.', '..', '.git'].includes(part)), 'unsafe workspace path');
  return name;
};
function file(root, name, base = root) {
  check(typeof name === 'string', 'invalid workspace filename');
  if (path.isAbsolute(name)) check(path.resolve(name) === name, 'absolute evidence/source path is not canonical');
  const absolute = path.isAbsolute(name) ? name : path.resolve(root, relative(name));
  const suffix = path.relative(base, absolute); check(suffix && !suffix.startsWith('..') && !path.isAbsolute(suffix), 'file escapes permitted root');
  const parts = path.relative(root, absolute).split(path.sep); let current = root;
  for (const part of parts) {
    current = path.join(current, part); const stat = fs.lstatSync(current);
    check(!stat.isSymbolicLink(), 'evidence/source path has symlink');
  }
  const stat = fs.lstatSync(absolute);
  check(stat.isFile() && stat.nlink === 1, 'evidence/source must be an independent regular file');
  check(fs.realpathSync(absolute) === absolute, 'evidence/source canonical path differs');
  return absolute;
}
function read(root, name, base) { return fs.readFileSync(file(root, name, base)); }
function mkdirWorkspace(root, name) {
  let current = root;
  for (const part of relative(name).split('/')) {
    current = path.join(current, part);
    try {
      const stat = fs.lstatSync(current); check(stat.isDirectory() && !stat.isSymbolicLink(), 'output ancestor is not a regular directory');
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      fs.mkdirSync(current, { mode: 0o755 });
    }
  }
}
function git(root, ...args) {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  check(result.status === 0, `cannot authenticate local git ${args[0]}`); return result.stdout;
}
export function readSourceTree(root) {
  rootIdentity(root);
  const head = git(root, 'rev-parse', '--verify', 'HEAD').trim();
  check(/^[a-f0-9]{40}$/.test(head), 'invalid measured checkout');
  const entries = git(root, 'ls-tree', '-r', '-z', 'HEAD').split('\0').filter(Boolean).map(line => {
    const match = /^(\d{6}) (blob|commit) ([a-f0-9]{40})\t([\s\S]+)$/.exec(line);
    check(match, 'malformed tracked source tree');
    return { path: match[4], mode: match[1], type: match[2], oid: match[3] };
  });
  const selected = selectRuntimeInputs(entries), paths = new Set(selected.map(entry => entry.path));
  const changed = git(root, 'diff', '--no-ext-diff', '--name-only', '-z', 'HEAD').split('\0').filter(Boolean);
  check(!changed.some(name => paths.has(name)), 'tracked runtime/build inputs are dirty');
  // Unknown untracked runtime code cannot be imported without entering the
  // source quota key. Ignored dependencies/generated tapes are sealed separately.
  const untracked = git(root, 'ls-files', '--others', '--exclude-standard', '-z').split('\0').filter(Boolean);
  for (const name of untracked) {
    let selectedPath = false;
    try { selectedPath = selectRuntimeInputs([{ path: name, type: 'blob', mode: '100644', oid: '0'.repeat(40) }]).length > 0; }
    catch (error) { if (!error.message.includes('empty tracked runtime projection')) throw error; }
    check(!selectedPath, 'untracked runtime/build source');
  }
  return { head, entries, selected, inputsSha256: sourceInputFingerprint(entries) };
}
function tapeInfo(root, item) {
  const raw = read(root, item.tape);
  const records = (item.tape.endsWith('.gz') ? gunzipSync(raw) : raw).toString('utf8').trim().split('\n').map(line => parseCohortJson(line));
  check(records.length > 0 && records.every(row => row && typeof row === 'object' && !Array.isArray(row)), 'invalid fixture records');
  const venue = venueOf(item.venue);
  const privateValue = records.find(row => row.type === 'config')?.private ?? records.find(row => row.type === 'meta')?.private ?? records.some(row => venue.isPrivateRecord ? venue.isPrivateRecord(row) : Boolean(venue.privateSocket && row.url?.includes(venue.privateSocket)));
  check(typeof privateValue === 'boolean', 'ambiguous fixture privacy');
  return { private: privateValue, tapeSha256: sha(raw) };
}
function publicCases(root, suite) {
  const result = [];
  for (const item of expectedGateCases(suite)) {
    // Private sanitized tapes are generated later in the ordinary workflow.
    // A missing tracked fixture is never interpreted as a private case.
    if (!fs.existsSync(path.resolve(root, item.tape))) {
      check(item.tape.startsWith('sdk-rs/harness/tapes/') && item.tape.endsWith('-private.jsonl'), 'missing required public/fixture case');
      continue;
    }
    const info = tapeInfo(root, item);
    if (!info.private) result.push({ ...item, ...info, correctnessRuns: 1, runs: 9 });
  }
  check(result.length > 0 && new Set(result.map(gateCaseKey)).size === result.length, 'empty/duplicate public roster');
  return result;
}
/** Explicit files, dependency trees, and shallow workspace-directory anchors.
 * Ignored package logs/build caches never enter by recursive package selection. */
export function selectCollectionRuntimeInputs({ root, source, cases, mode = 'family' }) {
  rootIdentity(root); check(['family', 'combined'].includes(mode), 'invalid module collection mode');
  const paths = new Set(source.selected.map(entry => entry.path));
  const directoryAnchors = new Set();
  const dependencyCandidates = new Set(['node_modules']);
  const packageMetadata = parseCohortJson(read(root, 'package.json'));
  const workspaces = Array.isArray(packageMetadata.workspaces) ? packageMetadata.workspaces : packageMetadata.workspaces?.packages;
  check(Array.isArray(workspaces) && workspaces.length > 0 && new Set(workspaces).size === workspaces.length, 'missing/duplicate tracked workspace declarations');
  const patterns = workspaces.map((pattern) => {
    relative(pattern);
    check(pattern.split('/').every(part => part === '*' || /^[A-Za-z0-9_.-]+$/.test(part)), 'unsupported workspace pattern');
    return new RegExp(`^${pattern.split('/').map(part => part === '*' ? '[^/]+' : part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('/')}$`);
  });
  for (const entry of source.entries) if (entry.path.endsWith('/package.json')) {
    const directory = entry.path.slice(0, -'/package.json'.length);
    if (patterns.some(pattern => pattern.test(directory))) {
      directoryAnchors.add(directory); paths.add(entry.path); dependencyCandidates.add(`${directory}/node_modules`);
    }
  }
  for (const entry of source.selected) {
    const parts = entry.path.split('/');
    for (let index = 1; index < parts.length; index++) dependencyCandidates.add(`${parts.slice(0, index).join('/')}/node_modules`);
  }
  for (const item of cases) paths.add(item.tape);
  for (const name of MODULE_FILES) { const selected = `${MODULE}/${name}`; file(root, selected); paths.add(selected); }
  check(fs.lstatSync(path.join(root, 'node_modules')).isDirectory(), 'missing root installed dependencies');
  for (const name of dependencyCandidates) if (fs.existsSync(path.join(root, name))) {
    const stat = fs.lstatSync(path.join(root, name)); check(stat.isDirectory() && !stat.isSymbolicLink(), 'dependency root is not a regular directory'); paths.add(name);
  }
  const minimal = [...paths].sort().filter((name, index, all) => !all.some((other, otherIndex) => otherIndex !== index && within(name, other)));
  return { paths: minimal, directoryAnchors: [...directoryAnchors].sort() };
}
function writeExclusive(filename, value) { fs.writeFileSync(filename, json(value), { flag: 'wx', mode: 0o600 }); }
function replaceState(filename, state) {
  const temp = `${filename}.next`; writeExclusive(temp, state); fs.renameSync(temp, filename);
}
function metadata({ suite, job, run, source }) {
  check(run && positive(run.id) && positive(run.run_number) && run.run_attempt === 1 && /^[a-f0-9]{40}$/.test(run.head_sha) && ['push', 'pull_request', 'workflow_dispatch'].includes(run.event), 'invalid attempt1 workflow metadata');
  check(job && positive(job.id) && positive(job.runner_id) && job.run_id === run.id && job.run_attempt === 1 && job.head_sha === run.head_sha && job.name === `Rust / replay measurement (${suite})`, 'invalid physical first job metadata');
  if (run.event === 'pull_request' && source.head !== run.head_sha) {
    // Shallow checkouts hide parents from revision walks, but the immutable
    // commit object still records the exact synthetic merge's parent order.
    const header = git(source.root, 'cat-file', '-p', source.head).split('\n\n', 1)[0];
    const parents = [...header.matchAll(/^parent ([a-f0-9]{40})$/gm)].map(match => match[1]);
    check(parents.length === 2 && parents[1] === run.head_sha, 'measured PR checkout not bound to tested head');
  } else check(source.head === run.head_sha, 'measured checkout differs from workflow head');
  return { run: { id: run.id, run_number: run.run_number, run_attempt: 1, head_sha: run.head_sha, event: run.event }, job: { id: job.id, runner_id: job.runner_id, run_id: job.run_id, run_attempt: 1, head_sha: job.head_sha, name: job.name } };
}
const core = state => Object.fromEntries(['schema', 'root', 'suite', 'mode', 'run', 'job', 'sourceHead', 'inputsSha256', 'policySha256', 'runtimeSha256', 'destination', 'cachePath', 'nodePath', 'environment', 'claims', 'expectedCases', 'manifestSha256'].map(key => [key, state[key]]));
function loadState(value, root) {
  const filename = typeof value === 'string' ? path.resolve(root ?? process.cwd(), value) : value?.statePath;
  check(typeof filename === 'string', 'missing collection state');
  const suffix = `/${OUTPUT}/state.json`;
  check(filename.endsWith(suffix), 'state outside fixed collection output');
  const workspace = rootIdentity(root ?? (typeof value === 'object' ? value.root : filename.slice(0, -suffix.length)));
  const disk = parseCohortJson(read(workspace, filename)); rootIdentity(disk.root);
  check(filename === disk.statePath && filename === path.join(disk.root, OUTPUT, 'state.json') && (root === undefined || root === disk.root), 'state/root path mismatch');
  file(disk.root, filename);
  check(disk.schema === 1 && disk.cachePath === CACHE && disk.nodePath === path.join(disk.root, '.cpu-runtime/node') && sha(canonical(core(disk))) === disk.staticSha256, 'collection state identity drift');
  if (typeof value === 'object') check(value.staticSha256 === disk.staticSha256, 'foreign/stale collection state');
  const tinyRoot = path.join(disk.root, OUTPUT, 'cohort');
  check(sha(read(disk.root, path.join(tinyRoot, 'manifest.json'))) === disk.manifestSha256, 'tiny runtime manifest changed');
  const bundle = parseCohortJson(read(disk.root, path.join(tinyRoot, 'cohort.json')));
  check(isDeepStrictEqual(bundle, { schema: 1, suite: disk.suite, claims: disk.claims }), 'premeasurement claim bundle changed');
  check(Array.isArray(disk.records) && new Set(disk.records.map(row => row.key)).size === disk.records.length, 'duplicate case evidence roster');
  return disk;
}
function environmentSafe() {
  for (const key of ['NODE_OPTIONS', 'NODE_PATH', 'TSX_DISABLE_CACHE', 'TSX_TSCONFIG_PATH']) check(!process.env[key], `unsupported ambient ${key}`);
}
export function prepareCollection({ root = process.cwd(), suite, destination, environment, job, run, mode = 'family' }) {
  root = rootIdentity(root); environmentSafe();
  check(['family', 'combined'].includes(mode), 'invalid collection mode');
  const source = { ...readSourceTree(root), root }, cases = publicCases(root, suite), identity = metadata({ suite, job, run, source });
  const output = path.join(root, OUTPUT); check(!fs.existsSync(output), 'collection output collision');
  // Refuse accidental reuse of a family archive as a combined build: both modes
  // seal the three files in actual pkg/node directly, without ci-replay metadata.
  const selection = selectCollectionRuntimeInputs({ root, source, cases, mode });
  mkdirWorkspace(root, `${OUTPUT}/tmp`); fs.chmodSync(path.join(output, 'tmp'), 0o755);
  const runtime = sealRuntime({ root, destination, ...selection, cachePath: CACHE, environment });
  const cacheEntries = runtime.manifest.cacheEntries.filter(entry => within(entry.path, CACHE));
  check(cacheEntries.length === 1 && cacheEntries[0].path === CACHE && cacheEntries[0].type === 'directory', 'initial dedicated cache is not empty');
  const policySha256 = sha(read(root, POLICY));
  const claims = cases.map(caseValue => {
    const claim = { schema: 1, suite, case: caseValue, policySha256, inputsSha256: source.inputsSha256, measuredCheckoutSha: source.head, runId: run.id, runNumber: run.run_number, attempt: 1, firstJobId: job.id, runtimeSha256: runtime.fingerprint };
    return { ...claim, key: cohortKey(claim) };
  });
  const tiny = path.join(output, 'cohort'); fs.mkdirSync(tiny); fs.mkdirSync(path.join(output, 'cases'));
  const manifestBytes = fs.readFileSync(path.join(destination, 'manifest.json'));
  fs.writeFileSync(path.join(tiny, 'manifest.json'), manifestBytes, { flag: 'wx', mode: 0o600 });
  writeExclusive(path.join(tiny, 'cohort.json'), { schema: 1, suite, claims });
  const state = { schema: 1, root, suite, mode, ...identity, sourceHead: source.head, inputsSha256: source.inputsSha256, policySha256,
    runtimeSha256: runtime.fingerprint, destination, cachePath: CACHE, nodePath: runtime.nodePath, environment: runtime.manifest.environment,
    claims, expectedCases: expectedGateCases(suite).map(item => ({ ...item })), manifestSha256: sha(manifestBytes), records: [], active: null, statePath: path.join(output, 'state.json') };
  state.staticSha256 = sha(canonical(core(state))); writeExclusive(state.statePath, state);
  return state;
}
function options(args) {
  if (!Array.isArray(args)) { check(args && typeof args === 'object', 'missing gate arguments'); return args; }
  return parseArgs({ args, strict: true, options: { venue: { type: 'string', default: 'binance' }, tape: { type: 'string' }, books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' }, runs: { type: 'string' }, report: { type: 'string' }, 'ci-wasm': { type: 'string' }, 'recorded-evidence': { type: 'string' }, 'no-egress': { type: 'boolean' } } }).values;
}
function verify(state) {
  const runtime = verifyRuntime({ root: state.root, directory: state.destination, environment: state.environment });
  check(runtime.fingerprint === state.runtimeSha256 && runtime.nodePath === state.nodePath, 'sealed runtime identity changed'); return runtime;
}
/** Restore the same empty dedicated loader-cache start for each selected case. */
export function resetCollectionCache({ root, cachePath, destination, environment, runtimeSha256, nodePath }) {
  rootIdentity(root);
  const runtime = verifyRuntime({ root, directory: destination, environment });
  const expected = runtime.manifest.cachePath;
  const requested = path.isAbsolute(cachePath) ? path.relative(root, cachePath).split(path.sep).join('/') : cachePath;
  check(expected === CACHE && requested === expected && runtime.fingerprint === runtimeSha256 && runtime.nodePath === nodePath, 'cache reset differs from authenticated runtime');
  const entries = runtime.manifest.cacheEntries.filter(entry => within(entry.path, expected));
  check(entries.length === 1 && entries[0].path === expected && entries[0].type === 'directory', 'sealed initial loader cache is not empty');
  const cache = path.join(root, expected);
  check(!fs.lstatSync(cache).isSymbolicLink(), 'dedicated cache replaced by symlink');
  fs.rmSync(cache, { recursive: true }); fs.mkdirSync(cache, { mode: entries[0].mode }); fs.chmodSync(cache, entries[0].mode);
  const initialCacheSha256 = cacheDigest(root, expected);
  check(initialCacheSha256 === runtime.initialCacheDigest, 'initial empty loader cache differs from sealed state');
  return { ...runtime, cacheDigest: initialCacheSha256, initialCacheSha256, env: { TMPDIR: cache, TMP: cache, TEMP: cache } };
}
/** Private cases return null and keep their original invocation/cache untouched. */
export function beforeCpuCase({ state: stateValue, args, root }) {
  const state = loadState(stateValue, root), values = options(args); environmentSafe();
  check(state.expectedCases.some(item => gateCaseKey(item) === gateCaseKey(values)), 'gate case outside suite roster');
  const info = tapeInfo(state.root, values);
  if (info.private) return null;
  const claim = state.claims.find(item => gateCaseKey(item.case) === gateCaseKey(values));
  check(claim && claim.case.tapeSha256 === info.tapeSha256 && String(values.runs) === '9' && !values['recorded-evidence'], 'public case/hash/runs differ from premeasurement claim');
  check(!state.active && !state.records.some(item => item.key === claim.key), 'public measurement already started/consumed');
  if (values['ci-wasm'] !== undefined) fail('collection uses sealed actual pkg/node; ci-wasm override is not permitted');
  const runtime = resetCollectionCache(state), initialCacheSha256 = runtime.initialCacheSha256;
  const startedAt = new Date(Date.now()).toISOString();
  const marker = { schema: 1, key: claim.key, claim, startedAt, initialCacheSha256, staticSha256: state.staticSha256 };
  const markerPath = path.join(state.root, OUTPUT, 'cases', `${claim.key}.start.json`); writeExclusive(markerPath, marker);
  state.active = { key: claim.key, markerPath, markerSha256: sha(read(state.root, markerPath)) }; replaceState(state.statePath, state);
  return { ...marker, root: state.root, statePath: state.statePath, markerPath, nodePath: state.nodePath, env: runtime.env, environment: state.environment };
}
export function capturePublicRaw({ root, report }) {
  rootIdentity(root);
  const outputRoot = path.join(root, 'sdk-rs/harness/out');
  const slots = [...['ts', 'rust'].map(impl => ({ impl, mode: 'correctness', run: 0 })), ...Array.from({ length: 9 }, (_, run) => performanceOrder(run).map(impl => ({ impl, mode: 'performance', run }))).flat()];
  check(Array.isArray(report.artifacts) && report.artifacts.length === slots.length, 'missing full public raw inventory');
  const names = new Set(), inodes = new Set();
  return report.artifacts.map((slot, index) => {
    check(isDeepStrictEqual({ impl: slot.impl, mode: slot.mode, run: slot.run }, slots[index]), 'raw slots reordered/missing/duplicated');
    const filename = file(root, slot.path, outputRoot), stat = fs.lstatSync(filename), inode = `${stat.dev}:${stat.ino}`;
    check(!names.has(filename) && !inodes.has(inode), 'raw files duplicated/aliased'); names.add(filename); inodes.add(inode);
    const data = fs.readFileSync(filename), parsed = parseCohortJson(data); check(parsed?.impl === slot.impl, 'raw implementation differs');
    const bytes = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(data);
    return { ...slots[index], bytes, sha256: sha(data) };
  });
}
export function afterCpuCase({ context, reportPath, exitCode }) {
  if (context === null) return null;
  check(context && [0, 1].includes(exitCode), 'invalid completed public replay/exit');
  const state = loadState(context.statePath, context.root);
  check(state.active?.key === context.key && state.active.markerPath === context.markerPath && state.staticSha256 === context.staticSha256 && !state.records.some(item => item.key === context.key), 'missing/foreign/consumed public start marker');
  const markerBytes = read(state.root, context.markerPath), marker = parseCohortJson(markerBytes);
  check(sha(markerBytes) === state.active.markerSha256 && marker.key === context.key && marker.startedAt === context.startedAt && isDeepStrictEqual(marker.claim, context.claim), 'public start marker changed');
  // Capture immediately after the measured gate returns, before parsing reports
  // or invoking any offline TS loader/validator that could warm the cache.
  const completedAt = new Date(Date.now()).toISOString(), finalCacheSha256 = cacheDigest(state.root, state.cachePath);
  check(Date.parse(completedAt) > Date.parse(marker.startedAt), 'invalid public execution time window');
  const prefix = `cpu-collection/cache/${context.key}`;
  const cache = captureCache({ root: state.root, cachePath: state.cachePath, destination: `sdk-rs/harness/out/${prefix}` });
  check(cache.digest === finalCacheSha256, 'final cache changed before snapshot'); verify(state);
  const outputRoot = path.join(state.root, 'sdk-rs/harness/out'), reportRaw = read(state.root, reportPath, outputRoot), report = parseCohortJson(reportRaw);
  const caseValue = marker.claim.case;
  check(report.schema === 1 && report.private === false && report.venue === caseValue.venue && report.tape === caseValue.tape && report.books === caseValue.books && report.tapeSha256 === caseValue.tapeSha256 && report.runs === 9 && report.correctnessRuns === 1 && ['PASS', 'FAIL'].includes(report.status) && (report.status === 'PASS') === (exitCode === 0), 'strict report differs from measured case/exit');
  const raw = capturePublicRaw({ root: state.root, report });
  const manifest = verify(state).manifest;
  const evidence = { schema: 1, key: context.key, claim: marker.claim, cache: { prefix, digest: cache.digest, manifestSha256: cache.manifestSha256 }, report, reportBytes: new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(reportRaw), reportSha256: sha(reportRaw), exitCode, raw,
    execution: { runId: String(state.run.id), jobId: String(state.job.id), runnerId: String(state.job.runner_id), attempt: 1, startedAt: marker.startedAt, completedAt,
      measuredCheckoutSha: state.sourceHead, inputsSha256: state.inputsSha256, policySha256: state.policySha256, runtimeSha256: state.runtimeSha256,
      nodeSha256: manifest.node.sha256, initialCacheSha256: marker.initialCacheSha256, finalCacheSha256,
      imageOS: state.environment.ImageOS, imageVersion: state.environment.ImageVersion, architecture: state.environment.arch, nodeVersion: state.environment.nodeVersion } };
  const evidencePath = path.join(state.root, OUTPUT, 'cases', `${context.key}.json`); writeExclusive(evidencePath, evidence);
  state.records.push({ key: context.key, evidencePath, evidenceSha256: sha(read(state.root, evidencePath)), reportPath: file(state.root, reportPath, outputRoot), reportSha256: evidence.reportSha256 });
  state.active = null; replaceState(state.statePath, state);
  return { evidence, evidencePath, statePath: state.statePath };
}
