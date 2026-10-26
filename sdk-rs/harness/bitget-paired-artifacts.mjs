import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { basename, isAbsolute, relative, resolve, sep } from 'node:path';
import { parseCohortJson } from './cpu-cohort.mjs';
import { BITGET_COHORT_ORDER, BITGET_PAIRED_FREEZE_SHA256, BITGET_PAIRED_PROTOCOL_SHA256 } from './bitget-paired-steady.mjs';

const sha = value => createHash('sha256').update(value).digest('hex');
const insist = (ok, message) => { if (!ok) throw new Error(`Bitget paired artifacts: ${message}`); };

/** Read independently retained packages and every original raw replay slot.
 * Producer authenticity remains the separate same-run API/log check. */
export function readBitgetPairedEvidence(directory) {
  const root = realpathSync(directory);
  const read = name => {
    const path = resolve(root, name), suffix = relative(root, path);
    insist(suffix && suffix !== '..' && !suffix.startsWith(`..${sep}`) && !isAbsolute(suffix), 'evidence path escape');
    let current = root;
    for (const part of suffix.split(sep)) {
      current = resolve(current, part);
      insist(!lstatSync(current).isSymbolicLink(), 'symlink in evidence path');
    }
    const stat = lstatSync(path);
    insist(stat.isFile() && stat.nlink === 1 && realpathSync(path) === path, 'evidence is not an independent regular file');
    return readFileSync(path);
  };
  const json = name => parseCohortJson(read(name));
  const jsonText = name => { const bytes = read(name); parseCohortJson(bytes); return bytes.toString('utf8'); };
  const plan = json('plan.json'), comparison = json('comparison.json');
  insist(!['T', 'tolerance', 'threshold', 'R0', 'r0', 'baselineOverride', 'anchorOverride', 'A0'].some(key => Object.hasOwn(plan, key)),
    'PR-supplied acceptance authority in measurement plan');
  insist(plan.protocolSha256 === BITGET_PAIRED_PROTOCOL_SHA256 && plan.freezeSha256 === BITGET_PAIRED_FREEZE_SHA256,
    'unapproved measurement protocol or calibration freeze');
  const archives = Object.fromEntries(['baseline', 'candidate'].map(arm => {
    const packagePath = `${arm}/node`, metadataBytes = jsonText(`${packagePath}/build.json`);
    const names = readdirSync(resolve(root, packagePath)).sort();
    insist(JSON.stringify(names) === JSON.stringify(['build.json', 'package.json', 'safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm']),
      'unexpected independent archive inventory');
    const shipped = existsSync(resolve(root, `${arm}/shipped`)) ? Object.fromEntries(['node', 'web'].map(target => {
      const path = `${arm}/shipped/${target}`, actual = readdirSync(resolve(root, path)).sort();
      const expected = ['safe_cex_wasm.d.ts', 'safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'safe_cex_wasm_bg.wasm.d.ts',
        ...(target === 'node' ? ['package.json'] : [])].sort();
      insist(JSON.stringify(actual) === JSON.stringify(expected), 'incomplete shipped node/web inventory');
      return [target, Object.fromEntries(actual.map(name => [name, sha(read(`${path}/${name}`))]))];
    })) : undefined;
    return [arm, { source: plan.refs[arm], metadataBytes, metadata: parseCohortJson(metadataBytes),
      ...(shipped ? { shipped } : {}),
      files: Object.fromEntries(names.map(name => [name, sha(read(`${packagePath}/${name}`))])) }];
  }));
  insist(comparison.cohorts?.length === 8, 'missing complete cohort receipt');
  const cohorts = BITGET_COHORT_ORDER.map((arm, index) => {
    const prefix = `${String(index + 1).padStart(2, '0')}-${arm}`;
    const reportBytes = jsonText(`${prefix}/gate.json`), report = parseCohortJson(reportBytes);
    insist(Array.isArray(report.artifacts) && report.artifacts.length === 20, 'incomplete replay inventory');
    const names = new Set();
    const raw = report.artifacts.map(({ impl, mode, run, path }) => {
      insist(typeof path === 'string' && path && !path.includes('\\') && !path.includes('\0'), 'invalid raw path');
      const name = basename(path);
      insist(name && name !== '.' && name !== '..' && !names.has(name), 'duplicate or invalid raw artifact');
      names.add(name);
      const bytes = jsonText(`${prefix}/${name}`);
      return { impl, mode, run, bytes, sha256: sha(bytes) };
    });
    const receipt = comparison.cohorts[index];
    insist(receipt.number === index + 1 && receipt.arm === arm, 'substituted cohort receipt');
    return { before: json(`${prefix}/before.json`), finishedAt: receipt.finishedAt,
      evidence: { report, reportBytes, raw } };
  });
  return { schema: 1, freezeSha256: plan.freezeSha256, protocolSha256: plan.protocolSha256,
    runId: String(plan.runId), attempt: plan.runAttempt, jobId: String(plan.jobId), runnerId: String(plan.runnerId),
    refs: plan.refs, candidateTree: plan.candidateTree, order: plan.order, archives, cohorts,
    measurementSource: plan.measurementSource, measurementTree: plan.measurementTree, measurementFiles: plan.measurementFiles,
    cpuModel: plan.runner.cpuModel, imageOS: plan.runner.imageOS, imageVersion: plan.runner.imageVersion,
    nodeSha256: plan.nodeSha256 };
}
