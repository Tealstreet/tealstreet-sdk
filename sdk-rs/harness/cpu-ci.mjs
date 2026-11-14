// Workflow orchestration: original reports remain strict; only the final job
// can certify an authenticated canonical cohort. There is no rerun API here.
import { spawnSync } from 'node:child_process';
import { PUBLIC_REPLAY_PAIRS } from './performance-policy.mjs';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual, parseArgs } from 'node:util';
import { expectedGateCases, gateCaseKey } from './ci-gate-suites.mjs';
import { createGithubReader, cohortKey, parseCohortJson, authenticateArtifactExecution, authenticateBitgetPairedFailureArtifact, authenticateMeasurementWindow, formatMeasurementMarker } from './cpu-cohort.mjs';
import { readArtifactZip } from './cpu-artifacts.mjs';
import { verifyCache } from './cpu-cache.mjs';
import { assessGateReport } from './known-failure-policy.mjs';
import { splitCpuErrors, validatePublicEvidence } from './cpu-remeasure.mjs';
import { verifyGateSuite } from './verify-gates.mjs';
import { revalidateRecordedEvidence } from './revalidate-gate.mjs';
import { cacheEntriesDigest, runtimeFingerprint } from './cpu-runtime.mjs';
import { loadGateLedger } from './verify-gates.mjs';
import { readBitgetPairedEvidence } from './bitget-paired-artifacts.mjs';
import { selectBitgetPair } from './bitget-paired-ci.mjs';
import { physicalCandidateRows, relistPhysicalJob } from './physical-job.mjs';
import { pairedSteadyAssessment, readBitgetSteadyCertificate, assessBitgetPairedSteady, bitgetAnchorAuthority, bitgetSteadyWarning, revalidateBitgetCpuRejection } from './bitget-paired-steady.mjs';
import { readBitgetOracleRefusal, isBitgetOracleRefusal } from './bitget-oracle-refusal.mjs';

import { collectFamilyIdentity, familyComparisonAuthority, readFamilyIdentity, readFamilyIdentityDirectory, familyCpuAssessment, familyCpuMiss } from './cpu-family-identity.mjs';

import { CPU_REMEASURE_SUITES, requireCpuRemeasureSuite } from './cpu-rollout.mjs';
const sha256 = (value) => createHash('sha256').update(value).digest('hex');
const insist = (ok, message) => { if (!ok) throw new Error(`CPU certification: ${message}`); };
const same = isDeepStrictEqual;
const json = (path) => parseCohortJson(readFileSync(path));
const write = (path, value) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, JSON.stringify(value, null, 2) + '\n'); };
// The collection's roster: every non-private case, including public tapes the
// workflow generates (Binance Options), not only tracked public fixtures.
export const publicCases = (suite) => expectedGateCases(suite).filter((item) => !item.tape.endsWith('-private.jsonl'));
const artifactRef = ({ meta }) => ({ artifactId: meta.id, name: meta.name, digest: meta.digest, bytes: meta.size_in_bytes });
const fullCase = (report) => Object.fromEntries(['venue', 'tape', 'tapeSha256', 'books', 'private', 'correctnessRuns', 'runs'].map((key) => [key, report[key]]));
const manifest = () => json(new URL('./known-gate-failures.json', import.meta.url));

export function classifyInitialCase(evidence, declarations, paired, family, refusal) {
  if (refusal !== undefined) {
    insist(isBitgetOracleRefusal(refusal) && evidence.report?.venue === 'bitget' && evidence.report.private === false &&
      evidence.execution?.measuredCheckoutSha === refusal.authority.candidate, 'unissued or mismatched Bitget oracle refusal');
    const errors = validatePublicEvidence(evidence);
    insist(!errors.length, errors.join('\n'));
    classifyInitialCase(evidence, declarations, paired, family);
    return { decision: 'not-certified' };
  }
  const assessment = pairedSteadyAssessment(assessGateReport(evidence.report, declarations), evidence.report, paired, evidence.execution?.measuredCheckoutSha);
  if (!assessment.errors.length) return { decision: 'first', assessment };
  if (family && familyCpuMiss(evidence, assessment)) {
    const identityAssessment = familyCpuAssessment(assessment, evidence.report, family, evidence.execution?.measuredCheckoutSha);
    insist(!identityAssessment.errors.length, identityAssessment.errors.join('\n'));
    return { decision: 'first', assessment: identityAssessment, unchanged: true };
  }
  // A CPU-only miss above its venue bound after the in-job re-measure refuses only this suite's CPU gate.
  const split = splitCpuErrors(evidence, assessment.errors, paired);
  insist(split.deferred.length && !split.kept.length, assessment.errors.join('\n'));
  return { decision: 'cpu-refused', assessment };
}

export function artifactEntriesToDirectory(entries, directory) {
  insist(entries instanceof Map && entries.size > 0, 'missing validated ZIP entries');
  mkdirSync(directory, { recursive: true });
  const root = realpathSync(directory);
  insist(resolve(directory) === root, 'artifact destination is not canonical');
  // readArtifactZip already rejects traversal, symlink and ancestor collisions.
  for (const [name, bytes] of entries) {
    const path = resolve(root, name), suffix = relative(root, path);
    insist(suffix && !suffix.startsWith(`..${sep}`) && suffix !== '..' && !suffix.startsWith(sep), 'artifact output escape');
    let ancestor = dirname(path);
    while (ancestor !== root) {
      if (existsSync(ancestor)) insist(realpathSync(ancestor) === ancestor, 'artifact symlink ancestor');
      ancestor = dirname(ancestor);
    }
    mkdirSync(dirname(path), { recursive: true });
    insist(!existsSync(path), 'artifact output already exists');
    writeFileSync(path, bytes, { flag: 'wx' });
  }
  return root;
}

function jsonMember(entries, name) {
  insist(entries.has(name), `missing artifact member ${name}`);
  return parseCohortJson(entries.get(name));
}

function makeContext(env = process.env) {
  insist(env.GITHUB_ACTIONS === 'true' && /^[1-9]\d*$/.test(env.GITHUB_RUN_ID ?? '') && env.GITHUB_RUN_ATTEMPT === '1', 'requires original Actions attempt1');
  insist(env.GITHUB_TOKEN && env.GITHUB_REPOSITORY, 'missing read-only Actions identity');
  const checkout = spawnSync('git', ['rev-parse', '--verify', 'HEAD'], { encoding: 'utf8' });
  insist(checkout.status === 0 && /^[a-f0-9]{40}\n?$/.test(checkout.stdout), 'missing local tested checkout');
  return { checkoutSha: checkout.stdout.trim(), root: realpathSync(process.cwd()), runId: Number(env.GITHUB_RUN_ID), reader: createGithubReader({ repository: env.GITHUB_REPOSITORY, token: env.GITHUB_TOKEN }), env };
}

export async function evidenceInventory(context, refresh = false) {
  if (!context.inventory || refresh) {
    context.inventory = Promise.all([context.reader.getRun(context.runId), context.reader.listJobs(context.runId),
      context.reader.listArtifacts(context.runId)]).then(([run, jobs, artifacts]) => ({ run, jobs, artifacts }));
  }
  return context.inventory;
}

async function artifact(context, runId, name) {
  insist(runId === context.runId, 'artifact from another workflow');
  const { run, artifacts } = await evidenceInventory(context), selected = artifacts.filter(item => item.name === name);
  insist(selected.length === 1, `missing/duplicate exact artifact ${name}`);
  const meta = selected[0];
  insist(meta.expired === false && meta.workflow_run?.id === runId && meta.workflow_run.head_sha === run.head_sha &&
    meta.workflow_run.head_branch === run.head_branch && /^sha256:[a-f0-9]{64}$/.test(meta.digest), 'artifact/source metadata differs');
  const bytes = await context.reader.getArtifactBytes(meta.id, 'artifact');
  insist(bytes.length === meta.size_in_bytes && `sha256:${sha256(bytes)}` === meta.digest, 'artifact ZIP bytes differ');
  return { meta, bytes, entries: readArtifactZip(bytes, { maxBytes: 4 * 1024 * 1024 * 1024 }) };
}

function restoreOriginalOutputs(context, entries) {
  const out = resolve(context.root, 'sdk-rs/harness/out');
  // This is a fresh verifier workspace; retained original ZIP bytes
  // remain untouched. Revalidation writes only separate supplemental files.
  rmSync(out, { recursive: true, force: true });
  artifactEntriesToDirectory(entries, out);
}

export function firstEvidence(entries, key, expectedCase) {
  const value = jsonMember(entries, `cpu-collection/cases/${key}.json`);
  insist(value?.report && value?.reportBytes && value?.raw && value?.execution && value?.claim, 'incomplete first evidence');
  insist(value.schema === 1 && value.key === key && value.claim.key === key && cohortKey(value.claim) === key &&
    same(fullCase(value.report), value.claim.case) && (!expectedCase || same(value.claim.case, expectedCase)), 'substituted first case/key');
  insist(typeof value.reportBytes === 'string' && value.reportSha256 === sha256(value.reportBytes) &&
    same(parseCohortJson(value.reportBytes), value.report), 'first strict report byte identity differs');
  return value;
}

function suiteEntries(entries) {
  const ledger = entries.get('ci/ledger.jsonl'); insist(ledger, 'missing original strict ledger');
  const pathKey = (path) => {
    const normalized = path.replaceAll('\\', '/'), marker = '/sdk-rs/harness/out/';
    const at = normalized.indexOf(marker);
    return at >= 0 ? normalized.slice(at + marker.length) : normalized.replace(/^sdk-rs\/harness\/out\//, '');
  };
  return ledger.toString('utf8').trim().split('\n').map((line) => {
    const row = parseCohortJson(line), reportBytes = entries.get(pathKey(row.reportPath));
    insist(reportBytes, 'ledger references missing strict report');
    const report = parseCohortJson(reportBytes);
    return { report, reportBytes: reportBytes.toString('utf8'), exitCode: row.exitCode,
      raw: report.private === false ? report.artifacts?.map(({ impl, mode, run, path }) => {
        const bytes = entries.get(pathKey(path)); insist(bytes, 'strict report references missing raw slot');
        // Strict parser refuses malformed UTF8 before a byte-hash projection.
        parseCohortJson(bytes);
        return { impl, mode, run, bytes: bytes.toString('utf8'), sha256: sha256(bytes) };
      }) : undefined };
  });
}

export function requireMeasuredProducer({ runId, checkoutSha, suite, evidence, producer }) {
  const execution = evidence?.execution, claim = evidence?.claim;
  insist(producer?.run_id === runId && producer.run_attempt === 1 && producer.name === `Rust / replay measurement (${suite})` &&
    execution?.runId === String(runId) && execution.jobId === String(producer.id) &&
    execution.runnerId === String(producer.runner_id) && execution.attempt === 1 &&
    execution.measuredCheckoutSha === checkoutSha && claim?.runId === runId && claim.suite === suite &&
    claim.measuredCheckoutSha === checkoutSha, 'measured execution differs from current authenticated producer');
  insist(claim.firstJobId === producer.id, 'first claim belongs to another physical producer');
}

async function authenticateCase(context, evidence, producer, suite) {
  requireMeasuredProducer({ runId: context.runId, checkoutSha: context.checkoutSha, suite, evidence, producer });
  const logBytes = await context.reader.getLog(producer.id);
  return authenticateMeasurementWindow({ execution: evidence.execution, key: evidence.key, job: producer, logBytes });
}

export async function currentPhysical(context, name) {
  const { run, selected } = await relistPhysicalJob(async refresh => context.inventory ? evidenceInventory(context, refresh) :
    { run: await context.reader.getRun(context.runId), jobs: await context.reader.listJobs(context.runId) }, name, { sleep: context.sleep });
  insist(run.run_attempt === 1 && selected.length === 1 && selected[0].run_attempt === 1 && selected[0].runner_id > 0,
    `missing unique original physical job; candidates ${physicalCandidateRows(selected)}`);
  return { run, job: selected[0] };
}

const PRODUCER_STEP_REFRESH_LIMIT = 20;
const PRODUCER_STEP_REFRESH_MS = 30_000;
const producerIdentity = job => Object.fromEntries(['id', 'run_id', 'run_attempt', 'runner_id',
  'name', 'head_sha', 'head_branch', 'status', 'conclusion', 'started_at'].map(key => [key, job[key]]));
const workflowIdentity = run => Object.fromEntries(['id', 'run_number', 'run_attempt', 'workflow_id',
  'path', 'head_sha', 'head_branch', 'event'].map(key => [key, run[key]]));

async function terminalProducer(context, jobName, stepName, bitgetRefusal = false) {
  let inventory = await evidenceInventory(context), originalJob, originalRun, physical;
  for (;;) {
    const { run, jobs } = inventory, selected = jobs.filter(job => job.name === jobName);
    insist(run.id === context.runId && run.run_attempt === 1 && selected.length === 1,
      'missing unique artifact physical producer in original workflow attempt1');
    const job = selected[0];
    insist(Number.isSafeInteger(job.id) && job.id > 0 && Number.isSafeInteger(job.runner_id) && job.runner_id > 0 &&
      job.run_id === run.id && job.run_attempt === 1 && job.head_sha === run.head_sha && job.head_branch === run.head_branch,
      'measurement not successful original physical attempt1');
    const { status, conclusion, ...observed } = producerIdentity(job);
    physical ??= observed;
    insist(same(observed, physical), 'producer physical identity changed during step metadata refresh');
    // The job record can trail its own terminal steps; only a settled record is judged.
    if (status !== 'completed' || conclusion == null) {
      context.producerStepRefreshes ??= 0;
      insist(context.producerStepRefreshes < PRODUCER_STEP_REFRESH_LIMIT,
        `producer job not terminal after ${PRODUCER_STEP_REFRESH_LIMIT} refreshes: run ${run.id}, job ${job.id} ${job.name} ` +
        `[${status}/${conclusion}]`);
      context.producerStepRefreshes++;
      await new Promise(resolve => setTimeout(resolve, PRODUCER_STEP_REFRESH_MS));
      inventory = await evidenceInventory(context, true);
      continue;
    }
    insist(job.conclusion === 'success' || (bitgetRefusal && jobName === 'Rust / Bitget counterbalanced steady CPU' &&
      job.conclusion === 'failure'), 'measurement not successful original physical attempt1');
    const identity = producerIdentity(job), workflow = workflowIdentity(run);
    originalJob ??= identity; originalRun ??= workflow;
    insist(same(identity, originalJob) && same(workflow, originalRun), 'producer physical identity changed during step metadata refresh');
    insist(Array.isArray(job.steps) && job.steps.every(step => step && typeof step.name === 'string'),
      'missing producer step inventory');
    const empty = job.steps.length === 0;
    const upload = job.steps.filter(step => step.name === stepName);
    insist(empty || upload.length === 1, `missing unique successful step ${stepName}`);
    if (!empty && upload[0].status === 'completed' && upload[0].conclusion != null)
      insist(upload[0].conclusion === 'success', `missing unique successful step ${stepName}`);
    const pending = job.steps.filter(step => step.status !== 'completed' || step.conclusion == null);
    if (!empty && !pending.length) {
      const bound = context.bindings?.find(binding => binding.job.id === job.id);
      insist(!bound || same(bound.job, job), 'physical producer changed after artifact binding');
      return { run, job };
    }
    context.producerStepRefreshes ??= 0;
    insist(context.producerStepRefreshes < PRODUCER_STEP_REFRESH_LIMIT,
      `${empty ? 'empty inventory' : 'producer steps not terminal'} after ${PRODUCER_STEP_REFRESH_LIMIT} refreshes: ` +
      `run ${run.id}, job ${job.id} ${job.name}; ` +
      (empty ? 'empty step inventory' : pending.map(step => `${step.name} [${step.status}/${step.conclusion}]`).join(', ')));
    context.producerStepRefreshes++;
    await new Promise(resolve => setTimeout(resolve, PRODUCER_STEP_REFRESH_MS));
    inventory = await evidenceInventory(context, true);
  }
}

export async function authenticatedBundle(context, runId, name, jobName, stepName, expectedCheckoutSha) {
  context.bundles ??= new Map();
  const key = JSON.stringify([runId, name, jobName, stepName, expectedCheckoutSha]);
  if (context.bundles.has(key)) return context.bundles.get(key);
  const bundle = await artifact(context, runId, name);
  const { run, job } = await terminalProducer(context, jobName, stepName);
  const currentArtifacts = (await evidenceInventory(context)).artifacts.filter(meta => meta.name === name);
  insist(currentArtifacts.length === 1 && same(currentArtifacts[0], bundle.meta), 'artifact identity changed during step metadata refresh');
  const logBytes = await context.reader.getLog(job.id);
  const receipt = authenticateArtifactExecution({ artifact: bundle.meta, artifactBytes: bundle.bytes,
    run, job, logBytes, stepName, jobName, expectedCheckoutSha });
  context.bindings?.push({ artifact: bundle.meta, job });
  const result = { ...bundle, receipt, job, logBytes };
  context.bundles.set(key, result);
  return result;
}

async function loadBitgetPaired(context) {
  context.bitgetPaired ??= (async () => {
    const bundle = await authenticatedBitgetPairedBundle(context);
    const directory = resolve(context.env.RUNNER_TEMP, 'authenticated-bitget-paired');
    artifactEntriesToDirectory(bundle.entries, directory);
    const authority = await selectBitgetPair({ reader: context.reader, runId: context.runId, root: context.root });
    if (bundle.job.conclusion === 'failure' && bundle.receipt.kind === 'oracle') {
      const result = jsonMember(bundle.entries, 'steady-assessment.json');
      insist(['tip', 'anchor'].includes(result.oracle?.witness), 'unknown refusal witness');
      const selected = result.oracle.witness === 'anchor' ? bitgetAnchorAuthority(authority) : authority;
      const sourceTrees = {};
      for (const source of new Set([selected.baseline, selected.candidate])) {
        const commit = await context.reader.getCommit(source);
        insist(commit.sha === source, 'oracle API source differs');
        sourceTrees[source] = commit.tree?.sha;
      }
      const refusal = readBitgetOracleRefusal({ entries: bundle.entries, authority, sourceTrees,
        tapeSha256: sha256(readFileSync(resolve(context.root, 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz'))) });
      if (refusal.witness === 'anchor') {
        const evidence = readBitgetPairedEvidence(resolve(directory, 'tip', 'records'));
        for (const cohort of evidence.cohorts) {
          restorePairedRawOutputs(context.root, cohort.evidence);
          revalidateRecordedEvidence(cohort.evidence, { root: context.root });
        }
        const tip = assessBitgetPairedSteady({ authority, evidence, manifest: manifest() });
        insist(tip.valid, tip.errors.join('\n'));
      }
      return { refusal, bundle };
    }
    const readOptions = { authority, manifest: manifest(),
      readEvidence: async (name, selected) => {
        const evidence = readBitgetPairedEvidence(resolve(directory, name, 'records'));
        for (const [index, cohort] of evidence.cohorts.entries()) {
          const key = sha256(`bitget-paired:${name}:${index + 1}`);
          insist(cohort.before.measurementKey === key, 'paired cohort measurement key differs');
          authenticateMeasurementWindow({ key, job: bundle.job, logBytes: bundle.logBytes, failureReceipt: bundle.receipt,
            execution: { runId: selected.runId, jobId: selected.jobId, runnerId: selected.runnerId,
              attempt: 1, measuredCheckoutSha: context.checkoutSha,
              startedAt: cohort.before.startedAt, completedAt: cohort.finishedAt } });
          restorePairedRawOutputs(context.root, cohort.evidence);
          revalidateRecordedEvidence(cohort.evidence, { root: context.root });
        }
        return evidence;
      } };
    if (bundle.job.conclusion === 'failure') {
      insist(bundle.receipt.kind === 'cpu', 'unknown paired failure');
      // Preserve a complete CPU rejection as the suite's verdict, never as an
      // oracle parsing error. Raw reports/markers are revalidated before this.
      throw await revalidateBitgetCpuRejection({ ...readOptions,
        message: bundle.receipt.message, record: jsonMember(bundle.entries, 'steady-assessment.json') });
    }
    const certificate = await readBitgetSteadyCertificate(readOptions);
    insist(certificate.steadyAccepted, 'tip or cumulative anchor steady regression');
    return { certificate, bundle };
  })();
  return context.bitgetPaired;
}

export async function authenticatedBitgetPairedBundle(context) {
  const jobName = 'Rust / Bitget counterbalanced steady CPU', stepName = 'Retain counterbalanced Bitget steady evidence';
  const { run, job } = await terminalProducer(context, jobName, stepName, true);
  if (job.conclusion === 'success') return authenticatedBundle(context, context.runId, 'bitget-paired-steady',
    jobName, stepName, context.checkoutSha);
  const bundle = await artifact(context, context.runId, 'bitget-paired-steady');
  const logBytes = await context.reader.getLog(job.id);
  const receipt = authenticateBitgetPairedFailureArtifact({ artifact: bundle.meta, artifactBytes: bundle.bytes,
    run, job, logBytes, expectedCheckoutSha: context.checkoutSha });
  context.bindings?.push({ artifact: bundle.meta, job });
  return { ...bundle, receipt, job, logBytes };
}

export function restorePairedRawOutputs(root, evidence) {
  const out = resolve(root, 'sdk-rs/harness/out'), entries = new Map();
  for (const [index, slot] of evidence.raw.entries()) {
    const target = resolve(root, evidence.report.artifacts[index].path), name = relative(out, target);
    insist(name && name !== '..' && !name.startsWith(`..${sep}`) && !name.startsWith(sep), 'paired raw path escapes original output root');
    insist(!entries.has(name), 'paired raw path collision');
    entries.set(name, Buffer.from(slot.bytes));
  }
  rmSync(out, { recursive: true, force: true });
  artifactEntriesToDirectory(entries, out);
}

export function pairedCertificateForRuntime(paired, state, entries) {
  if (!paired || state.mode !== 'family' || isBitgetOracleRefusal(paired.refusal)) return undefined;
  const recorded = jsonMember(entries, 'manifest.json');
  for (const [name, digest] of Object.entries(paired.certificate.moduleHashes)) {
    const path = `sdk-rs/hosts/wasm/pkg/node/${name}`;
    const matches = recorded.entries.filter(entry => entry.path === path && entry.type === 'file');
    insist(matches.length === 1 && matches[0].sha256 === digest, 'paired candidate differs from the original measured family module');
  }
  return paired.certificate;
}

async function prepareMeasurement(context, suite, destination, mode) {
  insist(context.env.GITHUB_RUN_ATTEMPT === '1', 'first measurement cannot reuse a retry cohort');
  requireCpuRemeasureSuite(suite);
  const { run, job } = await currentPhysical(context, `Rust / replay measurement (${suite})`);
  const { prepareCollection } = await import('./cpu-collection.mjs');
  const state = prepareCollection({ root: context.root, suite, destination, run, job, mode });
  insist(context.env.GITHUB_ENV && context.env.GITHUB_PATH && context.env.GITHUB_OUTPUT, 'missing Actions output channels');
  writeFileSync(context.env.GITHUB_ENV, `CPU_COLLECTION_STATE=${state.statePath}\n`, { flag: 'a' });
  writeFileSync(context.env.GITHUB_PATH, dirname(state.nodePath) + '\n', { flag: 'a' });
  writeFileSync(context.env.GITHUB_OUTPUT, `claim_path=${resolve(context.root, 'sdk-rs/harness/out/cpu-collection/cohort')}\nruntime_path=${state.destination}\n`, { flag: 'a' });
  return state;
}

async function collectMeasurement(context, suite) {
  requireCpuRemeasureSuite(suite);
  const entries = loadGateLedger({ root: context.root, includeRaw: true });
  let result = verifyGateSuite({ suite, entries, manifest: manifest(), mode: 'collect' });
  const state = json(resolve(context.root, 'sdk-rs/harness/out/cpu-collection/state.json'));
  insist(!state.active && state.records.length === publicCases(suite).length, 'missing completed public measurements');
  let unchanged = false;
  const probes = entries.map(entry => ({ entry, assessment: assessGateReport(entry.report, manifest()) }))
    .filter(({ entry, assessment }) => familyCpuMiss(entry, assessment));
  if (state.mode === 'family' && probes.length) {
    const cpuErrors = new Set(probes.flatMap(({ assessment }) => assessment.errors));
    const residual = result.errors.filter(error => !cpuErrors.has(error));
    insist(!residual.length, residual.join('\n'));
    const authority = await collectFamilyIdentity(context, suite);
    const runtimeManifest = json(resolve(context.root, 'sdk-rs/harness/out/cpu-collection/cohort/manifest.json'));
    const family = readFamilyIdentityDirectory({ directory: resolve(context.root, 'sdk-rs/harness/out/cpu-family'),
      authority, state, runtimeManifest });
    unchanged = Boolean(family);
    result = verifyGateSuite({ suite, entries, manifest: manifest(), mode: 'collect', family, sourceHead: state.sourceHead });
    if (unchanged) console.log(`${suite}: unchanged bytes, not measured`);
  }
  insist(!result.errors.length, result.errors.join('\n'));
  console.log(`${unchanged ? 0 : result.pending.length} CPU cases remain unresolved; this collection is not final certification`);
  for (const miss of result.cpuMisses) console.log(`${miss.venue} ${miss.tape}: CPU miss beyond remeasurement, judged by the CPU gate`);
  return result;
}

export function validateRetainedCache({ entries, evidence, state }) {
  const prefix = `cpu-collection/cache/${evidence.key}`;
  insist(evidence.cache?.prefix === prefix && evidence.cache.digest === evidence.execution.finalCacheSha256, 'cache receipt differs from exact measured case');
  const cache = verifyCache({ entries, prefix, expectedDigest: evidence.execution.finalCacheSha256 });
  insist(cache.manifestSha256 === evidence.cache.manifestSha256 && cache.manifest.workspaceRoot === state.root &&
    cache.manifest.cachePath === state.cachePath, 'cache bytes are not bound to actual runtime cache');
  return cache;
}

export function validateFirstBindings({ state, evidence, cohortEntries, actualManifest }) {
  const manifestBytes = cohortEntries.get('manifest.json');
  insist(manifestBytes, 'missing premeasurement runtime manifest');
  const recordedManifest = parseCohortJson(manifestBytes), fingerprint = runtimeFingerprint(recordedManifest);
  const bundle = jsonMember(cohortEntries, 'cohort.json');
  const claims = bundle.claims?.filter((claim) => claim.key === evidence.key);
  insist(bundle.schema === 1 && bundle.suite === state.suite && claims?.length === 1 &&
    same(claims[0], evidence.claim) && same(bundle.claims, state.claims), 'first state/evidence differs from authenticated premeasurement claims');
  insist(state.runtimeSha256 === fingerprint && evidence.claim.runtimeSha256 === fingerprint &&
    evidence.execution.runtimeSha256 === fingerprint && state.manifestSha256 === sha256(manifestBytes), 'first runtime digest is not premeasurement-bound');
  insist(state.root === recordedManifest.workspaceRoot && state.nodePath === resolve(state.root, '.cpu-runtime/node') &&
    same(state.environment, recordedManifest.environment) && state.cachePath === recordedManifest.cachePath &&
    evidence.execution.initialCacheSha256 === cacheEntriesDigest({ root: recordedManifest.workspaceRoot,
      cachePath: recordedManifest.cachePath, entries: recordedManifest.cacheEntries }), 'first Node/environment/cache differs from manifest');
  const claim = evidence.claim, execution = evidence.execution;
  insist(state.sourceHead === claim.measuredCheckoutSha && execution.measuredCheckoutSha === claim.measuredCheckoutSha &&
    state.inputsSha256 === claim.inputsSha256 && execution.inputsSha256 === claim.inputsSha256 &&
    state.policySha256 === claim.policySha256 && execution.policySha256 === claim.policySha256 &&
    state.run.id === claim.runId && state.run.run_number === claim.runNumber && state.job.id === claim.firstJobId &&
    execution.runId === String(state.run.id) && execution.jobId === String(state.job.id) && execution.runnerId === String(state.job.runner_id) &&
    execution.attempt === 1 && execution.nodeSha256 === recordedManifest.node.sha256 &&
    execution.imageOS === recordedManifest.environment.ImageOS && execution.imageVersion === recordedManifest.environment.ImageVersion &&
    execution.architecture === recordedManifest.environment.arch && execution.nodeVersion === recordedManifest.environment.nodeVersion,
  'first execution/source/runtime authority differs');
  insist(typeof evidence.reportBytes === 'string' && evidence.reportSha256 === sha256(evidence.reportBytes) &&
    same(parseCohortJson(evidence.reportBytes), evidence.report), 'first strict report byte identity differs');
  if (actualManifest) insist(same(actualManifest, recordedManifest), 'restored runtime manifest differs from first sealed manifest');
  return { schema: 1, runtimeSha256: fingerprint, manifestSha256: sha256(manifestBytes), nodeSha256: recordedManifest.node.sha256 };
}

export function retainApiEvidence(reader, destination) {
  const snapshot = reader.snapshots(), responses = [];
  for (const value of snapshot.responses) {
    const hash = sha256(value.raw);
    insist(hash === value.sha256, 'retained API bytes differ');
    const binaryArtifact = /^actions\/artifacts\/[1-9]\d*\/zip$/.test(value.path);
    const record = { path: value.path, status: value.status, finalStatus: value.finalStatus, sha256: hash, bytes: value.raw.length,
      retained: !binaryArtifact };
    // Original artifact ZIPs remain at their exact Actions IDs/digests. Retain
    // API responses and physical logs here without duplicating runtime ZIPs.
    if (!binaryArtifact) {
      const blob = resolve(destination, 'provenance/blobs', hash);
      mkdirSync(dirname(blob), { recursive: true });
      if (existsSync(blob)) insist(sha256(readFileSync(blob)) === hash, 'provenance blob collision');
      else writeFileSync(blob, value.raw, { flag: 'wx' });
      record.blob = `blobs/${hash}`;
    }
    responses.push(record);
  }
  const logs = snapshot.logs.map(({ jobId, raw, sha256: hash }) => {
    insist(sha256(raw) === hash && responses.some(value => value.sha256 === hash && value.retained), 'physical log missing from retained API bytes');
    return { jobId, sha256: hash, bytes: raw.length, blob: `blobs/${hash}` };
  });
  write(resolve(destination, 'provenance/inventory.json'), { schema: 1, responses, logs });
}


/** Every cohort has one original measurement, re-measured at most once in its own job.
 * The whole first suite (including private diagnostics) remains mandatory. */
export function validatePlannedCases(plan) {
  insist(plan?.schema === 1 && Number.isSafeInteger(plan.runId) && plan.runId > 0 &&
    /^[a-f0-9]{40}$/.test(plan.checkoutSha ?? '') && Array.isArray(plan.suites), 'invalid bounded same-run plan');
  insist(same(plan.suites.map(row => row.suite).sort(), [...CPU_REMEASURE_SUITES].sort()), 'missing/duplicate/substituted suite roster');
  for (const row of plan.suites) {
    requireCpuRemeasureSuite(row.suite);
    if (Object.hasOwn(row, 'refused')) {
      insist(same(Object.keys(row).sort(), ['refused', 'suite']) && typeof row.refused === 'string' && row.refused, 'invalid refused suite row');
      continue;
    }
    insist(Number.isSafeInteger(row.firstArtifactId) && /^sha256:[a-f0-9]{64}$/.test(row.firstArtifactDigest ?? '') &&
      Array.isArray(row.cases) && same(row.cases.map(value => gateCaseKey(value.case)).sort(), publicCases(row.suite).map(gateCaseKey).sort()), 'missing/duplicate/substituted complete public roster');
    insist(new Set(row.cases.map(value => value.key)).size === row.cases.length, 'duplicate case key');
    const refused = row.cases.some(value => value.decision === 'not-certified');
    insist(refused ? row.suite === 'bitget' && row.cases.every(value => value.decision === 'not-certified') &&
      row.oracleRefusal?.name === 'bitget-paired-steady' && Number.isSafeInteger(row.oracleRefusal.artifactId) && row.oracleRefusal.artifactId > 0 &&
      /^sha256:[a-f0-9]{64}$/.test(row.oracleRefusal.digest ?? '') && Number.isSafeInteger(row.oracleRefusal.bytes) && row.oracleRefusal.bytes > 0
      : row.oracleRefusal === undefined, 'invalid Bitget-only refusal plan authority');
    for (const value of row.cases) insist(['first', 'cpu-refused', 'not-certified'].includes(value.decision) && /^[a-f0-9]{64}$/.test(value.key ?? '') &&
      value.case.private === false && value.case.runs === PUBLIC_REPLAY_PAIRS && value.case.correctnessRuns === 1 && /^[a-f0-9]{64}$/.test(value.case.tapeSha256 ?? ''), 'invalid public disposition');
  }
  return plan;
}

async function familyIdentityForSuite(context, suite, strict, state, tiny, required = false) {
  if (state.mode !== 'family') return undefined;
  if (!strict.entries.has('cpu-family/proof.json')) {
    insist(!required, 'missing family comparison for unresolved CPU case');
    return undefined;
  }
  const authority = await familyComparisonAuthority({ ...context, suite });
  return readFamilyIdentity({ entries: strict.entries, authority, state, runtimeManifest: jsonMember(tiny.entries, 'manifest.json') });
}

async function gatherSuite(context, suite) {
  requireCpuRemeasureSuite(suite);
  const pairedBundle = suite === 'bitget' ? await loadBitgetPaired(context) : undefined;
  const strict = await authenticatedBundle(context, context.runId, `strict-replay-${suite}`,
    `Rust / replay measurement (${suite})`, 'Retain strict gate reports and artifact budgets', context.checkoutSha);
  restoreOriginalOutputs(context, strict.entries);
  const entries = suiteEntries(strict.entries);
  const needsComparison = entries.some(entry => familyCpuMiss(entry, assessGateReport(entry.report, manifest())));
  const tiny = await authenticatedBundle(context, context.runId, `cpu-cohort-${suite}`,
    `Rust / replay measurement (${suite})`, 'Retain CPU runtime capsule', context.checkoutSha);
  insist(tiny.job.id === strict.job.id, 'first uploads have different physical producers');
  const bundle = jsonMember(tiny.entries, 'cohort.json'), state = jsonMember(strict.entries, 'cpu-collection/state.json');
  const paired = pairedCertificateForRuntime(pairedBundle, state, tiny.entries);
  const family = await familyIdentityForSuite(context, suite, strict, state, tiny, needsComparison);
  const assessed = verifyGateSuite({ suite, entries, manifest: manifest(), mode: 'collect', family, sourceHead: state.sourceHead });
  insist(!assessed.errors.length, assessed.errors.join('\n'));
  insist(bundle.suite === suite && Array.isArray(bundle.claims) && bundle.claims.length === publicCases(suite).length, 'incomplete first claims');
  const { readSourceTree } = await import('./cpu-collection.mjs');
  insist(readSourceTree(context.root).inputsSha256 === state.inputsSha256 && state.sourceHead === context.checkoutSha, 'first measured inputs differ from current source');
  const row = { suite, firstArtifactId: strict.meta.id, firstArtifactDigest: strict.meta.digest, cases: [] };
  if (pairedBundle?.refusal) row.oracleRefusal = artifactRef(pairedBundle.bundle);
  for (const item of publicCases(suite)) {
    const matches = bundle.claims.filter(value => gateCaseKey(value.case) === gateCaseKey(item));
    insist(matches.length === 1, 'missing/duplicate exact first claim');
    const claim = matches[0], first = firstEvidence(strict.entries, claim.key, claim.case);
    validateFirstBindings({ state, evidence: first, cohortEntries: tiny.entries });
    validateRetainedCache({ entries: strict.entries, evidence: first, state });
    await authenticateCase(context, first, strict.job, suite);
    revalidateRecordedEvidence(first, { root: context.root });
    row.cases.push({ key: first.key, case: claim.case,
      decision: classifyInitialCase(first, manifest(), paired, family, pairedBundle?.refusal).decision });
  }
  return row;
}

async function planMeasurements(context, destination) {
  const suites = [];
  // One suite's missing or failed first evidence refuses only that suite (fail closed).
  for (const suite of CPU_REMEASURE_SUITES) {
    try { suites.push(await gatherSuite(context, suite)); }
    catch (error) { suites.push({ suite, refused: String(error?.message ?? error).slice(0, 2000) || 'first evidence refused' }); }
  }
  const plan = validatePlannedCases({ schema: 1, runId: context.runId, checkoutSha: context.checkoutSha, suites });
  write(resolve(destination, 'plan.json'), plan);
  return plan;
}

async function loadPlan(context) {
  const bundle = await authenticatedBundle(context, context.runId, 'cpu-plan',
    'Rust / CPU remeasurement plan', 'Retain CPU remeasurement plan', context.checkoutSha);
  const plan = validatePlannedCases(jsonMember(bundle.entries, 'plan.json'));
  insist(plan.runId === context.runId && plan.checkoutSha === context.checkoutSha, 'plan source/run/suite differs');
  return { plan, bundle };
}

async function certifySuite(context, suite, destination) {
  requireCpuRemeasureSuite(suite);
  const { plan, bundle: planBundle } = await loadPlan(context);
  const row = plan.suites.find(value => value.suite === suite);
  insist(row, 'suite missing from plan');
  if (row.refused) throw new Error(row.refused);
  const recomputed = await gatherSuite(context, suite);
  const pairedBundle = suite === 'bitget' ? await loadBitgetPaired(context) : undefined;
  insist(same(row, recomputed), 'authenticated plan differs from full current first-evidence revalidation');
  if (row.oracleRefusal) {
    insist(isBitgetOracleRefusal(pairedBundle?.refusal), 'missing authenticated Bitget refusal');
    write(resolve(destination, 'refusal.json'), { schema: 1, suite, runId: context.runId,
      checkoutSha: context.checkoutSha, disposition: 'NOT-CERTIFIED', certified: false,
      reason: pairedBundle.refusal.reason, pairedArtifact: row.oracleRefusal });
    const error = new Error(pairedBundle.refusal.reason);
    error.oracleRefusal = pairedBundle.refusal;
    throw error;
  }
  const strict = await authenticatedBundle(context, context.runId, `strict-replay-${suite}`,
    `Rust / replay measurement (${suite})`, 'Retain strict gate reports and artifact budgets', context.checkoutSha);
  const tiny = await authenticatedBundle(context, context.runId, `cpu-cohort-${suite}`,
    `Rust / replay measurement (${suite})`, 'Retain CPU runtime capsule', context.checkoutSha);
  const state = jsonMember(strict.entries, 'cpu-collection/state.json');
  const paired = pairedCertificateForRuntime(pairedBundle, state, tiny.entries);
  const family = await familyIdentityForSuite(context, suite, strict, state, tiny);
  const cases = [];
  for (const value of row.cases) {
    const first = firstEvidence(strict.entries, value.key, value.case);
    restoreOriginalOutputs(context, strict.entries);
    revalidateRecordedEvidence(first, { root: context.root });
    insist(value.decision !== 'cpu-refused', `${gateCaseKey(value.case)}: Rust slower than TS after the in-job re-measure; not certified`);
    const initial = classifyInitialCase(first, manifest(), paired, family);
    insist(initial.decision === 'first',
      'first result fails strict policy and authenticated byte/steady supplement');
    cases.push({ key: value.key, case: value.case, decision: { accepted: true, rule: first.report.status === 'PASS' ? 'strict-first-pass' : initial.unchanged ? 'unchanged bytes, not measured' : 'strict-declared-diagnostic', receipt: null } });
  }
  const certificate = { schema: 1, suite, runId: context.runId, checkoutSha: context.checkoutSha,
    plan: artifactRef(planBundle), first: artifactRef(strict), cases,
    ...(family ? { familyIdentity: family } : {}),
    ...(paired ? { pairedSteady: { artifact: artifactRef(pairedBundle.bundle), certificate: paired } } : {}) };
  write(resolve(destination, 'certificate.json'), certificate);
  const warning = bitgetSteadyWarning(paired);
  if (warning) console.log(`::warning title=CPU bitget::${warning}`);
  return certificate;
}

export async function evidenceEndCheck(context) {
  const { run, jobs, artifacts } = await evidenceInventory(context, true);
  insist(run.run_attempt === 1, 'workflow attempt changed during certification');
  for (const binding of context.bindings ?? []) {
    const inventory = artifacts.filter(value => value.name === binding.artifact.name);
    insist(inventory.length === 1 && same(inventory[0], binding.artifact), 'artifact identity became duplicate/changed');
    const producers = jobs.filter(job => job.name === binding.job.name);
    insist(producers.length === 1 && same(producers[0], binding.job), 'physical producer became duplicate/changed');
  }
}

/** Keep every suite's disposition; overall certification stays false if any is refused. */
export async function certifySuites(suites, certifyOne) {
  const certificates = {}, results = { certified: true, suites: {}, reasons: {} };
  for (const suite of suites) {
    try {
      certificates[suite] = await certifyOne(suite);
      results.suites[suite] = true;
    } catch (error) {
      results.certified = false;
      results.suites[suite] = false;
      results.reasons[suite] = String(error?.message ?? error).slice(0, 2000);
    }
  }
  return { results, certificates };
}

export async function certifyAll(context, destination,
  certifyOne = suite => certifySuite(context, suite, resolve(destination, suite))) {
  const { results: outcome, certificates } = await certifySuites(CPU_REMEASURE_SUITES, certifyOne);
  await evidenceEndCheck(context);
  const results = { schema: 1, runId: context.runId, checkoutSha: context.checkoutSha, ...outcome };
  write(resolve(destination, 'results.json'), results);
  if (context.env.GITHUB_OUTPUT) writeFileSync(context.env.GITHUB_OUTPUT, `results=${JSON.stringify(results)}\n`, { flag: 'a' });
  for (const [suite, reason] of Object.entries(outcome.reasons))
    console.log(`::error title=CPU certification ${suite}::${reason.replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A')}`);
  insist(outcome.certified, `not certified for suites: ${Object.keys(outcome.reasons).join(', ')}`);
  return certificates;
}

export function requireCertifiedSuite({ suite, results, result, runId, checkoutSha }) {
  requireCpuRemeasureSuite(suite);
  const suites = results?.suites, refused = suites ? Object.keys(suites).filter(name => suites[name] !== true).sort() : [];
  insist(result === 'success' && results?.schema === 1 && results.runId === runId && results.checkoutSha === checkoutSha &&
    suites && same(Object.keys(suites).sort(), [...CPU_REMEASURE_SUITES].sort()) &&
    Object.values(suites).every(value => typeof value === 'boolean') && results.certified === !refused.length &&
    results.reasons && same(Object.keys(results.reasons).sort(), refused) &&
    refused.every(name => typeof results.reasons[name] === 'string' && results.reasons[name]), 'missing/failed/incomplete shared CPU certification');
  if (!suites[suite]) throw new Error(results.reasons[suite]);
  return true;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { command: { type: 'string' }, suite: { type: 'string' }, out: { type: 'string' }, mode: { type: 'string', default: 'family' } } });
  if (!['plan', 'certify'].includes(values.command)) requireCpuRemeasureSuite(values.suite);
  if (values.command === 'status') {
    if (process.env.CPU_CERT_RESULT !== 'success') {
      console.error(`shared CPU certification did not complete (result=${process.env.CPU_CERT_RESULT || 'missing'}); certification not accepted`);
      process.exit(1);
    }
    const checkout = spawnSync('git', ['rev-parse', '--verify', 'HEAD'], { encoding: 'utf8' });
    insist(checkout.status === 0, 'missing wrapper checkout');
    requireCertifiedSuite({ suite: values.suite, results: parseCohortJson(process.env.CPU_CERT_RESULTS ?? 'null'),
      result: process.env.CPU_CERT_RESULT, runId: Number(process.env.GITHUB_RUN_ID), checkoutSha: checkout.stdout.trim() });
    console.log(`CPU certification accepted for ${values.suite}`);
    process.exit(0);
  }
  const context = { ...makeContext(), suite: values.suite, bindings: [] };
  try {
    if (values.command === 'prepare') await prepareMeasurement(context, values.suite, resolve(values.out), values.mode);
    else if (values.command === 'collect') await collectMeasurement(context, values.suite);
    else if (values.command === 'plan') { await planMeasurements(context, resolve(values.out)); await evidenceEndCheck(context); }
    else if (values.command === 'certify') await certifyAll(context, resolve(values.out));
    else throw new Error('unknown bounded CPU command');
  } finally {
    if (['plan', 'certify'].includes(values.command)) retainApiEvidence(context.reader, resolve(values.out));
  }
}
