import { readFileSync } from 'node:fs';
import { cpus } from 'node:os';

const readCpuInfo = () => {
  try { return readFileSync('/proc/cpuinfo', 'utf8'); } catch { return ''; }
};

// This snapshot is outside each child's measured CPU interval.
export function observeCpu({ cpuInfo = readCpuInfo(), logical = cpus() } = {}) {
  const rows = cpuInfo.trim().split(/\n\s*\n/).map((text) => Object.fromEntries(text.split('\n')
    .filter((line) => line.includes(':')).map((line) => {
      const index = line.indexOf(':'); return [line.slice(0, index).trim(), line.slice(index + 1).trim()];
    })));
  const models = [...new Set(rows.map((row) => row['model name']).filter(Boolean))];
  const topology = rows.length > 0 && rows.every((row) => row['physical id'] !== undefined && row['core id'] !== undefined);
  const observed = rows.map((row) => Number(row['cpu MHz'])).filter((n) => Number.isFinite(n) && n > 0);
  const speeds = observed.length ? observed : logical.map((cpu) => cpu.speed).filter((n) => Number.isFinite(n) && n > 0);
  return {
    models: models.length ? models : [...new Set(logical.map((cpu) => cpu.model))],
    logicalCpus: logical.length,
    physicalCores: topology ? new Set(rows.map((row) => `${row['physical id']}:${row['core id']}`)).size : null,
    sockets: topology ? new Set(rows.map((row) => row['physical id'])).size : null,
    frequencyMHz: speeds.length ? { source: observed.length ? 'proc-cpuinfo' : 'os-nominal', min: Math.min(...speeds), max: Math.max(...speeds) } : null,
  };
}
