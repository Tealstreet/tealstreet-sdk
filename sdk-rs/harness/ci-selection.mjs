import { appendFileSync, readFileSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { GATE_SUITES, replayMatrix } from './ci-gate-suites.mjs';
import { venueFamilies, venueFamily } from '../hosts/wasm/js/venues.mjs';
import { nativeMatrix } from './native-ci-checks.mjs';
import { createGithubReader } from './cpu-cohort.mjs';
import { selectDispatchedCheckout } from './ci-checkout.mjs';

const git = (...args) => execFileSync('git', args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 }).trim();
const family = (venue) => venueFamily(venue);
const crateFamily = (crate) => crate === 'kucoin' ? ['kucoin', 'kucoinspot'] : [family(crate)];
const venueCrate = venue => family(venue) === 'kucoinspot' ? 'kucoin' : family(venue);
const suiteFamilies = new Map(GATE_SUITES.map(suite => [suite, family(suite)]));

// Include the base graph too: deleting an edge must not hide its former users.
export function reverseVenueDependencies(manifests) {
  const result = new Map();
  for (const [crate, text] of manifests) {
    crateFamily(crate);
    for (const match of text.matchAll(/\bsafe-cex-venue-([a-z]+)\b/g)) {
      if (match[1] === crate) continue;
      crateFamily(match[1]);
      const dependents = result.get(match[1]) ?? new Set();
      dependents.add(crate); result.set(match[1], dependents);
    }
  }
  return result;
}

export function selectCi({ paths, manifests, full = false }) {
  if (!Array.isArray(paths) || paths.some(path => typeof path !== 'string' || !path || path.startsWith('/') || path.split('/').includes('..')))
    throw new Error('Invalid changed paths');
  const crates = new Set(), reasons = [], tests = new Set();
  let nativeFull = full, consumerTests = false;
  for (const path of paths) {
    if (path.startsWith('docs/') || /\.(?:md|rst|adoc)$/.test(path)) continue;
    const venue = /^sdk-rs\/venues\/([^/]+)\/(.*)$/.exec(path);
    if (venue) {
      try { crateFamily(venue[1]); } catch { full = true; reasons.push(path); continue; }
      tests.add(venue[1]);
      if (venue[2].startsWith('tests/') || venue[2].startsWith('benches/')) continue;
      crates.add(venue[1]); continue;
    }
    const ts = /^packages\/safe-cex\/src\/exchanges\/([^/]+)\//.exec(path);
    if (ts) {
      try { family(ts[1]); } catch { full = true; reasons.push(path); continue; }
      if (!/\.(?:test|spec)\.[cm]?[jt]sx?$/.test(path) && !path.includes('/__tests__/')) crates.add(venueCrate(ts[1]));
      continue;
    }
    if (/^sdk-rs\/(?:engine|crypto|registry|hosts\/(?:native|wasm))\/tests\//.test(path)) { nativeFull = true; continue; }
    if (/^sdk-rs\/hosts\/wasm\/js\/.*\.test\.mjs$/.test(path) ||
        /^sdk-rs\/harness\/.*\.integration\.[cm]?[jt]s$/.test(path)) { consumerTests = true; continue; }
    if (/^\.github\/tests\//.test(path) ||
        /\.(?:test|integration)\.[cm]?[jt]s$/.test(path)) continue;
    full = true; reasons.push(path);
  }
  try {
    const dependencies = reverseVenueDependencies(manifests);
    for (const crate of crates) for (const dependent of dependencies.get(crate) ?? []) crates.add(dependent);
  } catch (error) { full = true; reasons.push(`dependency graph: ${error.message}`); }
  const families = full ? venueFamilies : [...new Set([...crates].flatMap(crateFamily))].sort();
  const suites = GATE_SUITES.filter(suite => families.includes(suiteFamilies.get(suite)));
  const nativeCrates = [...new Set([...crates, ...tests])].sort();
  return { full, nativeFull: nativeFull || full, combinedRequired: full || consumerTests,
    native: nativeMatrix(nativeFull || full, nativeCrates),
    reasons, families, replay: replayMatrix(suites, family), nativeCrates,
    replayRequired: suites.length > 0, sizeRequired: families.length > 0 };
}

export function checkoutBase({ event, checkout, parents, tree, mergeTree }) {
  if (event.pull_request?.base?.ref !== 'feat/rust-rewrite' ||
      event.pull_request.base.repo?.full_name !== 'Tealstreet/tealstreet-sdk' ||
      !/^[a-f0-9]{40}$/.test(checkout) || !/^[a-f0-9]{40}$/.test(tree) || parents.length !== 2 ||
      parents.some(parent => !/^[a-f0-9]{40}$/.test(parent)) ||
      parents[1] !== event.pull_request.head.sha || tree !== mergeTree)
    throw new Error('PR selection requires the exact two-parent Rust checkout composition');
  return parents[0];
}

export function ciOutputs(result) {
  return Object.entries(result).map(([name, value]) => `${name}=${typeof value === 'string' ? value : JSON.stringify(value)}\n`).join('');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const isPr = process.env.GITHUB_EVENT_NAME === 'pull_request';
  if (process.env.GITHUB_EVENT_NAME === 'workflow_dispatch') await selectDispatchedCheckout({ git,
    runId: Number(process.env.GITHUB_RUN_ID), reader: createGithubReader({
      repository: process.env.GITHUB_REPOSITORY, token: process.env.GITHUB_TOKEN }) });
  let base;
  if (isPr) {
    const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
    const checkout = git('rev-parse', 'HEAD'), parents = git('show', '-s', '--format=%P', checkout).split(' ');
    base = checkoutBase({ event, checkout, parents, tree: git('rev-parse', 'HEAD^{tree}'),
      mergeTree: parents.length === 2 ? git('merge-tree', '--write-tree', ...parents) : '' });
    if (checkout !== process.env.GITHUB_SHA) throw new Error('Checkout differs from workflow composition');
  }
  const manifests = [];
  for (const { name: crate } of readdirSync('sdk-rs/venues', { withFileTypes: true }).filter(entry => entry.isDirectory())) {
    manifests.push([crate, readFileSync(`sdk-rs/venues/${crate}/Cargo.toml`, 'utf8')]);
    if (base) {
      const path = `sdk-rs/venues/${crate}/Cargo.toml`;
      if (git('ls-tree', base, '--', path)) manifests.push([crate, git('show', `${base}:${path}`)]);
    }
  }
  const paths = base ? execFileSync('git', ['diff', '--no-renames', '--name-only', '-z', base, 'HEAD'], { encoding: 'utf8' }).split('\0').filter(Boolean) : [];
  const result = selectCi({ paths, manifests, full: !isPr });
  result.checkoutSha = git('rev-parse', 'HEAD');
  console.log(JSON.stringify({ base: base ?? null, ...result }, null, 2));
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, ciOutputs(result));
}
