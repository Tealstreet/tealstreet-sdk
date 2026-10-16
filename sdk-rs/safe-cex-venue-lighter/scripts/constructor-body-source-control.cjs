'use strict';
const fs=require('node:fs'),assert=require('node:assert/strict'),read=fs.readFileSync;
fs.readFileSync=function(file,...args){const source=read.call(this,file,...args);if(!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts'))return source;const old='this.candlestickApi = new CandlestickApi(this.lighterApiClient);';assert.equal(source.split(old).length,2,'unique source candle constructor site');return source.replace(old,'this.candlestickApi = new OrderApi(this.lighterApiClient);');};
require('./constructor-body-oracle.cjs');
