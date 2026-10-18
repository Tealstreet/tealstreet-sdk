import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import { parse, stringify } from 'smol-toml';
import { kinds, manifestPathForSource, sourceForManifestPath, walkFiles, type SourceSymbols, type SymbolIdentity } from './extract.ts';

export interface SymbolRow extends SymbolIdentity {
  layer: '' | 'mirror' | 'adapted';
  rs: string | string[];
  status: 'pending' | 'ported' | 'divergent' | 'not-applicable';
  note: string;
}
export interface Manifest {
  file: { ts: string; rust_files?: string[] };
  symbol: SymbolRow[];
}

const key = (row: SymbolIdentity) => `${row.kind}:${row.ts}`;
const rowFields = ['ts', 'kind', 'layer', 'rs', 'status', 'note'];

function table(value: unknown, fields: string[], context: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${context}: expected table`);
  const record = value as Record<string, unknown>;
  for (const field of Object.keys(record)) {
    if (!fields.includes(field)) throw new Error(`${context}: unknown field ${field}`);
  }
  return record;
}

export function parseManifest(text: string, expectedSource: string, manifestPath = expectedSource): Manifest {
  let parsed: unknown;
  try {
    parsed = parse(text);
  } catch (error) {
    throw new Error(`${manifestPath}: ${error instanceof Error ? error.message : String(error)}`, { cause: error });
  }
  const root = table(parsed, ['file', 'symbol'], expectedSource);
  const file = table(root.file, ['ts', 'rust_files'], expectedSource);
  if (file.ts !== expectedSource) throw new Error(`${expectedSource}: [file].ts does not match manifest path`);
  if (file.rust_files !== undefined && !(Array.isArray(file.rust_files) &&
    file.rust_files.every((item) => typeof item === 'string' && !item.startsWith('/') &&
      !/[*?\[\]:\\]/.test(item) && item.endsWith('.rs') && item.includes('/') &&
      item.split('/').every((part) => part !== '' && part !== '.' && part !== '..')) &&
    new Set(file.rust_files).size === file.rust_files.length)) {
    throw new Error(`${expectedSource}: invalid or duplicate [file].rust_files`);
  }
  const symbols = root.symbol ?? [];
  if (!Array.isArray(symbols)) throw new Error(`${expectedSource}: expected [[symbol]] rows`);
  const seen = new Set<string>();
  const rows = symbols.map((value): SymbolRow => {
    const row = table(value, rowFields, expectedSource);
    for (const field of rowFields) {
      if (field !== 'rs' && typeof row[field] !== 'string') throw new Error(`${expectedSource}: ${String(row.ts)} missing string ${field}`);
    }
    // Mapping splits are ordered agent metadata; validate members without coercing or sorting.
    if (typeof row.rs !== 'string' && !(Array.isArray(row.rs) && row.rs.every((item) => typeof item === 'string'))) {
      throw new Error(`${expectedSource}: ${String(row.ts)} rs must be a string or an array of strings`);
    }
    if (!row.ts || !kinds.includes(row.kind as SymbolRow['kind']) ||
      !['', 'mirror', 'adapted'].includes(row.layer as string) ||
      !['pending', 'ported', 'divergent', 'not-applicable'].includes(row.status as string)) {
      throw new Error(`${expectedSource}: invalid symbol row ${String(row.ts)}`);
    }
    const identity = key(row as unknown as SymbolRow);
    if (seen.has(identity)) throw new Error(`${expectedSource}: duplicate symbol ${identity}`);
    seen.add(identity);
    return {
      ts: row.ts as string,
      kind: row.kind as SymbolRow['kind'],
      layer: row.layer as SymbolRow['layer'],
      rs: Array.isArray(row.rs) ? [...row.rs] : row.rs as string,
      status: row.status as SymbolRow['status'],
      note: row.note as string,
    };
  });
  return { file: { ts: expectedSource, ...(file.rust_files !== undefined ? { rust_files: [...(file.rust_files as string[])] } : {}) }, symbol: rows };
}

export function serializeManifest(manifest: Manifest): string {
  const symbol = manifest.symbol.map((row) => ({
    ts: row.ts, kind: row.kind, layer: row.layer, rs: row.rs, status: row.status, note: row.note,
  }));
  return stringify({ file: manifest.file, ...(symbol.length ? { symbol } : {}) }).trimEnd() + '\n';
}

export interface ReconcileResult {
  added: string[];
  orphaned: string[];
  changed: string[];
}

export function reconcile(sources: SourceSymbols[], manifestRoot: string, check: boolean): ReconcileResult {
  const existing = new Map<string, { text: string; manifest: Manifest; path: string }>();
  // Validate the whole corpus before the first write: malformed metadata must never be replaced.
  for (const path of existsSync(manifestRoot) ? walkFiles(manifestRoot) : []) {
    if (!path.endsWith('.toml')) continue;
    const source = sourceForManifestPath(relative(manifestRoot, path).split(sep).join('/'));
    const text = readFileSync(path, 'utf8');
    existing.set(source, { text, manifest: parseManifest(text, source, path), path });
  }
  const result: ReconcileResult = { added: [], orphaned: [], changed: [] };
  const writes: { path: string; text: string }[] = [];
  for (const source of sources) {
    const manifestPath = manifestPathForSource(source.ts);
    const previous = existing.get(source.ts);
    const oldRows = new Map(previous?.manifest.symbol.map((row) => [key(row), row]));
    const rows = source.symbols.map((symbol): SymbolRow => {
      const match = oldRows.get(key(symbol));
      oldRows.delete(key(symbol));
      if (match) return match;
      result.added.push(`${source.ts}::${symbol.ts} (${symbol.kind})`);
      return { ...symbol, layer: '', rs: '', status: 'pending', note: '' };
    });
    for (const row of oldRows.values()) {
      result.orphaned.push(`${source.ts}::${row.ts} (${row.kind})`);
      rows.push(row);
    }
    const text = serializeManifest({ file: previous?.manifest.file ?? { ts: source.ts }, symbol: rows });
    const path = resolve(manifestRoot, manifestPath);
    if (text !== previous?.text) {
      writes.push({ path, text });
      result.changed.push(source.ts);
    }
    existing.delete(source.ts);
  }
  for (const [source, { manifest }] of existing) {
    for (const row of manifest.symbol) result.orphaned.push(`${source}::${row.ts} (${row.kind}; source file vanished)`);
    if (!manifest.symbol.length) result.orphaned.push(`${source} (source file vanished)`);
  }
  if (!check) {
    for (const { path, text } of writes) {
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, text);
    }
  }
  return result;
}
