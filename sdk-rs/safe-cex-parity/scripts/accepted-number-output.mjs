import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const sdk = fs.realpathSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..'));
const definition = 'safe-cex-num/src/numeric_ingress/eligibility.rs';
// Reviewed at 59eb80867e: four arms destructure Self and call the output or
// passthrough trait per field, with an enum match. No declaration expansion.
// This pins canonical recipe tokens, not an unrelated whole-file fingerprint.
const recipeSha256 = '89cfa3fb6554bcf459b71aa9dbee6c4b73175c40df3ba352a2c5f94f82ea9731';
const prefix = 'accepted-set: NUMBER_OUTPUT_PROVIDER assertion';
function real(file) {
  if (typeof file !== 'string') return undefined;
  try { return fs.realpathSync(file); }
  catch (error) { if (error.code === 'ENOENT') return undefined; throw error; }
}

// This capability is produced only after the real consumer's compiler binding
// is known. The pre-doc scanner performs rejection-only argument inspection.
// Same-named packages, renamed bindings and provider self-use gain no permission.
export function verifiedNumberOutputOrigin(doc, consumer, metadata, artifacts, scanner) {
  const binding = metadata.resolve?.nodes.find(node => node.id === consumer.id)?.deps
    .filter(dep => dep.name === 'safe_cex_num') ?? [];
  if (!binding.length) return [];
  assert.equal(binding.length, 1, `${prefix}: unique dependency binding required`);
  assert.ok(['2018', '2021', '2024'].includes(consumer.edition), `${prefix}: supported consumer edition required`);
  const providers = metadata.packages.filter(pkg => pkg.id === binding[0].pkg
    && pkg.name === 'safe-cex-num' && pkg.source == null
    && real(pkg.manifest_path) === real(path.join(sdk, 'safe-cex-num/Cargo.toml')));
  assert.equal(providers.length, 1, `${prefix}: canonical SDK package required`);
  const provider = providers[0];
  assert.ok(metadata.workspace_members.includes(provider.id), `${prefix}: SDK workspace member required`);
  const targets = provider.targets.filter(target => target.kind.includes('lib')
    && real(target.src_path) === real(path.join(sdk, 'safe-cex-num/src/lib.rs')));
  assert.equal(targets.length, 1, `${prefix}: canonical provider library required`);
  const selected = Object.values(doc.external_crates ?? {}).filter(crate => crate.name === 'safe_cex_num')
    .filter(crate => {
      const compilerPath = real(crate.path);
      if (!compilerPath) return false;
      const matches = artifacts.filter(artifact => artifact.filenames?.some(file => real(file) === compilerPath));
      return matches.length === 1 && matches[0].package_id === provider.id
        && matches[0].profile?.test === false && matches[0].target?.kind.includes('lib')
        && matches[0].target?.name === targets[0].name
        && real(matches[0].target?.src_path) === real(targets[0].src_path);
    });
  assert.equal(selected.length, 1, `${prefix}: actual consumer compiler artifact required`);
  const checked = spawnSync(scanner, ['--number-output-provider-json', sdk], { encoding: 'utf8' });
  assert.equal(checked.status, 0, `${prefix}: unconditional source route required\n${checked.error ?? ''}\n${checked.stderr}`);
  const recipe = JSON.parse(checked.stdout);
  assert.equal(recipe.recipeFormat, 'proc-macro2-token-string-v1', `${prefix}: canonical token format required`);
  assert.equal(recipe.definition, definition, `${prefix}: exact definition required`);
  assert.deepEqual(recipe.moduleChain, ['safe-cex-num/src/lib.rs', 'safe-cex-num/src/numeric_ingress.rs', definition], `${prefix}: independent source route required`);
  assert.equal(createHash('sha256').update(recipe.tokens).digest('hex'), recipeSha256, `${prefix}: recipe changed; expression-macro review required`);
  return ['safe_cex_num::js_number_output'];
}

// Provider definition coverage is distinct from consumer expression permission:
// this binds one declaration and a distinct provider-only self-call capability.
export function verifiedNumberOutputDefinition(doc, provider, metadata, artifacts, scanner) {
  if (provider.name !== 'safe-cex-num') return null;
  assert.ok(provider.source == null
    && real(provider.manifest_path) === real(path.join(sdk, 'safe-cex-num/Cargo.toml'))
    && metadata.workspace_members.includes(provider.id), `${prefix}: canonical definition provider required`);
  const targets = provider.targets.filter(target => target.kind.includes('lib')
    && target.name === 'safe_cex_num'
    && real(target.src_path) === real(path.join(sdk, 'safe-cex-num/src/lib.rs')));
  assert.equal(targets.length, 1, `${prefix}: canonical definition library required`);
  const root = doc.index?.[String(doc.root)];
  assert.ok(root?.crate_id === 0 && root.name === 'safe_cex_num' && root.inner?.module
    && real(path.resolve(sdk, root.span?.filename ?? '')) === real(targets[0].src_path),
    `${prefix}: actual definition compiler owner required`);
  const selected = artifacts.filter(artifact => artifact.package_id === provider.id
    && artifact.profile?.test === false && artifact.target?.kind.includes('lib')
    && artifact.target?.name === targets[0].name
    && real(artifact.target?.src_path) === real(targets[0].src_path));
  assert.equal(selected.length, 1, `${prefix}: actual definition compiler artifact required`);
  const checked = spawnSync(scanner, ['--number-output-provider-json', sdk], { encoding: 'utf8' });
  assert.equal(checked.status, 0, `${prefix}: unconditional definition source route required\n${checked.error ?? ''}\n${checked.stderr}`);
  const recipe = JSON.parse(checked.stdout);
  assert.equal(recipe.recipeFormat, 'proc-macro2-token-string-v1', `${prefix}: canonical definition token format required`);
  assert.equal(recipe.definition, definition, `${prefix}: exact definition required`);
  assert.deepEqual(recipe.moduleChain, ['safe-cex-num/src/lib.rs', 'safe-cex-num/src/numeric_ingress.rs', definition], `${prefix}: independent definition source route required`);
  assert.equal(createHash('sha256').update(recipe.tokens).digest('hex'), recipeSha256, `${prefix}: recipe changed; expression-macro review required`);
  const file = real(path.join(sdk, definition));
  const scanned = spawnSync(scanner, ['--macro-origins-json', '[]', file], { encoding: 'utf8' });
  assert.equal(scanned.status, 0, `${prefix}: definition scope scan required\n${scanned.stderr}`);
  const ident = 'UNSUPPORTED_MODULE_MACRO:js_number_output';
  const matches = scanned.stdout.trim().split('\n').filter(Boolean).map(line => line.split('\t'))
    .filter(([source, name]) => real(source) === file && name === ident);
  assert.equal(matches.length, 1, `${prefix}: unique reviewed macro definition required`);
  const attribute = recipe.exportAttribute;
  assert.ok(attribute?.ident === 'UNSUPPORTED_ATTRIBUTE_EXPANSION:macro_export'
    && Number.isSafeInteger(attribute.line) && attribute.line > 0
    && Number.isSafeInteger(attribute.column) && attribute.column >= 0,
    `${prefix}: exact parsed export attribute required`);
  assert.equal(scanned.stdout.trim().split('\n').filter(Boolean).map(line => line.split('\t'))
    .filter(([source, name, row, column]) => real(source) === file && name === attribute.ident
      && Number(row) === attribute.line && Number(column) === attribute.column).length,
  1, `${prefix}: unique parsed export attribute marker required`);
  return { file, ident, line: Number(matches[0][2]), column: Number(matches[0][3]), exportAttribute: attribute, selfOrigin: 'safe_cex_num::js_number_output::self' };
}

// These two scanner events belong to one verified canonical definition. This
// does not grant attribute permission elsewhere, including in the same file.
export function numberOutputDefinitionCovers(definition, file, ident, row, column) {
  if (!definition || real(file) !== definition.file) return false;
  return [definition, definition.exportAttribute].some(marker => ident === marker.ident
    && Number(row) === marker.line && Number(column) === marker.column);
}
