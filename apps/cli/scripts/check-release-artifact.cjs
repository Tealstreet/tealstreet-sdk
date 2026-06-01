#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');

const filePath = process.argv[2];
if (!filePath) {
  console.error('Usage: check-release-artifact.cjs <artifact>');
  process.exit(2);
}

const bytes = fs.readFileSync(filePath);
const strings = [];
let current = '';

for (const byte of bytes) {
  if (byte >= 32 && byte <= 126) {
    current += String.fromCharCode(byte);
    continue;
  }

  if (current.length >= 8) {
    strings.push(current);
  }
  current = '';
}

if (current.length >= 8) {
  strings.push(current);
}

const checks = [
  {
    label: 'CLI source path',
    pattern: /(?:tealstreet-next\/apps\/cli|apps\/cli\/src|\/apps\/cli\/)/i,
  },
  {
    label: 'GitHub token',
    pattern: /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{20,}\b|\bgithub_pat_[A-Za-z0-9_]{20,}\b/,
  },
  {
    label: 'npm token',
    pattern: /\bnpm_[A-Za-z0-9]{20,}\b/,
  },
  {
    label: 'secret assignment',
    pattern: /\b[A-Z0-9_]*(?:SECRET|PASSWORD|PRIVATE_KEY|ACCESS_TOKEN)[A-Z0-9_]*\s*[:=]\s*['"]?[A-Za-z0-9_./+=-]{16,}/,
  },
];

const matches = [];
for (const value of strings) {
  for (const check of checks) {
    if (check.pattern.test(value)) {
      matches.push({ label: check.label, value });
      break;
    }
  }

  if (matches.length >= 50) {
    break;
  }
}

function redact(value) {
  if (value.length <= 12) {
    return '[redacted]';
  }
  return `${value.slice(0, 4)}...${value.slice(-4)} (${value.length} chars)`;
}

if (matches.length > 0) {
  console.error(`${path.basename(filePath)} contains release-blocking strings:`);
  for (const match of matches) {
    console.error(`  [${match.label}] ${redact(match.value)}`);
  }
  if (matches.length >= 50) {
    console.error('  ...showing at most 50 matches');
  }
  process.exit(1);
}
