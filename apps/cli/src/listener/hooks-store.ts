import { existsSync, mkdirSync, readFileSync, writeFileSync, renameSync, unlinkSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import crypto from 'crypto';
import { BRAND } from '../brand';

const DIR = join(homedir(), BRAND.configDirName);
const HOOKS_FILE = join(DIR, 'hooks.json');

const ID_BYTES = 16; // 32 hex chars
const SECRET_BYTES = 16; // 32 hex chars
const PREFIX_MIN = 8;

export interface Hook {
  id: string;
  name: string;
  secret: string;
  command: string;
  account?: string;
  createdAt: number;
}

interface HooksFile {
  hooks: Hook[];
}

function ensureDir(): void {
  if (!existsSync(DIR)) mkdirSync(DIR, { recursive: true, mode: 0o700 });
}

// hooks.json holds webhook shared secrets; the rename keeps the temp file's 0600.
function atomicWrite(path: string, contents: string): void {
  ensureDir();
  const tmp = `${path}.tmp.${process.pid}`;
  writeFileSync(tmp, contents, { mode: 0o600 });
  try {
    renameSync(tmp, path);
  } catch (err) {
    try {
      unlinkSync(tmp);
    } catch {
      /* ignore */
    }
    throw err;
  }
}

export function getHooksFilePath(): string {
  return HOOKS_FILE;
}

export function loadHooks(): Hook[] {
  if (!existsSync(HOOKS_FILE)) return [];
  try {
    const parsed = JSON.parse(readFileSync(HOOKS_FILE, 'utf-8')) as HooksFile;
    if (!Array.isArray(parsed.hooks)) return [];
    return parsed.hooks;
  } catch {
    return [];
  }
}

function saveHooks(hooks: Hook[]): void {
  atomicWrite(HOOKS_FILE, JSON.stringify({ hooks }, null, 2));
}

export function createHook(input: { name: string; command: string; account?: string }): { hook: Hook; secret: string } {
  if (!input.name || typeof input.name !== 'string') {
    throw new Error('hook name required');
  }
  if (!input.command || typeof input.command !== 'string') {
    throw new Error('hook command required');
  }
  const id = crypto.randomBytes(ID_BYTES).toString('hex');
  const secret = crypto.randomBytes(SECRET_BYTES).toString('hex');
  const hook: Hook = {
    id,
    name: input.name.slice(0, 80),
    secret,
    command: input.command,
    account: input.account,
    createdAt: Date.now(),
  };
  const hooks = loadHooks();
  hooks.push(hook);
  saveHooks(hooks);
  return { hook, secret };
}

export function rotateHookSecret(target: string): { hook: Hook; secret: string } | { error: string; matches?: Hook[] } {
  const found = findHook(target);
  if ('error' in found) return found;
  const hooks = loadHooks();
  const idx = hooks.findIndex((h) => h.id === found.hook.id);
  if (idx < 0) return { error: 'hook not found' };
  const secret = crypto.randomBytes(SECRET_BYTES).toString('hex');
  hooks[idx] = { ...hooks[idx], secret };
  saveHooks(hooks);
  return { hook: hooks[idx], secret };
}

export function revokeHook(target: string): { removed: Hook[]; matches?: Hook[] } {
  const hooks = loadHooks();
  let matches = hooks.filter((h) => h.id === target);
  if (matches.length === 0 && target.length >= PREFIX_MIN) {
    matches = hooks.filter((h) => h.id.startsWith(target));
  }
  if (matches.length !== 1) {
    return { removed: [], matches };
  }
  const remaining = hooks.filter((h) => h.id !== matches[0].id);
  saveHooks(remaining);
  return { removed: matches };
}

export function revokeAllHooks(): number {
  const n = loadHooks().length;
  saveHooks([]);
  return n;
}

export function findHook(target: string): { hook: Hook } | { error: string; matches?: Hook[] } {
  const hooks = loadHooks();
  let matches = hooks.filter((h) => h.id === target);
  if (matches.length === 0 && target.length >= PREFIX_MIN) {
    matches = hooks.filter((h) => h.id.startsWith(target));
  }
  if (matches.length === 1) return { hook: matches[0] };
  if (matches.length > 1) return { error: 'ambiguous prefix', matches };
  return { error: 'no match' };
}

export function verifyHookToken(
  hookId: string,
  token: string,
): { ok: true; hook: Hook } | { ok: false; reason: 'unknown' | 'invalid' } {
  const hooks = loadHooks();
  const hook = hooks.find((h) => h.id === hookId);
  if (!hook) return { ok: false, reason: 'unknown' };
  // Wrap timingSafeEqual: it throws RangeError on unequal-length buffers.
  if (token.length !== hook.secret.length) return { ok: false, reason: 'invalid' };
  const a = Buffer.from(token);
  const b = Buffer.from(hook.secret);
  if (!crypto.timingSafeEqual(a, b)) return { ok: false, reason: 'invalid' };
  return { ok: true, hook };
}
