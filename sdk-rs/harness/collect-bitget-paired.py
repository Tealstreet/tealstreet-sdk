"""Collect the fixed public Bitget ABBA/BAAB supplement; no certification here."""
import argparse
import hashlib
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys


def insist(ok, message):
    if not ok:
        raise RuntimeError(message)


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, data):
    Path(path).write_text(json.dumps(data, indent=2) + '\n')


def logged(command, log, allowed=(0,)):
    with Path(log).open('w') as stream:
        result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT)
    insist(result.returncode in allowed,
           f'command failed ({result.returncode}): {command}; see {log}')
    return result.returncode


parser = argparse.ArgumentParser()
parser.add_argument('--baseline', required=True)
parser.add_argument('--candidate', required=True)
parser.add_argument('--out', required=True)
parser.add_argument('--check-only', action='store_true')
parser.add_argument('--authority', required=True)
parser.add_argument('--module-path', required=True)
args = parser.parse_args()
root = Path.cwd()
refs = {'baseline': args.baseline, 'candidate': args.candidate}
for arm, ref in refs.items():
    insist(len(ref) == 40 and all(c in '0123456789abcdef' for c in ref), f'invalid {arm} ref')
    insist(output('git', 'rev-parse', ref) == ref, f'missing exact {arm} source')
authority = json.loads(Path(args.authority).read_text())
insist(authority['witness'] in ('tip', 'anchor'), 'unknown comparison witness')
insist(authority['baseline'] == args.baseline and authority['candidate'] == args.candidate,
       'collector source differs from premeasurement API authority')
insist(authority['attempt'] == 1 and authority['candidateTree'] == output('git', 'rev-parse', args.candidate + '^{tree}'),
       'collector composition/attempt differs')
insist(output('git', 'rev-parse', 'HEAD') == args.candidate, 'collector requires the actual tested checkout')
plan = {'refs': refs, 'witness': authority['witness'],
        'measurementSource': args.candidate, 'measurementTree': authority['candidateTree'],
        'measurementFiles': authority['measurementFiles'],
        'order': ['baseline', 'candidate', 'candidate', 'baseline',
                  'candidate', 'baseline', 'baseline', 'candidate'],
        'blockOrders': ['ABBA', 'BAAB'], 'pairsPerCohort': 9,
        'venue': 'bitget', 'tape': 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz',
        'books': 'BTCUSDT,ETHUSDT', 'collectionOnly': True,
        'protocolSha256': authority['protocolSha256'],
        'freezeSha256': authority['freezeSha256'],
        'candidateTree': authority['candidateTree'],
        'jobId': authority['jobId'], 'runnerId': authority['runnerId'],
        'noGatePolicyOverride': True,
        'primaryStatistic': 'ratio of pooled Rust medians to pooled TS medians by label',
        'reportAlso': ['all cohort ratios', 'block contrasts', 'like-position contrasts'],
        'acceptanceRule': 'separate authenticated verifier required; collection grants no authority',
        'baselineAuthority': authority['witness'], 'notR0Initialization': True,
        'canonicalModulePath': 'one fixed active-module directory for every cohort',
        'noAdaptiveWaitsFiltersOrRetries': True}
for position in range(4):
    insist({plan['order'][position], plan['order'][position + 4]} == set(refs),
           'labels are not counterbalanced at every within-block position')
subprocess.run(['git', 'merge-base', '--is-ancestor', args.baseline,
                args.candidate], check=True)
if args.check_only:
    print(json.dumps(plan, indent=2))
    sys.exit(0)
insist(os.environ.get('GITHUB_ACTIONS') == 'true', 'measurement is CI-only')
insist(os.environ.get('GITHUB_EVENT_NAME') in ('pull_request', 'workflow_dispatch', 'push'), 'unsupported workflow event')
insist(os.environ.get('GITHUB_RUN_ATTEMPT') == '1', 'job reruns are forbidden')
insist(os.environ.get('GITHUB_RUN_ID') == authority['runId'], 'authority belongs to another run')
insist(not output('git', 'status', '--porcelain'), 'dirty initial checkout')

result_root = Path(args.out).resolve()
insist(not result_root.exists(), 'refuse overwriting earlier observations')
result_root.mkdir(parents=True)
plan.update({'measuredCheckout': output('git', 'rev-parse', 'HEAD'),
             'runId': os.environ['GITHUB_RUN_ID'],
             'runAttempt': int(os.environ['GITHUB_RUN_ATTEMPT']),
             'recordedAt': datetime.now(timezone.utc).isoformat(),
             'nodeSha256': sha(output('node', '-p', 'process.execPath')),
             'runner': {'cpuModel': next(line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')),
                        'uname': list(platform.uname()),
                        'cpuInfo': Path('/proc/cpuinfo').read_text(),
                        'lscpu': output('lscpu'),
                        'imageOS': os.environ.get('ImageOS'),
                        'imageVersion': os.environ.get('ImageVersion')},
             'scriptSha256': sha(__file__),
             'nodeVersion': output('node', '--version'),
             'nodeVersions': json.loads(output('node', '-p', 'JSON.stringify(process.versions)'))})
write_json(result_root / 'plan.json', plan)
tape_sha256 = sha(plan['tape'])
plan['tapeSha256'] = tape_sha256
plan['booksSha256'] = hashlib.sha256(plan['books'].encode()).hexdigest()
write_json(result_root / 'plan.json', plan)
source_seals = {arm: {p: output('git', 'rev-parse', f'{ref}:{p}')
                for p in ('sdk-rs/harness/build.sh', 'sdk-rs/harness/gate.mjs',
                          'sdk-rs/harness/performance-policy.mjs',
                          'sdk-rs/harness/replay.mjs', 'sdk-rs/harness/ci-wasm.mjs',
                          'sdk-rs/harness/wasm-budget.mjs', '.github/workflows/rust.yml')}
                for arm, ref in refs.items()}
plan['sourceSeals'] = source_seals
plan['independentPackagesMayDiffer'] = True
plan['sourceDiffSha256'] = hashlib.sha256(subprocess.check_output(['git', 'diff', args.baseline, args.candidate])).hexdigest()
(result_root / 'source-diff.patch').write_bytes(subprocess.check_output(['git', 'diff', args.baseline, args.candidate]))
write_json(result_root / 'plan.json', plan)
size_records = {}
for arm, ref in refs.items():
    subprocess.run(['git', 'checkout', '--detach', ref], check=True)
    insist(not output('git', 'status', '--porcelain'), f'dirty {arm} build inputs')
    phase = result_root / arm
    phase.mkdir()
    try:
        logged(['bash', 'sdk-rs/harness/build.sh', '--venue', 'bitget'], phase / 'build.log')
        logged(['node', 'sdk-rs/harness/wasm-budget.mjs', '--venue', 'bitget'], phase / 'budget.log')
        budget = json.loads(Path('sdk-rs/harness/out/wasm-budget-venue.json').read_text())
        write_json(phase / 'budget.json', budget)
        logged(['node', 'sdk-rs/harness/ci-wasm.mjs', '--venue', 'bitget',
                '--archive', str(phase / 'node')], phase / 'archive.log')
        shutil.copy2('sdk-rs/target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm',
                     phase / 'raw.wasm')
        shutil.copy2('sdk-rs/hosts/wasm/pkg/web/safe_cex_wasm_bg.wasm', phase / 'web.wasm')
        shipped = phase / 'node/safe_cex_wasm_bg.wasm'
        size_records[arm] = {'source': ref, 'budgetBytes': budget['budgetBytes'], 'nodeBytes': shipped.stat().st_size,
                             'webBytes': (phase / 'web.wasm').stat().st_size,
                             'nodeSha256': sha(shipped), 'webSha256': sha(phase / 'web.wasm'),
                             'rawSha256': sha(phase / 'raw.wasm')}
        insist(all(size_records[arm][k] <= budget['budgetBytes'] for k in ('nodeBytes', 'webBytes')),
               f'{arm} exceeds its checked-in family budget')
        write_json(phase / 'size.json', size_records[arm])
    finally:
        logged(['cargo', 'clean', '--manifest-path', 'sdk-rs/Cargo.toml'], phase / 'clean.log')
        insist(not Path('sdk-rs/target').exists(), f'{arm} target cleanup incomplete')
write_json(result_root / 'sizes.json', size_records)
archive_names = sorted(p.name for p in (result_root / 'baseline/node').iterdir())
insist(archive_names == sorted(p.name for p in (result_root / 'candidate/node').iterdir()),
       'independent archive entries differ')
plan['archiveMetadata'] = {arm: json.loads((result_root / arm / 'node/build.json').read_text())
                           for arm in refs}
write_json(result_root / 'plan.json', plan)
active_module = Path(args.module_path).resolve()
insist(active_module == result_root.parent.parent / 'active-module', 'unexpected canonical module path')
cohorts = []
semantics = {}
subprocess.run(['git', 'checkout', '--detach', args.candidate], check=True)
def verify_measurement_source():
    insist(output('git', 'rev-parse', 'HEAD') == args.candidate, 'measurement must use common candidate source')
    insist(output('git', 'rev-parse', 'HEAD^{tree}') == plan['measurementTree'], 'measurement tree changed')
    insist(sha(output('node', '-p', 'process.execPath')) == plan['nodeSha256'], 'measurement Node changed')
    for source_path, seal in plan['measurementFiles'].items():
        insist(output('git', 'hash-object', source_path) == seal, 'common measurement bytes changed: ' + source_path)
    insist(not output('git', 'status', '--porcelain'), 'dirty cohort source')
    insist(sha(plan['tape']) == tape_sha256, 'changed tape before cohort')
    for source_path, seal in source_seals['candidate'].items():
        insist(output('git', 'rev-parse', f'HEAD:{source_path}') == seal,
               f'changed source seal: {source_path}')

for number, arm in enumerate(plan['order'], 1):
    phase = result_root / f'{number:02d}-{arm}'
    phase.mkdir()
    verify_measurement_source()
    if active_module.exists():
        shutil.rmtree(active_module)
    shutil.copytree(result_root / arm / 'node', active_module)
    module_seals = {p.name: sha(p) for p in active_module.iterdir()}
    for name in archive_names:
        insist(module_seals[name] == sha(result_root / arm / 'node' / name),
               f'active module differs from selected archive: {name}')
    logged(['node', '--input-type=module', '-e',
            "import { validatePairedCiWasm } from './sdk-rs/harness/ci-wasm.mjs'; "
            "validatePairedCiWasm({directory: process.argv[1], venue: 'bitget', sourceHead: process.argv[2]});",
            str(active_module), refs[arm]], phase / 'module-validation.log')
    before = {'number': number, 'arm': arm, 'block': (number - 1) // 4 + 1,
              'position': (number - 1) % 4 + 1,
              'source': refs[arm], 'modulePath': str(active_module),
              'measurementSource': args.candidate, 'measurementTree': plan['measurementTree'],
              'measurementFiles': plan['measurementFiles'], 'nodeSha256': plan['nodeSha256'],
              'hashes': module_seals, 'startedAt': datetime.now(timezone.utc).isoformat()}
    before['measurementKey'] = hashlib.sha256(('bitget-paired:' + authority['witness'] + ':' + str(number)).encode()).hexdigest()
    write_json(phase / 'before.json', before)
    print('\nCPU_CASE_BEGIN ' + before['measurementKey'] + ' ' + before['startedAt'], flush=True)
    report_path = phase / 'gate.json'
    command = ['node', 'sdk-rs/harness/gate.mjs', '--venue', plan['venue'],
               '--tape', plan['tape'], '--books', plan['books'], '--runs', '9',
               '--ci-wasm', str(active_module), '--paired-source', refs[arm], '--report', str(report_path)]
    exit_code = logged(command, phase / 'gate.log', allowed=(0, 1))
    insist(report_path.exists(), f'cohort {number} produced no report')
    finished_at = datetime.now(timezone.utc).isoformat()
    print('\nCPU_CASE_END ' + before['measurementKey'] + ' ' + finished_at, flush=True)
    verify_measurement_source()
    report = json.loads(report_path.read_text())
    insist(report['ciWasm'] == json.loads((active_module / 'build.json').read_text()),
           'report did not use the exact selected module seal')
    for name, seal in module_seals.items():
        insist(sha(active_module / name) == seal, 'active module changed during cohort')
    insist(report['runs'] == 9 and not report['private'], 'public pair contract changed')
    insist(report['venue'] == plan['venue'] and report['books'] == plan['books'] and report['tapeSha256'] == tape_sha256, 'case fingerprint changed')
    insist(report['correctnessRuns'] == 1 and report['requiredImprovement'] == 0.1,
           'correctness/threshold contract changed')
    insist(report['failures'] == [] or all(x.startswith('performance ') for x in report['failures']),
           f'non-performance failure in {arm}: {report["failures"]}')
    insist((exit_code == 0) == (report['status'] == 'PASS'), 'gate status/exit mismatch')
    insist(all(x['total'] == 0 and not x['errors'] for x in report['fixers']),
           'unexpected public fixer activity')
    for artifact in report['artifacts']:
        source = Path(artifact['path'])
        insist(source.is_file(), f'missing replay output {source}')
        destination = phase / source.name
        shutil.copy2(source, destination)
        if artifact['mode'] == 'correctness':
            raw = json.loads(source.read_text())
            raw.pop('metrics')
            impl = artifact['impl']
            if impl in semantics:
                insist(raw == semantics[impl], f'changed {impl} output in {number}-{arm}')
            else:
                semantics[impl] = raw
    ts = report['summary']['ts']['steadyCpuMs']
    rust = report['summary']['rust']['steadyCpuMs']
    insist(all(math.isfinite(x) and x > 0 for x in (ts, rust)), 'invalid CPU summary')
    cohorts.append({'number': number, 'arm': arm, 'status': report['status'],
                    'block': before['block'], 'position': before['position'],
                    'startedAt': before['startedAt'], 'finishedAt': finished_at,
                    'failures': report['failures'], 'tsSteadyMs': ts, 'rustSteadyMs': rust,
                    'rustTsRatio': rust / ts, 'limitMs': 0.9 * ts, 'marginPercent': 100 * (1 - rust / (0.9 * ts)),
                    'summary': report['summary']})
    write_json(result_root / 'cohorts.json', cohorts)
reports = [json.loads((result_root / f'{c["number"]:02d}-{c["arm"]}/gate.json').read_text())
           for c in cohorts]
def pool(indices):
    medians = {}
    for impl in ('ts', 'rust'):
        samples = [s['steadyCpuMs'] for index in indices for s in reports[index]['samples'][impl]]
        insist(len(samples) == 9 * len(indices) and
               all(math.isfinite(x) and x > 0 for x in samples), 'invalid pooled samples')
        medians[impl] = statistics.median(samples)
    return {'cohorts': [index + 1 for index in indices], 'pairs': 9 * len(indices),
            'tsMedianMs': medians['ts'], 'rustMedianMs': medians['rust'],
            'rustTsRatio': medians['rust'] / medians['ts']}
pooled = {arm: pool([i for i, c in enumerate(cohorts) if c['arm'] == arm]) for arm in refs}
blocks = []
for block in (1, 2):
    arms = {arm: pool([i for i, c in enumerate(cohorts) if c['block'] == block and c['arm'] == arm])
            for arm in refs}
    blocks.append({'block': block, 'arms': arms,
                   'candidateBaselineRatio': arms['candidate']['rustTsRatio'] / arms['baseline']['rustTsRatio']})
positions = []
for position in range(1, 5):
    arms = {arm: pool([i for i, c in enumerate(cohorts) if c['position'] == position and c['arm'] == arm])
            for arm in refs}
    positions.append({'position': position, 'arms': arms,
                      'candidateBaselineRatio': arms['candidate']['rustTsRatio'] / arms['baseline']['rustTsRatio']})
result = {'collectionOnly': True,
          'gatePolicyChanged': False, 'noCertification': True, 'plan': plan,
          'sizes': size_records, 'cohorts': cohorts, 'blocks': blocks,
          'likePositionContrasts': positions, 'pooled': pooled,
          'pooledCandidateBaselineRatio': pooled['candidate']['rustTsRatio'] / pooled['baseline']['rustTsRatio'],
          'allCorrectnessOutputsExact': True,
          'productionGateFailuresRetained': [c['number'] for c in cohorts if c['status'] == 'FAIL']}
write_json(result_root / 'comparison.json', result)
lines = ['# Bitget public steady counterbalanced collection', '',
         'Collection only. Original reports retained; separate authenticated verifier decides the steady supplement.', '',
         '| Cohort | Block/position | Arm | Rust/TS | Rust median ms | TS median ms | Original gate |',
         '| --- | --- | --- | ---: | ---: | ---: | --- |']
for c in cohorts:
    lines.append(f'| {c["number"]} | {c["block"]}/{c["position"]} | {c["arm"]} | '
                 f'{c["rustTsRatio"]:.6f} | {c["rustSteadyMs"]:.3f} | {c["tsSteadyMs"]:.3f} | {c["status"]} |')
lines += ['', f'Pooled B/A: {result["pooledCandidateBaselineRatio"]:.6f}.',
          f'Block B/A: {[b["candidateBaselineRatio"] for b in blocks]}.',
          f'Like-position B/A: {[p["candidateBaselineRatio"] for p in positions]}.',
          'All original failures, raw samples and audits retained.']
(result_root / 'table.md').write_text('\n'.join(lines) + '\n')
print(json.dumps(result, indent=2))
