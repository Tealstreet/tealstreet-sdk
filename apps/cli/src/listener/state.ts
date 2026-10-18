import { existsSync, mkdirSync, readFileSync, writeFileSync, renameSync, unlinkSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import crypto from 'crypto';
import { BRAND } from '../brand';

const DIR = join(homedir(), BRAND.configDirName);
const LISTENER_FILE = join(DIR, 'listener.json');
const TRUSTED_FILE = join(DIR, 'trusted-clients.json');

const CLIENT_ID_MAX = 64;

export interface TrustedClient {
  clientId: string;
  label: string;
  origin: string;
  approvedAt: number;
}

interface TrustedFile {
  clients: TrustedClient[];
}

interface ListenerFile {
  listenerId: string;
  createdAt: number;
}

let cachedListenerId: string | null = null;

function ensureDir(): void {
  if (!existsSync(DIR)) mkdirSync(DIR, { recursive: true });
}

function atomicWrite(path: string, contents: string): void {
  ensureDir();
  const tmp = `${path}.tmp.${process.pid}`;
  writeFileSync(tmp, contents);
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

export function getListenerId(): string {
  if (cachedListenerId) return cachedListenerId;
  if (existsSync(LISTENER_FILE)) {
    try {
      const parsed = JSON.parse(readFileSync(LISTENER_FILE, 'utf-8')) as ListenerFile;
      if (typeof parsed.listenerId === 'string' && parsed.listenerId.length > 0) {
        cachedListenerId = parsed.listenerId;
        return cachedListenerId;
      }
    } catch {
      /* fall through, regenerate */
    }
  }
  const id = crypto.randomUUID();
  atomicWrite(LISTENER_FILE, JSON.stringify({ listenerId: id, createdAt: Date.now() }, null, 2));
  cachedListenerId = id;
  return id;
}

export function loadTrustedClients(): TrustedFile {
  if (!existsSync(TRUSTED_FILE)) return { clients: [] };
  try {
    const parsed = JSON.parse(readFileSync(TRUSTED_FILE, 'utf-8')) as TrustedFile;
    if (!Array.isArray(parsed.clients)) return { clients: [] };
    return parsed;
  } catch {
    return { clients: [] };
  }
}

function saveTrustedClients(state: TrustedFile): void {
  atomicWrite(TRUSTED_FILE, JSON.stringify(state, null, 2));
}

function isValidClientId(id: unknown): id is string {
  return typeof id === 'string' && id.length > 0 && id.length <= CLIENT_ID_MAX;
}

export function isTrusted(clientId: string): TrustedClient | undefined {
  return loadTrustedClients().clients.find((c) => c.clientId === clientId);
}

export function addTrustedClient(entry: { clientId: string; label: string; origin: string }): void {
  if (!isValidClientId(entry.clientId)) {
    throw new Error(`invalid clientId (must be non-empty string ≤${CLIENT_ID_MAX} chars)`);
  }
  const state = loadTrustedClients();
  const existing = state.clients.findIndex((c) => c.clientId === entry.clientId);
  const record: TrustedClient = {
    clientId: entry.clientId,
    label: entry.label.slice(0, 200),
    origin: entry.origin,
    approvedAt: Date.now(),
  };
  if (existing >= 0) {
    state.clients[existing] = record;
  } else {
    state.clients.push(record);
  }
  saveTrustedClients(state);
}

export function removeTrustedClient(target: string): { removed: TrustedClient[]; matches?: TrustedClient[] } {
  const state = loadTrustedClients();
  let matches = state.clients.filter((c) => c.clientId === target);
  if (matches.length === 0 && target.length >= 8) {
    matches = state.clients.filter((c) => c.clientId.startsWith(target));
  }
  if (matches.length !== 1) {
    return { removed: [], matches };
  }
  state.clients = state.clients.filter((c) => c.clientId !== matches[0].clientId);
  saveTrustedClients(state);
  return { removed: matches };
}

export function removeAllTrustedClients(): number {
  const state = loadTrustedClients();
  const n = state.clients.length;
  state.clients = [];
  saveTrustedClients(state);
  return n;
}
