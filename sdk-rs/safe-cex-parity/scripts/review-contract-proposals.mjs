import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from './accepted-inventory.mjs';
import { loadContracts } from './accepted-contract-files.mjs';

// Review documents deliberately have no contract-shaped `category` field. They
// are not inputs to accepted-set.mjs and confer no discovery/runtime permission.
export function compilerNames(doc) {
  assert.equal(doc.format_version, 61, 'review changed rustdoc JSON format');
  const names = new Map();
  const visit = (id, prefix) => {
    const item = doc.index[String(id)];
    assert.ok(item, `missing compiler declaration ${id}`);
    if (item.crate_id !== 0 || item.inner.use || item.name == null || item.name === '_') return;
    const name = [...prefix, item.name].join('::');
    assert.ok(!names.has(name), `ambiguous compiler declaration ${name}`);
    names.set(name, item);
    if (item.inner.module) for (const child of item.inner.module.items) visit(child, [...prefix, item.name]);
  };
  visit(doc.root, []);
  return names;
}

export function normalizeCompiler(value, doc, names = compilerNames(doc)) {
  const local = new Map([...names].map(([name, item]) => [String(item.id), name]));
  const normalize = value => {
    if (Array.isArray(value)) return value.map(normalize);
    if (!value || typeof value !== 'object') return value;
    const result = Object.fromEntries(Object.entries(value).filter(([key]) => key !== 'id').map(([key, child]) => [key, normalize(child)]));
    if (value.id != null && value.path) result.path = doc.paths[String(value.id)]?.path.join('::') ?? local.get(String(value.id)) ?? value.path;
    return result;
  };
  return normalize(value);
}

export function proposalKind(owner, kind) {
  if (kind === 'alias') return {
    classificationCandidate: 'review-required',
    review: 'Inspect the actual expanded alias and compiler trait probes; generic arguments and raw target spelling do not establish decoder behavior.',
    nativeEvidenceRequired: ['Actual alias/root compiler identity', 'Executed Deserialize and alternate-conversion trait probes', 'Direct native decoded-value witnesses if any decoder exists'],
  };
  if (kind === 'construction') return {
    classificationCandidate: 'construction-policy-review',
    review: 'Review private state and each actual Clone/Copy implementation; choose value duplication or shared handle semantics from source and native observations.',
    nativeEvidenceRequired: ['Native duplication/identity observation appropriate to this specific owner'],
  };
  if (owner.deserialize) return {
    classificationCandidate: 'decoder',
    shapeCandidates: owner.kind === 'record' ? ['map', 'union', 'json', 'uninhabited'] : ['string', 'map', 'union', 'json', 'uninhabited'],
    review: 'The compiler confirms Deserialize. Determine the intended TS/tooling input domain independently, including Serde attributes and all alternate APIs.',
    nativeEvidenceRequired: ['Independently reviewed positive fixtures', 'Actual trait and qualified decoder shape probes', 'Direct native output observations for raw decoder APIs'],
  };
  const raw = owner.rawEntrypoints ?? [];
  if (owner.kind !== 'function' && raw.length && raw.every(value => value === false)) return {
    classificationCandidate: 'typed-construction-review',
    review: 'The inventoried entrypoints have no compiler-detected raw input. Inspect each body and constructor result before writing its specific typed/non-decoder reason.',
    nativeEvidenceRequired: ['Executed native witnesses for each typed constructor/API classified as runtime coverage'],
  };
  return {
    classificationCandidate: 'review-required',
    review: 'A raw signature or decoded output is a candidate, not proof of a decoder. Review each body and classify decoder or non-decoder with a function-specific reason.',
    nativeEvidenceRequired: ['For decoders: independently authored input/output cases and actual native calls', 'For non-decoders: reviewed body/role plus native compiler classification receipt; existing tests are supplementary'],
  };
}

export function sourceEvidence(span, { workspace, repository, maxLines = 32 }) {
  assert.ok(span?.filename && span.begin?.length === 2 && span.end?.length === 2, 'compiler source span required');
  const filename = fs.realpathSync(path.resolve(workspace, span.filename));
  const relative = path.relative(fs.realpathSync(repository), filename);
  assert.ok(relative && relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative) && filename.endsWith('.rs'), 'compiler source must be a Rust file inside this checkout');
  const bytes = fs.readFileSync(filename);
  const lines = bytes.toString('utf8').split('\n');
  assert.ok(span.begin[0] >= 1 && span.end[0] >= span.begin[0] && span.end[0] <= lines.length, 'compiler source span outside file');
  const end = Math.min(span.end[0], span.begin[0] + maxLines - 1);
  return {
    file: relative.split(path.sep).join('/'), begin: span.begin, end: span.end,
    sourceSha256: createHash('sha256').update(bytes).digest('hex'),
    excerpt: lines.slice(span.begin[0] - 1, end).map((text, i) => `${span.begin[0] + i}: ${text}`).join('\n'),
    excerptTruncated: end < span.end[0],
  };
}

export function targetProposals(doc, row, context) {
  assert.ok(!row.error, `${row.crate}/${row.target}: resolve blocked census inventory before proposing contracts: ${row.error}`);
  for (const key of ['missingOwners', 'missingAliases', 'missingConstruction', 'receiverDiagnostics']) assert.ok(Array.isArray(row[key]), `census requires ${key}`);
  const names = compilerNames(doc);
  // This existing regression selector asserts the exact requested compiler
  // owners. It is used only for review data, never a substituted gate inventory.
  const owners = row.missingOwners.length ? inventory(doc, { onlyOwners: row.missingOwners }).types : [];
  const proposals = [];
  const add = (name, kind, compiler, spans) => {
    const evidence = spans.filter(Boolean).map(span => sourceEvidence(span, context));
    proposals.push({
      name, kind, status: 'needs-human-review', gatePermission: false,
      ...proposalKind(compiler, kind), compiler,
      existingClassification: context.existingClassification?.(name, kind) ?? null,
      sourceEvidence: evidence,
      nativeEvidence: { behaviorExecuted: false, note: 'Fresh compiler discovery is not native behavioral proof. Textual test references below have not been executed by this helper.' },
      testReferences: context.testReferences?.(names.get(name)?.name ?? name.split('::').at(-1)) ?? [],
    });
  };
  for (const owner of owners) add(owner.name, 'owner', owner, [owner.span, ...(owner.entrypointSources ?? [])]);
  for (const name of row.missingAliases) {
    const item = names.get(name);
    assert.ok(item?.inner.type_alias && item.visibility === 'public', `stale census: missing public compiler alias ${name}`);
    add(name, 'alias', { name, rawTarget: normalizeCompiler(item.inner.type_alias.type, doc, names), generics: normalizeCompiler(item.inner.type_alias.generics, doc, names) }, [item.span]);
  }
  for (const name of row.missingConstruction) {
    const item = names.get(name);
    const declaration = item?.inner.struct ?? item?.inner.enum ?? item?.inner.union;
    assert.ok(declaration, `stale census: missing construction owner ${name}`);
    const implementations = declaration.impls.map(id => doc.index[String(id)]).filter(item => item?.inner.impl && !item.inner.impl.is_negative && !item.inner.impl.blanket_impl);
    const duplication = implementations.filter(item => ['core::clone::Clone', 'core::marker::Copy'].includes(doc.paths[String(item.inner.impl.trait?.id)]?.path.join('::')));
    assert.ok(duplication.length, `stale census: missing Clone/Copy implementation ${name}`);
    add(name, 'construction', { name, traits: duplication.map(item => doc.paths[String(item.inner.impl.trait.id)].path.join('::')).sort() }, [item.span, ...duplication.map(item => item.span)]);
  }
  return { crate: row.crate, target: row.target, proposals, receiverRefusals: row.receiverDiagnostics, limitations: 'Selected census keys only. Receiver diagnostics remain refusals; aliases have raw targets only, without expanded/root identity validation. No local-scope or feature/wasm matrix coverage.' };
}

function command(program, args, cwd, env = {}) {
  const result = spawnSync(program, args, { cwd, env: { ...process.env, ...env }, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  assert.equal(result.status, 0, `${program} ${args.join(' ')} failed\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}

function testIndex(crateRoot, repository) {
  const result = spawnSync('rg', ['--files', crateRoot, '-g', '*.rs', '-g', '*.mjs', '-g', '*.cjs', '-g', '!target/**'], { encoding: 'utf8' });
  assert.equal(result.status, 0, `cannot enumerate test candidates: ${result.stderr}`);
  const records = [];
  for (const filename of result.stdout.trim().split('\n').filter(Boolean)) {
    if (filename.split(path.sep).includes('target')) continue;
    const testFile = /(?:^|[/\\])(?:tests|scripts)[/\\]/.test(path.relative(crateRoot, filename));
    let inTests = testFile;
    for (const [i, text] of fs.readFileSync(filename, 'utf8').split('\n').entries()) {
      if (/^\s*#\[(?:cfg\(test\)|test)\]/.test(text)) inTests = true;
      if (inTests) records.push({ file: path.relative(repository, filename).split(path.sep).join('/'), line: i + 1, text: text.slice(0, 240), words: new Set(text.match(/[A-Za-z_][A-Za-z_0-9]*/g) ?? []) });
    }
  }
  return ident => records.filter(record => record.words.has(ident)).slice(0, 6).map(({ words, ...record }) => ({ ...record, executed: false }));
}

export function main(args) {
  const values = new Map();
  for (const arg of args) {
    assert.match(arg, /^--(?:crate|census|out|jobs)=.+$/, 'usage: node review-contract-proposals.mjs --crate=PACKAGE --census=/absolute/census.json [--out=/tmp/proposals.json] [--jobs=1|2]');
    const split = arg.indexOf('='); const key = arg.slice(2, split);
    assert.ok(!values.has(key), `duplicate --${key}`); values.set(key, arg.slice(split + 1));
  }
  assert.ok(values.has('crate') && values.has('census'), '--crate and --census are required');
  const jobs = values.get('jobs') ?? '2';
  assert.match(jobs, /^[12]$/, '--jobs must be 1 or 2; follow the selected crate Cargo limits');
  const cargoEnv = { CARGO_BUILD_JOBS: jobs };
  const workspace = fs.realpathSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..'));
  const repository = path.dirname(workspace);
  const output = values.has('out') ? path.resolve(values.get('out')) : undefined;
  if (output) {
    const destination = path.join(fs.realpathSync(path.dirname(output)), path.basename(output));
    const relative = path.relative(repository, destination);
    assert.ok(relative.startsWith(`..${path.sep}`) || relative === '..', 'proposal output must be outside the checkout; contracts are never written by this helper');
    assert.ok(!fs.existsSync(output) || !fs.lstatSync(output).isSymbolicLink(), 'proposal output cannot be a symlink');
  }
  const census = JSON.parse(fs.readFileSync(values.get('census'), 'utf8'));
  const rows = census.rows?.filter(row => row.crate === values.get('crate'));
  assert.ok(rows?.length, `no census rows for ${values.get('crate')}`);
  const metadata = JSON.parse(command('cargo', ['metadata', '--no-deps', '--locked', '--format-version=1'], workspace, cargoEnv));
  const packages = metadata.packages.filter(pkg => pkg.name === values.get('crate') && metadata.workspace_members.includes(pkg.id));
  assert.equal(packages.length, 1, 'one actual workspace compiler package required');
  const pkg = packages[0];
  const references = testIndex(path.dirname(pkg.manifest_path), repository);
  const contracts = loadContracts(path.join(workspace, 'safe-cex-parity/accepted-set/contracts'));
  const construction = loadPolicy('construction', path.join(workspace, 'safe-cex-parity/accepted-set'));
  const targets = [];
  for (const row of rows) {
    const target = pkg.targets.find(target => target.name === row.target && target.kind.some(kind => ['lib', 'cdylib', 'bin'].includes(kind)));
    assert.ok(target, `stale census target ${row.target}`);
    if (row.error) { targets.push({ crate: row.crate, target: row.target, blocked: row.error, proposals: [], receiverRefusals: row.receiverDiagnostics }); continue; }
    command('cargo', ['rustdoc', '--locked', '-p', pkg.name, `--jobs=${jobs}`, ...(target.kind.includes('bin') ? ['--bin', target.name] : ['--lib']), '--', '-Z', 'unstable-options', '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], workspace, { ...cargoEnv, RUSTC_BOOTSTRAP: '1' });
    const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name.replaceAll('-', '_')}.json`), 'utf8'));
    assert.equal(doc.index[String(doc.root)].name, target.name.replaceAll('-', '_'), 'actual compiler target root required');
    targets.push(targetProposals(doc, row, { workspace, repository, testReferences: references, existingClassification: (name, kind) => (kind === 'construction' ? construction : contracts)[name] }));
  }
  const report = {
    format: 'accepted-contract-review-proposals-v1', status: 'needs-human-review', gatePermission: false,
    head: command('git', ['rev-parse', 'HEAD'], repository).trim(), censusHead: census.head,
    compiler: command('rustc', ['-Vv'], workspace).trim(), configuration: 'native/default', cargoJobs: Number(jobs), package: pkg.name, targets,
    instructions: 'Review the TS/tooling input contract and each source body. Author expected outputs independently. Execute actual native witnesses before copying a reviewed classification into its per-crate contracts/<compiler-crate>.json file. This report is not a contract, exemption, runtime receipt or passing gate.',
  };
  const text = JSON.stringify(report, null, 2) + '\n';
  if (output) fs.writeFileSync(output, text, { flag: 'wx' }); else process.stdout.write(text);
  process.stderr.write(`${pkg.name}: ${targets.reduce((n, target) => n + target.proposals.length, 0)} review proposals; ${targets.filter(target => target.blocked).length} blocked targets; no contracts written\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
