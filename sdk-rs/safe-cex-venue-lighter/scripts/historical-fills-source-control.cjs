'use strict';
const fs=require('node:fs'),assert=require('node:assert/strict'),read=fs.readFileSync;
fs.readFileSync=function(file,...args){const source=read.call(this,file,...args);if(!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts'))return source;const original='const maxLimit = Math.min(Math.max(limit, 1), 200);';assert.equal(source.split(original).length,2,'unique actual execution limit site');return source.replace(original,original.replace(', 200)',', 199)'));};
require('./historical-fills-oracle.cjs');
