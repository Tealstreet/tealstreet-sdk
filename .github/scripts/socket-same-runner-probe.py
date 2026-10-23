"""Diagnostic only: fixed CI WASM variants, one runner, no gate-policy changes."""
import argparse, datetime, hashlib, io, json, math, os, re
import shutil, statistics, subprocess, zipfile
from pathlib import Path

REPO = 'Tealstreet/tealstreet-sdk'
ORDERS = [('landing', 'before', 'after'), ('before', 'after', 'landing'),
          ('after', 'landing', 'before'), ('after', 'before', 'landing'),
          ('landing', 'after', 'before'), ('before', 'landing', 'after')]
REFERENCE = ['sdk-rs/harness', 'packages/safe-cex', 'package.json', 'yarn.lock',
             '.yarnrc', '.yarnrc.yml', '.yarn']
PACKAGE_FILES = {'build.json', 'safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'package.json'}
PERF_FAILURE = re.compile(r'^performance (main-thread CPU|steady-state CPU|process CPU|retained memory): Rust must be (at least 10% below TS|within max\(1 ms, 5%\) of a sub-100 ms TS objective) \(ts=[0-9.e+-]+, rust=[0-9.e+-]+\)$')

def need(ok, message):
    if not ok:
        raise ValueError(message)

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def unique(pairs):
    result = {}
    for key, value in pairs:
        need(key not in result, 'Duplicate JSON key')
        result[key] = value
    return result

def loads(raw):
    def bad(value):
        raise ValueError('Nonfinite JSON: ' + value)
    def finite(value):
        number = float(value)
        need(math.isfinite(number), 'Nonfinite JSON number: ' + value)
        return number
    return json.loads(raw, object_pairs_hook=unique, parse_constant=bad, parse_float=finite)

def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()

def run(args, cwd):
    return subprocess.check_output(args, cwd=cwd)

def git(root, *args):
    return run(['git', *args], root)

def api(root, path):
    return loads(run(['gh', 'api', 'repos/' + REPO + '/' + path], root))

def system(root):
    result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'env': {key: os.environ.get(key) for key in ['ImageOS', 'ImageVersion',
                      'RUNNER_NAME', 'RUNNER_OS', 'RUNNER_ARCH', 'GITHUB_RUN_ID',
                      'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB', 'GITHUB_SHA', 'GITHUB_REF']}}
    for name, command in [('node', ['node', '--version']), ('yarn', ['corepack', 'yarn', '--version']),
                          ('kernel', ['uname', '-a']), ('cpu', ['lscpu']),
                          ('processes', ['ps', '-eo', 'comm,pcpu'])]:
        result[name] = run(command, root).decode()
    for name in ['loadavg', 'stat', 'meminfo']:
        result[name] = Path('/proc', name).read_text()
    return result

def prepare(root, out, role, pin):
    need(pin['attempt'] == 1 and pin['family'] == 'bitunix', 'Only exact family attempt1 packages')
    latest = api(root, 'actions/runs/' + str(pin['run']))
    exact = api(root, f"actions/runs/{pin['run']}/attempts/1")
    for record, state in [(latest, pin['latestState']), (exact, pin['exactState'])]:
        need(state['status'] == 'completed' and
             {k: record[k] for k in state} == state, 'Workflow terminal state/attempt changed: ' + role)
        need(record['id'] == pin['run'] and
             record['head_sha'] == pin['head'] and record['head_branch'] == pin['branch'] and
             record['event'] == pin['event'] and record['path'] == '.github/workflows/rust.yml',
             'Workflow identity/attempt changed: ' + role)
    commits = {}
    for name, expected in pin['commits'].items():
        commit = api(root, 'git/commits/' + expected['sha'])
        actual = {'sha': commit['sha'], 'tree': commit['tree']['sha'],
                  'parents': [parent['sha'] for parent in commit['parents']]}
        need(actual == expected, 'Source commit tree/parents changed: ' + role)
        commits[name] = actual
    need(commits['head']['sha'] == pin['head'] and
         commits['checkout']['sha'] == pin['build']['sourceHead'] and
         commits['head']['tree'] == commits['checkout']['tree'], 'Checkout tree differs from measured head')
    inv = api(root, f"actions/runs/{pin['run']}/artifacts?per_page=100")
    need(inv['total_count'] == len(inv['artifacts']) <= 100, 'Incomplete artifact inventory')
    choices = [a for a in inv['artifacts'] if a['name'] == 'ci-wasm-bitunix']
    need(len(choices) == 1, 'Duplicate/missing package name')
    artifact = choices[0]
    need(artifact == pin['artifact'] and not artifact['expired'], 'Frozen package metadata changed')
    job = api(root, 'actions/jobs/' + str(pin['job']))
    need(job['id'] == pin['job'] and job['run_id'] == pin['run'] and job['run_attempt'] == 1 and
         job['head_sha'] == pin['head'] and job['head_branch'] == pin['branch'] and
         job['name'] == 'Rust / replay gate (bitunix)' and job['status'] == 'completed' and
         job['conclusion'] == pin['jobConclusion'], 'Physical job identity changed')
    log = run(['gh', 'api', '--allow-escape-sequences', f"repos/{REPO}/actions/jobs/{pin['job']}/logs"], root)
    need(sha(log) == pin['logSha256'], 'Physical job log differs')
    message = ('Artifact ci-wasm-bitunix has been successfully uploaded! Final size is ' +
               str(artifact['size_in_bytes']) + ' bytes. Artifact ID is ' + str(artifact['id']))
    lines = [line.removeprefix('\ufeff').split(' ', 1)[-1] for line in log.decode('utf-8-sig').splitlines()]
    need(lines.count(message) == 1, 'No exact package upload ID/name/size')
    need(('SHA256 digest of uploaded artifact is ' + artifact['digest'].removeprefix('sha256:')) in lines,
         'No exact upload digest')
    checkout = pin['build']['sourceHead']
    found = [lines[i + 1] for i, line in enumerate(lines[:-1])
             if re.search(r'(?:^|/)git log -1 --format=%H$', line.removeprefix('[command]'))]
    need(found == [checkout], 'Actual completed checkout differs from package source')
    steps = [step for step in job['steps'] if step['name'] == 'Retain the matching CI replay engine briefly']
    need(len(steps) == 1 and steps[0]['status'] == 'completed' and steps[0]['conclusion'] == 'success',
         'No successful physical package upload step')
    def instant(text):
        return datetime.datetime.fromisoformat(re.sub(r'(\.\d{6})\d+(?=Z|[+-])', r'\1', text).replace('Z', '+00:00'))
    start = instant(steps[0]['started_at'])
    end = instant(steps[0]['completed_at']) + datetime.timedelta(seconds=1)
    timed = [(index, instant(line.split(' ', 1)[0]), line.split(' ', 1)[1])
             for index, line in enumerate([v.removeprefix('\ufeff') for v in log.decode('utf-8-sig').splitlines()]) if ' ' in line]
    finalized = 'Artifact ci-wasm-bitunix successfully finalized. Artifact ID ' + str(artifact['id'])
    done = [v for v in timed if v[2] == finalized]
    uploaded = [v for v in timed if v[2] == message]
    need(len(done) == len(uploaded) == 1 and done[0][0] < uploaded[0][0], 'Ambiguous package upload')
    digests = [v for v in timed if v[0] < done[0][0] and re.fullmatch(r'SHA256 digest of uploaded artifact is [a-f0-9]{64}', v[2])]
    need(digests and digests[-1][2] == 'SHA256 digest of uploaded artifact is ' + artifact['digest'].removeprefix('sha256:'),
         'Digest does not bind exact finalization')
    need(not any(' successfully finalized. Artifact ID ' in v[2] for v in timed if digests[-1][0] < v[0] < done[0][0]),
         'Digest belongs to a different upload')
    need(all(start <= v[1] < end for v in [done[0], uploaded[0], digests[-1]]) and
         all(start <= instant(artifact[k]) < end for k in ['created_at', 'updated_at']), 'Wrong physical upload window')
    blob = run(['gh', 'api', f"repos/{REPO}/actions/artifacts/{artifact['id']}/zip"], root)
    need(len(blob) == artifact['size_in_bytes'] and 'sha256:' + sha(blob) == artifact['digest'],
         'Package ZIP hash/size mismatch')
    destination = out / 'packages' / role
    destination.mkdir(parents=True)
    with zipfile.ZipFile(io.BytesIO(blob)) as archive:
        need(set(archive.namelist()) == PACKAGE_FILES and len(archive.namelist()) == 4,
             'Unexpected package members')
        for name in PACKAGE_FILES:
            (destination / name).write_bytes(archive.read(name))
    build = loads((destination / 'build.json').read_bytes())
    need(build == pin['build'] and build['runId'] == str(pin['run']) and
         build['family'] == 'bitunix' and build['venue'] == 'bitunix' and
         build['profile'] == 'release' and build['target'] == 'nodejs', 'Package build contract differs')
    for name, digest in build['hashes'].items():
        need(name in PACKAGE_FILES - {'build.json'} and sha((destination / name).read_bytes()) == digest,
             'Package payload hash mismatch')
    need(set(build['hashes']) == PACKAGE_FILES - {'build.json'}, 'Incomplete payload hash map')
    (destination / 'original.zip').write_bytes(blob)
    (destination / 'physical.log').write_bytes(log)
    (destination / 'physical.json').write_bytes(encoded(job))
    (destination / 'inventory.json').write_bytes(encoded(inv))
    (destination / 'run-latest.json').write_bytes(encoded(latest))
    (destination / 'run-exact.json').write_bytes(encoded(exact))
    (destination / 'commits.json').write_bytes(encoded(commits))
    return destination

def collect_raw(root, destination, gate):
    expected = {(impl, mode, number) for impl in ['ts', 'rust']
                for mode, numbers in [('correctness', range(1)), ('performance', range(9))]
                for number in numbers}
    seen, paths = set(), set()
    samples = {'ts': [], 'rust': []}
    artifacts, correctness = [], {}
    for record in gate['artifacts']:
        slot = (record['impl'], record['mode'], record['run'])
        need(type(record['run']) is int and slot in expected and slot not in seen,
             'Duplicate/invalid raw sample slot')
        seen.add(slot)
        source = (root / record['path']).resolve()
        need(source.is_relative_to(root / 'sdk-rs/harness/out') and source.is_file() and source not in paths,
             'Unsafe/missing/duplicate raw path')
        paths.add(source)
        raw = source.read_bytes()
        name = f"{record['impl']}-{record['mode']}-{record['run']}.json"
        (destination / name).write_bytes(raw)
        decoded = loads(raw)
        need(decoded['impl'] == record['impl'] and
             decoded['metrics']['measuredMode'] == record['mode'], 'Raw sample identity/mode differs')
        artifacts.append({'record': record, 'retained': name, 'sha256': sha(raw)})
        if record['mode'] == 'performance':
            value = decoded['metrics']['steadyCpuMs']
            need(type(value) in (float, int) and math.isfinite(value) and value > 0, 'Invalid CPU sample')
            samples[record['impl']].append(value)
        else:
            decoded.pop('metrics')
            correctness[record['impl']] = sha(encoded(decoded))
    need(seen == expected and len(artifacts) == 20 and set(correctness) == {'ts', 'rust'},
         'Incomplete exact 9-pair raw roster')
    return artifacts, samples, correctness


def preserve_diagnostics(root, destination, previous):
    directory = root / 'sdk-rs/harness/out'
    if directory.is_dir():
        for path in directory.iterdir():
            if path.name not in previous and path.is_dir() and not path.is_symlink():
                shutil.copytree(path, destination / 'harness-diagnostics' / path.name)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--contract', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--root', type=Path, required=True)
    args = parser.parse_args()
    root, out = args.root.resolve(), args.out.resolve()
    need(os.environ.get('GITHUB_ACTIONS') == 'true', 'This protocol requires one isolated Actions runner')
    need(not out.exists(), 'Refuse to overwrite an experiment')
    contract_raw = args.contract.read_bytes()
    contract = loads(contract_raw)
    need(contract['protocol'] == 'socket-phaseF-same-runner-v1' and
         contract['orders'] == [list(x) for x in ORDERS] and contract['pairsPerGate'] == 9 and
         set(contract['cases']) == {'landing', 'before', 'after'}, 'Protocol/schedule changed')
    out.mkdir(parents=True)
    (out / 'contract.json').write_bytes(contract_raw)
    (out / 'helper.py').write_bytes(Path(__file__).read_bytes())
    initial = git(root, 'rev-parse', 'HEAD').decode().strip()
    (out / 'system-before.json').write_bytes(encoded(system(root)))
    packages = {role: prepare(root, out, role, pin) for role, pin in contract['cases'].items()}
    refs = {role: sha(git(root, 'ls-tree', '-r', '-z', pin['head'], '--', *REFERENCE))
            for role, pin in contract['cases'].items()}
    need(len(set(refs.values())) == 1 and set(refs.values()) == {contract['referenceSha256']}, 'Harness/TS/dependency inputs differ between variants')
    (out / 'reference-inputs.json').write_bytes(encoded(refs))
    results = []
    raw_by_impl = {'ts': [], 'rust': []}
    for block, order in enumerate(ORDERS):
        for position, role in enumerate(order):
            pin = contract['cases'][role]
            subprocess.run(['git', 'checkout', '--quiet', '--detach', pin['head']], cwd=root, check=True)
            need(git(root, 'rev-parse', 'HEAD').decode().strip() == pin['head'], 'Wrong measured checkout')
            destination = out / 'results' / f'{block + 1:02}-{position + 1}-{role}'
            destination.mkdir(parents=True)
            before = system(root)
            report = destination / 'gate.json'
            command = ['corepack', 'yarn', '-s', 'rust:gate', '--venue', 'bitunix', '--tape',
                       'sdk-rs/harness/fixtures/bitunix-public.jsonl.gz', '--books', 'BTCUSDT,ETHUSDT',
                       '--runs', '9', '--ci-wasm', str(packages[role]), '--report', str(report)]
            output_dir = root / 'sdk-rs/harness/out'
            previous = {p.name for p in output_dir.iterdir()} if output_dir.is_dir() else set()
            try:
                with (destination / 'stdout.log').open('wb') as stdout, (destination / 'stderr.log').open('wb') as stderr:
                    process = subprocess.run(command, cwd=root, stdout=stdout, stderr=stderr, timeout=1800)
            finally:
                preserve_diagnostics(root, destination, previous)
            need(process.returncode in (0, 1) and report.is_file(), 'Gate crashed or produced no complete report')
            gate = loads(report.read_bytes())
            need(gate['venue'] == 'bitunix' and gate['private'] is False and gate['runs'] == 9 and
                 gate['correctnessRuns'] == 1 and gate['books'] == 'BTCUSDT,ETHUSDT' and
                 gate['ciWasm'] == pin['build'] and gate['tapeSha256'] == contract['tapeSha256'] and
                 gate['requiredImprovement'] == 0.10, 'Incomplete/wrong workload or package report')
            need(gate['status'] in ('PASS', 'FAIL') and
                 (process.returncode == 0) == (gate['status'] == 'PASS'), 'Gate exit/status mismatch')
            need(all(PERF_FAILURE.fullmatch(f) for f in gate['failures']),
                 'Correctness/structural failure invalidates experiment')
            artifacts, samples, correctness = collect_raw(root, destination, gate)
            for impl, digest in correctness.items():
                raw_by_impl[impl].append(digest)
            row = {'block': block + 1, 'position': position + 1, 'role': role, 'head': pin['head'],
                   'exit': process.returncode, 'gateStatus': gate['status'], 'failures': gate['failures'],
                   'gateSha256': sha(report.read_bytes()), 'artifacts': artifacts, 'samples': samples,
                   'medians': {key: statistics.median(values) for key, values in samples.items()},
                   'before': before, 'after': system(root)}
            (destination / 'receipt.json').write_bytes(encoded(row))
            results.append(row)
            (out / 'progress.json').write_bytes(encoded(results))
            print('completed', block + 1, position + 1, role, row['medians'], gate['status'], flush=True)
    need(len(results) == 18 and all(len(set(values)) == 1 for values in raw_by_impl.values()),
         'Incomplete schedule or cross-variant correctness changed')
    for role, pin in contract['cases'].items():
        prepare(root, out / 'end-auth', role, pin)
    medians = {}
    for role in contract['cases']:
        rows = [row for row in results if row['role'] == role]
        medians[role] = {impl: statistics.median([value for row in rows for value in row['samples'][impl]])
                         for impl in ['ts', 'rust']}
        medians[role]['rustOverTs'] = medians[role]['rust'] / medians[role]['ts']
    gap = medians['before']['rust'] - medians['landing']['rust']
    normalized_gap = medians['before']['rustOverTs'] - medians['landing']['rustOverTs']
    block_deltas = []
    for block in range(1, 7):
        rows = {row['role']: row['medians'] for row in results if row['block'] == block}
        block_deltas.append({'block': block, 'medians': rows,
            'rustBeforeMinusLanding': rows['before']['rust'] - rows['landing']['rust'],
            'rustAfterMinusLanding': rows['after']['rust'] - rows['landing']['rust'],
            'ratioBeforeMinusLanding': rows['before']['rust'] / rows['before']['ts'] - rows['landing']['rust'] / rows['landing']['ts'],
            'ratioAfterMinusLanding': rows['after']['rust'] / rows['after']['ts'] - rows['landing']['rust'] / rows['landing']['ts']})
    result = {'kind': 'DIAGNOSTIC-SAME-RUNNER-NOT-MERGE-PROOF', 'workflowSourceHead': initial,
              'contractSha256': sha(contract_raw), 'helperSha256': sha(Path(__file__).read_bytes()),
              'results': results, 'medians': medians, 'blockDeltas': block_deltas, 'allSamplesRetained': True,
              'allCrossVariantCorrectnessEqual': True, 'thresholdsChanged': False,
              'rustGapClosedFraction': (medians['before']['rust'] - medians['after']['rust']) / gap if gap > 0 else None,
              'ratioGapClosedFraction': (medians['before']['rustOverTs'] - medians['after']['rustOverTs']) / normalized_gap if normalized_gap > 0 else None,
              'systemAfter': system(root)}
    (out / 'result.json').write_bytes(encoded(result))
    git(root, 'checkout', '--quiet', '--detach', initial)
    print('COMPLETE fixed18-gate/54-pairs-per-variant control', medians, flush=True)

if __name__ == '__main__':
    main()
