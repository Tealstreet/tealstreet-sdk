'use strict';
const fs=require('node:fs'),assert=require('node:assert/strict'),read=fs.readFileSync;
fs.readFileSync=function(file,...args){const source=read.call(this,file,...args);if(!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts'))return source;const original='const baseOrderId = (BigInt(Date.now()) * 1000n + BigInt(Math.floor(Math.random() * 1000))) % maxClientOrderIndex;';assert.equal(source.split(original).length,2,'unique actual batch base-ID site');return source.replace(original,original.replace('* 1000n','* 1001n'));};
require('./batch-placement-oracle.cjs');
