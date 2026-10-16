import assert from 'node:assert/strict';
import {loadSource} from './source-loader.mjs';
const {isPublicEndpoint} = loadSource(new URL('../../../packages/safe-cex/src/exchanges/okx/okx.api.ts',import.meta.url),['isPublicEndpoint'],['isPublicEndpoint']);
const urls = [undefined,'','/api/v5/public/instruments','/api/v5/market/tickers','/api/v5/account/positions','/api/v5/publicity/orders','/PUBLIC/instruments','public','/public','/market','/public/','/market/','https://host/path?x=/public/','\uFEFF/api/v5/public/instruments'];
const cases = urls.map(url=>({url,expected:isPublicEndpoint(url)}));
assert.equal(cases.length,14);
process.stdout.write(JSON.stringify(cases));
