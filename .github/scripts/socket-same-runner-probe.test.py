"""Offline protocol controls; never builds WASM or runs a replay."""
import copy, importlib.util, io, json, tempfile, unittest, zipfile
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('socket-same-runner-probe.py'))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)

class ProtocolTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.out = self.root / 'retained'
        self.out.mkdir()
        self.records = []
        self.raw_dir = self.root / 'sdk-rs/harness/out/test'
        self.raw_dir.mkdir(parents=True)
        for impl in ['ts', 'rust']:
            for mode, count in [('correctness', 1), ('performance', 9)]:
                for number in range(count):
                    source = self.raw_dir / f'{impl}-{mode}-{number}.json'
                    source.write_bytes(p.encoded({'impl': impl, 'books': {}, 'metrics':
                        {'steadyCpuMs': number + 1, 'measuredMode': mode}}))
                    self.records.append({'impl': impl, 'mode': mode, 'run': number,
                                         'path': str(source.relative_to(self.root))})
        self.gate = {'artifacts': self.records}

    def test_roster_positive(self):
        artifacts, samples, correctness = p.collect_raw(self.root, self.out, self.gate)
        self.assertEqual(len(artifacts), 20)
        self.assertEqual({k: len(v) for k, v in samples.items()}, {'ts': 9, 'rust': 9})
        self.assertEqual(set(correctness), {'ts', 'rust'})

    def test_reject_roster_mutations(self):
        for name in ['duplicate', 'missing', 'extra', 'unknown_impl', 'unknown_mode',
                     'negative_run', 'high_run', 'boolean_run', 'duplicate_path']:
            with self.subTest(name=name):
                gate = copy.deepcopy(self.gate)
                if name == 'duplicate': gate['artifacts'][2] = copy.deepcopy(gate['artifacts'][1])
                if name == 'missing': gate['artifacts'].pop(0)
                if name == 'extra': gate['artifacts'].append(copy.deepcopy(gate['artifacts'][0]))
                if name == 'unknown_impl': gate['artifacts'][0]['impl'] = 'other'
                if name == 'unknown_mode': gate['artifacts'][0]['mode'] = 'unknown'
                if name == 'negative_run': gate['artifacts'][1]['run'] = -1
                if name == 'high_run': gate['artifacts'][1]['run'] = 9
                if name == 'boolean_run': gate['artifacts'][1]['run'] = False
                if name == 'duplicate_path': gate['artifacts'][1]['path'] = gate['artifacts'][0]['path']
                with self.assertRaises(ValueError): p.collect_raw(self.root, self.out, gate)

    def test_reject_raw_identity_and_cpu(self):
        source = self.root / self.records[1]['path']
        original = p.loads(source.read_bytes())
        for mutation in [{'impl': 'rust'}, {'metrics': {'measuredMode': 'correctness', 'steadyCpuMs': 1}},
                         {'metrics': {'measuredMode': 'performance', 'steadyCpuMs': 0}},
                         {'metrics': {'measuredMode': 'performance', 'steadyCpuMs': True}}]:
            with self.subTest(mutation=mutation):
                source.write_bytes(p.encoded({**original, **mutation}))
                with self.assertRaises(ValueError): p.collect_raw(self.root, self.out, self.gate)
        source.write_text('{"impl":"ts","metrics":{"measuredMode":"performance","steadyCpuMs":1e999}}')
        with self.assertRaises(ValueError): p.collect_raw(self.root, self.out, self.gate)

    def test_all_json_finite_and_unique(self):
        for raw in ['{"a":1e999}', '{"metrics":{"a":-1e999}}', '{"a":NaN}',
                    '{"a":Infinity}', '{"a":-Infinity}', '{"a":1,"a":2}']:
            with self.subTest(raw=raw), self.assertRaises(ValueError): p.loads(raw)
        self.assertEqual(p.loads('{"a":1.25}'), {'a': 1.25})

    def test_preserve_incomplete_output(self):
        (self.raw_dir / 'crash.stderr').write_text('incomplete sample retained')
        p.preserve_diagnostics(self.root, self.out, set())
        self.assertEqual((self.out / 'harness-diagnostics/test/crash.stderr').read_text(),
                         'incomplete sample retained')
        other = self.root / 'another'
        p.preserve_diagnostics(self.root, other, {'test'})
        self.assertFalse(other.exists())

    def fixture(self):
        start, end = '2026-10-23T01:00:00Z', '2026-10-23T01:00:01Z'
        payload = {name: name.encode() for name in p.PACKAGE_FILES - {'build.json'}}
        build = {'sourceHead': 'checkout', 'runId': '1', 'family': 'bitunix', 'venue': 'bitunix',
                 'profile': 'release', 'target': 'nodejs', 'hashes': {k: p.sha(v) for k, v in payload.items()}}
        payload['build.json'] = p.encoded(build)
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w') as archive:
            for name, value in payload.items(): archive.writestr(name, value)
        blob = stream.getvalue()
        artifact = {'name': 'ci-wasm-bitunix', 'id': 2, 'expired': False, 'size_in_bytes': len(blob),
                    'digest': 'sha256:' + p.sha(blob), 'created_at': start, 'updated_at': start}
        log = ('2026-10-23T00:00:00Z [command]/usr/bin/git log -1 --format=%H\n'
               '2026-10-23T00:00:00Z checkout\n'
               f'{start} SHA256 digest of uploaded artifact is {p.sha(blob)}\n'
               f'{start} Artifact ci-wasm-bitunix successfully finalized. Artifact ID 2\n'
               f'{start} Artifact ci-wasm-bitunix has been successfully uploaded! Final size is {len(blob)} bytes. Artifact ID is 2\n').encode()
        state = {'run_attempt': 1, 'status': 'completed', 'conclusion': 'failure'}
        pin = {'attempt': 1, 'family': 'bitunix', 'run': 1, 'head': 'head', 'branch': 'branch',
               'event': 'pull_request', 'latestState': dict(state), 'exactState': dict(state),
               'job': 3, 'jobConclusion': 'failure', 'artifact': artifact, 'build': build,
               'logSha256': p.sha(log), 'commits': {
                   'head': {'sha': 'head', 'tree': 'same', 'parents': ['parent']},
                   'checkout': {'sha': 'checkout', 'tree': 'same', 'parents': ['base', 'head']}}}
        record = {'id': 1, 'head_sha': 'head', 'head_branch': 'branch', 'event': 'pull_request',
                  'path': '.github/workflows/rust.yml', **state}
        data = {'actions/runs/1': record, 'actions/runs/1/attempts/1': copy.deepcopy(record),
                'actions/runs/1/artifacts?per_page=100': {'total_count': 1, 'artifacts': [artifact]},
                'actions/jobs/3': {'id': 3, 'run_id': 1, 'run_attempt': 1, 'head_sha': 'head',
                    'head_branch': 'branch', 'name': 'Rust / replay gate (bitunix)', 'status': 'completed',
                    'conclusion': 'failure', 'steps': [{'name': 'Retain the matching CI replay engine briefly',
                      'status': 'completed', 'conclusion': 'success', 'started_at': start, 'completed_at': end}]}}
        for commit in pin['commits'].values():
            data['git/commits/' + commit['sha']] = {'sha': commit['sha'], 'tree': {'sha': commit['tree']},
                                                     'parents': [{'sha': v} for v in commit['parents']]}
        return pin, copy.deepcopy(data), log, blob

    def prepare(self, pin, data, log, blob):
        with patch.object(p, 'api', side_effect=lambda root, path: copy.deepcopy(data[path])), \
             patch.object(p, 'run', side_effect=lambda args, root: log if args[-1].endswith('/logs') else blob):
            return p.prepare(self.root, self.out, 'case', pin)

    def test_package_provenance_positive(self):
        args = self.fixture()
        result = self.prepare(*args)
        self.assertEqual((result / 'original.zip').read_bytes(), args[-1])
        self.assertTrue((result / 'commits.json').is_file())

    def test_latest_retry_does_not_relabel_physical_job(self):
        pin, data, log, blob = self.fixture()
        pin['latestState'] = {'run_attempt': 2, 'status': 'completed', 'conclusion': 'success'}
        data['actions/runs/1'].update(pin['latestState'])
        self.prepare(pin, data, log, blob)

    def test_reject_provenance_mutations(self):
        for name in ['retry', 'pending', 'head', 'tree', 'parents', 'duplicate', 'expired',
                     'job_attempt', 'job_conclusion', 'log', 'zip', 'window', 'upload_step', 'checkout']:
            with self.subTest(name=name):
                pin, data, log, blob = self.fixture()
                if name == 'retry': data['actions/runs/1']['run_attempt'] = 2
                if name == 'pending': data['actions/runs/1']['status'] = 'in_progress'
                if name == 'head': data['actions/runs/1']['head_sha'] = 'other'
                if name == 'tree': data['git/commits/checkout']['tree']['sha'] = 'other'
                if name == 'parents': data['git/commits/checkout']['parents'].reverse()
                if name == 'duplicate':
                    inv = data['actions/runs/1/artifacts?per_page=100']; inv['artifacts'] *= 2; inv['total_count'] = 2
                if name == 'expired': data['actions/runs/1/artifacts?per_page=100']['artifacts'][0]['expired'] = True
                if name == 'job_attempt': data['actions/jobs/3']['run_attempt'] = 2
                if name == 'job_conclusion': data['actions/jobs/3']['conclusion'] = 'cancelled'
                if name == 'log': log += b'changed'
                if name == 'zip': blob += b'changed'
                if name == 'window': data['actions/jobs/3']['steps'][0]['started_at'] = '2026-10-23T02:00:00Z'
                if name == 'upload_step': data['actions/jobs/3']['steps'][0]['conclusion'] = 'failure'
                if name == 'checkout':
                    log = log.replace(b'Z checkout\n', b'Z wrong\n'); pin['logSha256'] = p.sha(log)
                with self.assertRaises(ValueError): self.prepare(pin, data, log, blob)

    def test_fixed_balanced_schedule(self):
        self.assertEqual(len(p.ORDERS), 6)
        self.assertEqual(len(set(p.ORDERS)), 6)
        for position in range(3):
            for role in ['landing', 'before', 'after']:
                self.assertEqual(sum(order[position] == role for order in p.ORDERS), 2)

if __name__ == '__main__':
    unittest.main()
