'use strict';
const fs=require('node:fs'),assert=require('node:assert/strict'),read=fs.readFileSync;
fs.readFileSync=function(file,...args){const source=read.call(this,file,...args);if(!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts'))return source;const old='const orderId = (BigInt(Date.now()) * 100n + BigInt(Math.floor(Math.random() * 100))) % maxClientOrderIndex;';assert.equal(source.split(old).length,2,'unique single placement clock ID site');return source.replace(old,old.replace('* 100n','* 101n'));};
require('./single-placement-oracle.cjs');
