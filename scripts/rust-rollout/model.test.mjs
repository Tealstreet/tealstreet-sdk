import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import test from 'node:test';
import { evaluate, compareArtifacts } from './model.mjs';

const digest = (letter) => letter.repeat(64);
const ref = 'a'.repeat(40);
const matrix = {
  schema: 1,
  sdkRef: ref,
  venues: [
    {
      alias: 'test-venue',
      familyLegacyHttpSites: 0,
      rustRegistered: true,
      liveAccountShapes: ['one-way'],
      blockers: [],
    },
  ],
};
function fixture() {
  const bundle = (letter, engine) => ({
    app: digest(letter),
    server: digest(letter),
    worker: digest(letter),
    wasm: digest(letter),
    wireVersion: 'synthetic-wire',
    engine,
    supportedAliases: ['test-venue'],
  });
  return {
    schema: 1,
    stage: 'internal',
    assignment: { pseudonym: 'synthetic-user', target: 'rust', revision: 1 },
    artifacts: { frozen: bundle('a', 'typescript'), rust: bundle('b', 'rust') },
    accounts: [{ id: 'synthetic-account', venue: 'test-venue', shape: 'one-way' }],
    sessions: [
      {
        accountId: 'synthetic-account',
        clientEngine: 'rust',
        serverEngine: 'rust',
        revision: 1,
        app: digest('b'),
        server: digest('b'),
        wireVersion: 'synthetic-wire',
        previousWriterRetired: true,
        resumeFenced: true,
        unknownWrites: 0,
        protectionOwnerTransferred: true,
      },
    ],
    storage: {
      oldNewOldFixture: 'synthetic-proof',
      backwardCompatible: true,
      frozenDbOpens: true,
      cacheAuthority: 'display-only',
    },
    controls: Object.fromEntries(
      ['internalIsolation', 'killSwitch', 'monitoringThresholds', 'rollbackRehearsal', 'samApproval'].map((key) => [
        key,
        'synthetic-proof',
      ]),
    ),
    evidence: [
      {
        venue: 'test-venue',
        shape: 'one-way',
        sdkRef: ref,
        app: digest('b'),
        server: digest('b'),
        observedDays: 7,
        maintenanceEvent: true,
        ...Object.fromEntries(
          [
            'consumer',
            'publicQuiet',
            'publicVolatile',
            'privateShadow',
            'smoke',
            'sessionParity',
            'rollback',
            'productScope',
          ].map((key) => [key, 'synthetic-proof']),
        ),
      },
    ],
  };
}

test('complete synthetic declarations remain dry-run and grant no authority', () => {
  const result = evaluate(matrix, fixture());
  assert.equal(result.declarationsComplete, true);
  assert.equal(result.mode, 'dry-run');
  assert.equal(result.authority, 'none');
  assert.equal(result.liveEvidenceVerified, false);
});

/** @type {[string, (input: any) => void, string][]} */
const blockedCases = [
  [
    'all held aliases',
    (f) => f.accounts.push({ id: 'other', venue: 'unknown', shape: 'spot' }),
    'account-2:unknown-alias',
  ],
  [
    'account shape',
    (f) => {
      f.accounts[0].shape = 'hedge';
    },
    'account-1:unverified-account-shape',
  ],
  [
    'matching server engine',
    (f) => {
      f.sessions[0].serverEngine = 'typescript';
    },
    'account-1:session-engine-or-artifact-mismatch',
  ],
  [
    'assignment revision',
    (f) => {
      f.sessions[0].revision = 2;
    },
    'account-1:session-engine-or-artifact-mismatch',
  ],
  [
    'server digest',
    (f) => {
      f.sessions[0].server = digest('c');
    },
    'account-1:session-engine-or-artifact-mismatch',
  ],
  [
    'frozen alias availability',
    (f) => {
      f.artifacts.frozen.supportedAliases = [];
    },
    'account-1:unavailable-in-frozen',
  ],
  [
    'stale evidence',
    (f) => {
      f.evidence[0].sdkRef = 'c'.repeat(40);
    },
    'account-1:evidence-artifact-mismatch',
  ],
  [
    'private shadow',
    (f) => {
      delete f.evidence[0].privateShadow;
    },
    'account-1:missing-proof:privateShadow',
  ],
  [
    'unknown outcomes',
    (f) => {
      f.sessions[0].unknownWrites = 1;
    },
    'account-1:unreconciled-writes',
  ],
  [
    'old resume rights',
    (f) => {
      f.sessions[0].resumeFenced = false;
    },
    'account-1:old-owner-not-fenced',
  ],
  [
    'cache is display-only',
    (f) => {
      f.storage.cacheAuthority = 'loaded';
    },
    'storage-flip-back-unproven',
  ],
  [
    'backward compatibility',
    (f) => {
      f.storage.backwardCompatible = false;
    },
    'storage-flip-back-unproven',
  ],
  [
    'kill switch',
    (f) => {
      delete f.controls.killSwitch;
    },
    'missing-control:killSwitch',
  ],
  [
    'declared monitoring thresholds',
    (f) => {
      delete f.controls.monitoringThresholds;
    },
    'missing-control:monitoringThresholds',
  ],
  [
    'observation window',
    (f) => {
      f.evidence[0].observedDays = 6;
    },
    'account-1:observation-incomplete',
  ],
  [
    'duplicate session',
    (f) => {
      f.sessions.push({ ...f.sessions[0] });
    },
    'account-1:missing-or-duplicate-session',
  ],
  [
    'duplicate evidence',
    (f) => {
      f.evidence.push({ ...f.evidence[0] });
    },
    'account-1:missing-or-duplicate-evidence',
  ],
];
for (const [name, change, expected] of blockedCases)
  test(`blocks ${name}`, () => {
    const f = fixture();
    change(f);
    const result = evaluate(matrix, f);
    assert.equal(result.declarationsComplete, false);
    assert.ok(result.blockers.includes(expected));
  });

test('credentials, arbitrary fields, empty accounts and duplicate account IDs are refused', () => {
  const cases = [
    (f) => {
      f.accounts[0].apiKey = 'never-print-this';
    },
    (f) => {
      f.apply = true;
    },
    (f) => {
      f.accounts = [];
    },
    (f) => {
      f.accounts.push({ ...f.accounts[0] });
    },
  ];
  for (const change of cases) {
    const f = fixture();
    change(f);
    assert.throws(() => evaluate(matrix, f));
  }
});

test('source blockers cannot be cleared by supplying happy rehearsal declarations', () => {
  const blocked = structuredClone(matrix);
  blocked.venues[0].blockers = ['legacy-removal-unresolved'];
  assert.ok(evaluate(blocked, fixture()).blockers.includes('account-1:legacy-removal-unresolved'));
});

const file = (path, hash, bytes) => ({
  path,
  sha256: digest(hash),
  bytes,
  gzipBytes: bytes / 2,
  brotliBytes: bytes / 4,
});
const manifest = (files) => ({ schema: 1, revision: ref, files });
test('artifact comparison distinguishes touched/removed files from additive bytes', () => {
  const result = compareArtifacts(
    manifest([file('app.js', 'a', 100), file('old.js', 'a', 40)]),
    manifest([file('app.js', 'b', 120), file('new.wasm', 'c', 80)]),
  );
  assert.deepEqual(result.modified, ['app.js']);
  assert.deepEqual(result.added, ['new.wasm']);
  assert.deepEqual(result.removed, ['old.js']);
  assert.equal(result.additiveOnly, false);
  assert.deepEqual(result.delta, { bytes: 60, gzipBytes: 30, brotliBytes: 15 });
});

test('same-hash additions are additive; incomplete or inconsistent sizes never become a size claim', () => {
  const before = manifest([file('app.js', 'a', 100)]);
  assert.equal(compareArtifacts(before, manifest([...before.files, file('new.js', 'b', 40)])).additiveOnly, true);
  const bad = [
    manifest([]),
    manifest([file('../escape', 'b', 40)]),
    manifest([file('app.js', 'a', 104)]),
    manifest([file('app.js', 'a', 100), file('app.js', 'b', 40)]),
  ];
  for (const candidate of bad) assert.throws(() => compareArtifacts(before, candidate));
  const missing = manifest([file('new.js', 'b', 40)]);
  delete missing.files[0].gzipBytes;
  assert.throws(() => compareArtifacts(before, missing));
});

test('committed matrix covers every registry alias at its immutable source pin', () => {
  const data = JSON.parse(
    readFileSync(new URL('../../docs/plans/rust-bundle-readiness.json', import.meta.url), 'utf8'),
  );
  const registry = execFileSync('git', ['show', `${data.sdkRef}:packages/safe-cex/src/exchanges/registry.ts`], {
    encoding: 'utf8',
  });
  const aliases = [...registry.matchAll(/^  (?:'([^']+)'|([\w-]+)): \(\) =>/gm)]
    .map((match) => match[1] ?? match[2])
    .sort();
  assert.deepEqual(data.venues.map((row) => row.alias).sort(), aliases);
  assert.ok(data.venues.every((row) => row.liveAccountShapes.length === 0 && row.blockers.length > 0));
});

test('CLI is offline-only and refuses apply/deploy/flag mutations', () => {
  const cli = new URL('./cli.mjs', import.meta.url);
  for (const args of [['--apply'], ['deploy'], ['evaluate', '--apply'], ['matrix', '--flip']]) {
    const result = spawnSync(process.execPath, [cli.pathname, ...args], { encoding: 'utf8' });
    assert.equal(result.status, 2);
    assert.match(result.stderr, /offline preparation only/);
  }
  const matrixResult = spawnSync(process.execPath, [cli.pathname, 'matrix'], { encoding: 'utf8' });
  assert.equal(matrixResult.status, 0, matrixResult.stderr);
  assert.equal(JSON.parse(matrixResult.stdout).authority, 'none');
});

test('source matrix family counts, defaults and signing declarations match pinned code', () => {
  const data = JSON.parse(
    readFileSync(new URL('../../docs/plans/rust-bundle-readiness.json', import.meta.url), 'utf8'),
  );
  const source = (path) => execFileSync('git', ['show', `${data.sdkRef}:${path}`], { encoding: 'utf8' });
  const inventory = JSON.parse(source('sdk-rs/scripts/request-legacy-sites.json'));
  const construction = JSON.parse(source('packages/safe-cex/src/exchanges/rust/construction-rules.json'));
  const registry = source('packages/safe-cex/src/exchanges/registry.ts');
  const totals = {};
  for (const [path, sites] of Object.entries(inventory)) {
    const family = path.split('/')[1];
    totals[family] = (totals[family] ?? 0) + Object.values(sites).reduce((sum, count) => sum + count, 0);
  }
  for (const row of data.venues) {
    assert.equal(row.familyLegacyHttpSites, totals[row.family] ?? 0, row.alias);
    assert.equal(
      row.dispatchSigningDeclared,
      construction.venues[row.alias]?.bridge?.dispatchSigning ?? false,
      row.alias,
    );
    const start = registry.indexOf(`${row.alias.includes('-') ? `'${row.alias}'` : row.alias}: () =>`);
    assert.ok(start >= 0);
    const entry = registry.slice(start).split(/\n  (?=\/\/|[\w'-]+:)|\n};/)[0];
    assert.equal(row.registryDefault, entry.includes('RustEngineExchange') ? 'rust' : 'typescript', row.alias);
  }
});

test('the shipped synthetic example stays blocked, without echoing its pseudonym', () => {
  const result = spawnSync(
    process.execPath,
    [new URL('./cli.mjs', import.meta.url).pathname, 'evaluate', new URL('./example.json', import.meta.url).pathname],
    { encoding: 'utf8' },
  );
  assert.equal(result.status, 2, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.equal(output.declarationsComplete, false);
  assert.ok(output.blockers.some((reason) => reason.endsWith('live-evidence-not-certified')));
  assert.ok(!result.stdout.includes('synthetic-user'));
});

test('opt-in and widening require the staged human decisions', () => {
  const input = fixture();
  input.stage = 'opt-in';
  assert.ok(evaluate(matrix, input).blockers.includes('missing-control:optInConsent'));
  input.controls.optInConsent = 'synthetic-consent';
  assert.equal(evaluate(matrix, input).declarationsComplete, true);
  input.stage = 'widen';
  assert.ok(evaluate(matrix, input).blockers.includes('missing-control:priorCohortReview'));
  input.controls.priorCohortReview = 'synthetic-reviewed-cohort';
  assert.equal(evaluate(matrix, input).declarationsComplete, true);
});
