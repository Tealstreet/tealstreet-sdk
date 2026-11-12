import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { selectCpuComparison } from './bitget-paired-ci.mjs';
import { parseCohortJson } from './cpu-cohort.mjs';
import { validatePublicEvidence } from './cpu-remeasure.mjs';
import { requireCpuRemeasureSuite } from './cpu-rollout.mjs';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';
import { CPU_FIELDS, performanceObjectives, performanceFailures } from './performance-policy.mjs';

const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const same = isDeepStrictEqual;
const insist = (ok, message) => { if (!ok) throw new Error(`CPU family identity: ${message}`); };
const issued = new WeakSet(), certificates = new WeakSet();
const files = ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js', 'package.json'];

export async function familyComparisonAuthority(options) {
  const suite = requireCpuRemeasureSuite(options.suite);
  const { run, pull, landing, job, checkoutSha, checkoutTree, mergeTree } = await selectCpuComparison({ ...options,
    jobName: `Rust / replay measurement (${suite})` });
  insist(run.repository?.full_name === 'Tealstreet/tealstreet-sdk' && run.run_attempt === 1 &&
    job.run_id === run.id && job.run_attempt === 1 && job.head_sha === run.head_sha &&
    Number.isSafeInteger(job.id) && job.id > 0 && Number.isSafeInteger(job.runner_id) && job.runner_id > 0 &&
    checkoutTree === mergeTree && /^[a-f0-9]{40}$/.test(checkoutSha), 'invalid original source/producer');
  const baseline = landing ? landing.parents?.[0]?.sha : pull?.base?.sha;
  insist(/^[a-f0-9]{40}$/.test(baseline ?? '') && /^[a-f0-9]{40}$/.test(checkoutTree) &&
    (landing ? landing.sha === run.head_sha && checkoutSha === landing.sha && landing.tree?.sha === checkoutTree
      : pull.head.sha === run.head_sha), 'baseline/candidate differs from authenticated composition');
  const value = Object.freeze({ schema: 1, suite, family: venueFamily(suite), runId: String(run.id), attempt: 1,
    jobId: String(job.id), runnerId: String(job.runner_id), candidate: checkoutSha, candidateTree: checkoutTree,
    baseline });
  issued.add(value);
  return value;
}

export async function collectFamilyIdentity(context, suite) {
  const authority = await familyComparisonAuthority({ ...context, suite });
  const temporary = resolve(context.env.RUNNER_TEMP, `cpu-family-${suite}`);
  mkdirSync(temporary, { recursive: false });
  const script = resolve(temporary, 'collect-cpu-family.py'), authorityPath = resolve(temporary, 'authority.json');
  copyFileSync(resolve(context.root, 'sdk-rs/harness/collect-cpu-family.py'), script);
  writeFileSync(authorityPath, JSON.stringify(authority));
  const child = spawnSync('python3', [script, '--authority', authorityPath, '--temporary', temporary], {
    cwd: context.root, env: context.env, stdio: 'inherit' });
  insist(child.status === 0 && !child.error && !child.signal, 'independent base build/sealing failed; no byte-identity pass');
  const current = await familyComparisonAuthority({ ...context, suite });
  insist(same(current, authority), 'source/base/producer changed during build');
  return authority;
}

/** Input entries are from the digest-checked original producer ZIP. */
export function readFamilyIdentityDirectory({ directory, ...options }) {
  const names = ['proof.json', ...['baseline', 'candidate'].flatMap(arm =>
    ['build.json', ...files].map(name => `${arm}/${name}`))];
  const entries = new Map(names.map(name => [`cpu-family/${name}`, readFileSync(resolve(directory, name))]));
  return readFamilyIdentity({ entries, ...options });
}

export function readFamilyIdentity({ entries, authority, state, runtimeManifest }) {
  insist(issued.has(authority), 'unissued source/producer authority');
  const read = name => {
    const value = entries.get(`cpu-family/${name}`);
    insist(Buffer.isBuffer(value), `missing sealed family member ${name}`);
    return value;
  };
  const proof = parseCohortJson(read('proof.json'));
  insist(same(proof, authority) && state.mode === 'family' && state.suite === authority.suite &&
    state.sourceHead === authority.candidate && String(state.run.id) === authority.runId &&
    String(state.job.id) === authority.jobId && String(state.job.runner_id) === authority.runnerId,
  'proof differs from original family source/run/job/runner');
  const archives = Object.fromEntries(['baseline', 'candidate'].map(arm => {
    const metadata = parseCohortJson(read(`${arm}/build.json`));
    insist(metadata.schema === 1 && metadata.sourceHead === authority[arm] && metadata.runId === authority.runId &&
      metadata.family === authority.family && venueFamily(metadata.venue) === authority.family &&
      metadata.target === 'nodejs' && metadata.profile === 'release', 'archive source/run/family differs');
    const hashes = Object.fromEntries(files.map(name => [name, sha(read(`${arm}/${name}`))]));
    insist(same(metadata.hashes, hashes), 'metadata differs from actual WASM/glue/package seals');
    return [arm, { metadata, hashes }];
  }));
  insist(same(archives.baseline.metadata.tools, archives.candidate.metadata.tools) &&
    same(archives.baseline.metadata.buildFlags, archives.candidate.metadata.buildFlags), 'build tools/flags changed');
  for (const [name, digest] of Object.entries(archives.candidate.hashes)) {
    const matches = runtimeManifest.entries.filter(entry => entry.path === `sdk-rs/hosts/wasm/pkg/node/${name}` && entry.type === 'file');
    insist(matches.length === 1 && matches[0].sha256 === digest, 'candidate differs from original family runtime');
  }
  if (!files.every(name => archives.baseline.hashes[name] === archives.candidate.hashes[name])) return undefined;
  const certificate = Object.freeze({ disposition: 'unchanged bytes, not measured', measured: false,
    authority, moduleHashes: Object.freeze(archives.candidate.hashes) });
  certificates.add(certificate);
  return certificate;
}

export function familyCpuAssessment(assessment, report, certificate, source) {
  if (!certificate) return assessment;
  insist(certificates.has(certificate) && report.private === false &&
    source === certificate.authority.candidate && venueFamily(report.venue) === certificate.authority.family,
  'unissued or mismatched family byte proof');
  insist(report.ciWasm === undefined || report.ciWasm?.sourceHead === source &&
    same(report.ciWasm.hashes, certificate.moduleHashes), 'report metadata differs from issued family byte proof');
  const cpu = performanceObjectives(report.samples.ts, report.samples.rust).filter(value => CPU_FIELDS.includes(value.field));
  const failures = performanceFailures(report.samples.ts, report.samples.rust, report.venue).filter(failure =>
    cpu.some(value => failure.startsWith(`performance ${value.label}:`)));
  return { ...assessment, errors: assessment.errors.filter(error =>
    !failures.some(failure => error === `${report.venue}: unexpected ${failure}`)) };
}

/** A build probe only; this cannot certify or waive a CPU result. */
export function familyCpuMiss(evidence, assessment) {
  if (!assessment.errors.length || validatePublicEvidence(evidence).length) return false;
  const report = evidence.report;
  const cpu = performanceObjectives(report.samples.ts, report.samples.rust).filter(value => CPU_FIELDS.includes(value.field));
  const failures = performanceFailures(report.samples.ts, report.samples.rust, report.venue).filter(failure =>
    cpu.some(value => failure.startsWith(`performance ${value.label}:`)));
  return assessment.errors.every(error => failures.some(failure => error === `${report.venue}: unexpected ${failure}`));
}
