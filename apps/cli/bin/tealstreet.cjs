#!/usr/bin/env node
'use strict';

const { spawnSync } = require('node:child_process');
const { ensureBinary } = require('../scripts/install-binary.cjs');

const binary = ensureBinary({ optional: false });
const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (result.signal) {
  process.kill(process.pid, result.signal);
}

process.exit(result.status ?? 0);
