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
    pattern: /\b[A-Z0-9_]*(?:SECRET|PASSWORD|PRIVATE_KEY|ACCESS_TOKEN)[A-Z0-9_]*\s*[:=]\s*['"]?([A-Za-z0-9_./+=-]{16,})/,
    // A CONSTANT assigned a dotted lowercase attribute key is a semantic
    // convention, not a credential — OpenTelemetry (bundled via @sentry/bun)
    // ships hundreds, e.g. AWS_SECRETSMANAGER_SECRET_ARN =
    // "aws.secretsmanager.secret.arn". Requiring a dot-separated all-lowercase
    // identifier keeps this narrow: a real token has mixed case or no dots.
    ignoreValue: /^[a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+$/,
  },
];

// Scan EVERY occurrence in a string, not just the first. These strings are
// whole minified chunks — one benign hit used to `break` the check loop and
// hide every later match in the same 233KB blob, including a real secret.
const matches = [];
outer: for (const value of strings) {
  for (const check of checks) {
    const scanner = new RegExp(check.pattern.source, `${check.pattern.flags.replace('g', '')}g`);
    let found;
    while ((found = scanner.exec(value)) !== null) {
      if (found[0] === '') {
        scanner.lastIndex += 1;
        continue;
      }
      if (check.ignoreValue && check.ignoreValue.test(found[1] ?? '')) {
        continue;
      }
      matches.push({ label: check.label, value: found[0] });
      if (matches.length >= 50) {
        break outer;
      }
    }
  }
}

// Report the MATCH, not the enclosing chunk. Redacting a whole minified blob
// printed `at ...ent. (233360 chars)`, which named neither the offender nor
// the reason.
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
