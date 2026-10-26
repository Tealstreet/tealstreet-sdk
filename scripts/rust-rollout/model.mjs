const SHA256 = /^[a-f0-9]{64}$/;
const COMMIT = /^[a-f0-9]{40}$/;
const STAGES = new Set(['internal', 'opt-in', 'widen']);
const PROOFS = [
  'consumer',
  'publicQuiet',
  'publicVolatile',
  'privateShadow',
  'smoke',
  'sessionParity',
  'rollback',
  'productScope',
];
const CONTROLS = ['internalIsolation', 'killSwitch', 'monitoringThresholds', 'rollbackRehearsal', 'samApproval'];

function requireValue(ok, label) {
  if (!ok) throw new Error(label);
}

function record(value, label) {
  requireValue(value && typeof value === 'object' && !Array.isArray(value), `Invalid ${label}`);
}

function exactKeys(value, allowed, label) {
  record(value, label);
  requireValue(
    Object.keys(value).every((key) => allowed.includes(key)),
    `Unexpected field in ${label}`,
  );
}

function reference(value) {
  return typeof value === 'string' && value.trim().length > 0;
}

function count(value) {
  return Number.isSafeInteger(value) && value >= 0;
}

function rejectCredentials(value) {
  if (Array.isArray(value)) return value.forEach(rejectCredentials);
  if (!value || typeof value !== 'object') return;
  for (const [key, child] of Object.entries(value)) {
    requireValue(
      !/^(api[_-]?key|api[_-]?secret|secret|passphrase|token|signature|credentials)$/i.test(key),
      'Credential field refused; use synthetic manifests only',
    );
    rejectCredentials(child);
  }
}

export function validateMatrix(matrix) {
  record(matrix, 'matrix');
  requireValue(matrix.schema === 1 && COMMIT.test(matrix.sdkRef), 'Invalid matrix identity');
  requireValue(Array.isArray(matrix.venues) && matrix.venues.length > 0, 'Empty matrix');
  const aliases = new Set();
  for (const row of matrix.venues) {
    requireValue(reference(row.alias) && !aliases.has(row.alias), 'Invalid or duplicate matrix alias');
    aliases.add(row.alias);
    requireValue(row.rustRegistered === true && count(row.familyLegacyHttpSites), 'Invalid source row');
    requireValue(Array.isArray(row.blockers) && Array.isArray(row.liveAccountShapes), 'Invalid readiness row');
  }
  return matrix;
}

function artifact(value, engine, label) {
  exactKeys(value, ['app', 'server', 'worker', 'wasm', 'wireVersion', 'engine', 'supportedAliases'], label);
  requireValue(
    ['app', 'server', 'worker', 'wasm'].every((key) => SHA256.test(value[key])),
    `Missing immutable ${label} digests`,
  );
  requireValue(reference(value.wireVersion) && value.engine === engine, `Invalid ${label} engine/wire`);
  requireValue(
    Array.isArray(value.supportedAliases) && value.supportedAliases.every(reference),
    `Invalid ${label} aliases`,
  );
}

// This validates submitted rehearsal declarations; it never verifies live evidence or grants rollout authority.
export function evaluate(matrix, input) {
  validateMatrix(matrix);
  rejectCredentials(input);
  exactKeys(
    input,
    ['schema', 'stage', 'assignment', 'artifacts', 'accounts', 'sessions', 'storage', 'controls', 'evidence'],
    'input',
  );
  requireValue(input.schema === 1 && STAGES.has(input.stage), 'Invalid rehearsal stage');
  exactKeys(input.assignment, ['pseudonym', 'target', 'revision'], 'assignment');
  requireValue(
    reference(input.assignment.pseudonym) &&
      input.assignment.target === 'rust' &&
      count(input.assignment.revision) &&
      input.assignment.revision > 0,
    'Invalid assignment',
  );
  exactKeys(input.artifacts, ['frozen', 'rust'], 'artifacts');
  artifact(input.artifacts.frozen, 'typescript', 'frozen artifact');
  artifact(input.artifacts.rust, 'rust', 'Rust artifact');
  requireValue(input.artifacts.frozen.app !== input.artifacts.rust.app, 'Frozen and Rust artifacts must differ');
  requireValue(Array.isArray(input.accounts) && input.accounts.length > 0, 'Missing all-held-account inventory');
  requireValue(Array.isArray(input.sessions) && Array.isArray(input.evidence), 'Invalid sessions/evidence');
  exactKeys(input.storage, ['oldNewOldFixture', 'backwardCompatible', 'frozenDbOpens', 'cacheAuthority'], 'storage');
  exactKeys(input.controls, [...CONTROLS, 'optInConsent', 'priorCohortReview'], 'controls');
  const blockers = [];
  if (
    !reference(input.storage.oldNewOldFixture) ||
    input.storage.backwardCompatible !== true ||
    input.storage.frozenDbOpens !== true ||
    input.storage.cacheAuthority !== 'display-only'
  )
    blockers.push('storage-flip-back-unproven');
  for (const name of CONTROLS) if (!reference(input.controls[name])) blockers.push(`missing-control:${name}`);
  if (input.stage !== 'internal' && !reference(input.controls.optInConsent))
    blockers.push('missing-control:optInConsent');
  if (input.stage === 'widen' && !reference(input.controls.priorCohortReview))
    blockers.push('missing-control:priorCohortReview');
  const seen = new Set();
  for (const [index, account] of input.accounts.entries()) {
    exactKeys(account, ['id', 'venue', 'shape'], 'account');
    requireValue(
      reference(account.id) && !seen.has(account.id) && reference(account.venue) && reference(account.shape),
      'Invalid or duplicate account',
    );
    seen.add(account.id);
    const label = `account-${index + 1}`;
    const row = matrix.venues.find((item) => item.alias === account.venue);
    if (!row) {
      blockers.push(`${label}:unknown-alias`);
      continue;
    }
    for (const reason of row.blockers) blockers.push(`${label}:${reason}`);
    if (!row.liveAccountShapes.includes(account.shape)) blockers.push(`${label}:unverified-account-shape`);
    for (const bundle of ['frozen', 'rust'])
      if (!input.artifacts[bundle].supportedAliases.includes(account.venue))
        blockers.push(`${label}:unavailable-in-${bundle}`);
    const sessions = input.sessions.filter((item) => item.accountId === account.id);
    if (sessions.length !== 1) {
      blockers.push(`${label}:missing-or-duplicate-session`);
      continue;
    }
    const session = sessions[0];
    exactKeys(
      session,
      [
        'accountId',
        'clientEngine',
        'serverEngine',
        'revision',
        'app',
        'server',
        'wireVersion',
        'previousWriterRetired',
        'resumeFenced',
        'unknownWrites',
        'protectionOwnerTransferred',
      ],
      'session',
    );
    if (
      session.clientEngine !== 'rust' ||
      session.serverEngine !== 'rust' ||
      session.revision !== input.assignment.revision ||
      session.app !== input.artifacts.rust.app ||
      session.server !== input.artifacts.rust.server ||
      session.wireVersion !== input.artifacts.rust.wireVersion
    )
      blockers.push(`${label}:session-engine-or-artifact-mismatch`);
    if (
      session.previousWriterRetired !== true ||
      session.resumeFenced !== true ||
      session.protectionOwnerTransferred !== true
    )
      blockers.push(`${label}:old-owner-not-fenced`);
    if (session.unknownWrites !== 0) blockers.push(`${label}:unreconciled-writes`);
    const proofs = input.evidence.filter((item) => item.venue === account.venue && item.shape === account.shape);
    if (proofs.length !== 1) {
      blockers.push(`${label}:missing-or-duplicate-evidence`);
      continue;
    }
    const proof = proofs[0];
    exactKeys(
      proof,
      ['venue', 'shape', 'sdkRef', 'app', 'server', ...PROOFS, 'observedDays', 'maintenanceEvent'],
      'evidence',
    );
    if (
      proof.sdkRef !== matrix.sdkRef ||
      proof.app !== input.artifacts.rust.app ||
      proof.server !== input.artifacts.rust.server
    )
      blockers.push(`${label}:evidence-artifact-mismatch`);
    for (const key of PROOFS) if (!reference(proof[key])) blockers.push(`${label}:missing-proof:${key}`);
    if (!count(proof.observedDays) || proof.observedDays < 7 || proof.maintenanceEvent !== true)
      blockers.push(`${label}:observation-incomplete`);
  }
  if (input.sessions.some((session) => !seen.has(session.accountId)))
    blockers.push('session-outside-account-inventory');
  if (
    input.evidence.some(
      (proof) => !input.accounts.some((account) => account.venue === proof.venue && account.shape === proof.shape),
    )
  )
    blockers.push('evidence-outside-account-inventory');
  return {
    mode: 'dry-run',
    authority: 'none',
    liveEvidenceVerified: false,
    stage: input.stage,
    accountsChecked: input.accounts.length,
    declarationsComplete: blockers.length === 0,
    blockers: [...new Set(blockers)],
    next: 'Human review of declarations and actual evidence; this tool cannot authorize a flag flip.',
  };
}

function fileManifest(manifest) {
  exactKeys(manifest, ['schema', 'revision', 'files'], 'artifact manifest');
  requireValue(
    manifest.schema === 1 &&
      COMMIT.test(manifest.revision) &&
      Array.isArray(manifest.files) &&
      manifest.files.length > 0,
    'Invalid artifact manifest',
  );
  const files = new Map();
  for (const file of manifest.files) {
    exactKeys(file, ['path', 'sha256', 'bytes', 'gzipBytes', 'brotliBytes'], 'artifact file');
    requireValue(
      reference(file.path) &&
        !file.path.startsWith('/') &&
        !file.path.split('/').includes('..') &&
        !files.has(file.path) &&
        SHA256.test(file.sha256),
      'Invalid or duplicate artifact path/hash',
    );
    requireValue(
      ['bytes', 'gzipBytes', 'brotliBytes'].every((key) => count(file[key])),
      'Missing measured raw/gzip/brotli bytes',
    );
    files.set(file.path, file);
  }
  return files;
}

export function compareArtifacts(before, after) {
  const oldFiles = fileManifest(before),
    newFiles = fileManifest(after);
  const added = [],
    modified = [],
    removed = [];
  for (const [path, file] of newFiles) {
    const prior = oldFiles.get(path);
    if (!prior) added.push(path);
    else if (file.sha256 !== prior.sha256) modified.push(path);
    else
      requireValue(
        ['bytes', 'gzipBytes', 'brotliBytes'].every((key) => file[key] === prior[key]),
        'Identical hash has inconsistent sizes',
      );
  }
  for (const path of oldFiles.keys()) if (!newFiles.has(path)) removed.push(path);
  const totals = (files) =>
    Object.fromEntries(
      ['bytes', 'gzipBytes', 'brotliBytes'].map((key) => [
        key,
        [...files.values()].reduce((sum, file) => sum + file[key], 0),
      ]),
    );
  const oldSize = totals(oldFiles),
    newSize = totals(newFiles);
  requireValue([...Object.values(oldSize), ...Object.values(newSize)].every(count), 'Artifact total overflow');
  return {
    mode: 'dry-run',
    authority: 'none',
    before: before.revision,
    after: after.revision,
    added,
    modified,
    removed,
    additiveOnly: modified.length === 0 && removed.length === 0,
    oldSize,
    newSize,
    delta: Object.fromEntries(Object.keys(oldSize).map((key) => [key, newSize[key] - oldSize[key]])),
  };
}
