const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const target = process.argv[2];
assert(target && path.isAbsolute(target), 'provide an absolute isolated oracle dependency directory');
const manifest = JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8'));
const venue = JSON.parse(fs.readFileSync(path.join(root,'packages/safe-cex/package.json'),'utf8'));
const lock = fs.readFileSync(path.join(root,'yarn.lock'),'utf8');
// Stark SDK is an existing lock selector, independently fingerprinted by the fixtures.
assert.match(lock, /^starknet@\^7\.6\.4:$/m, 'locked Stark SDK selector missing');
fs.mkdirSync(target,{recursive:true});
fs.writeFileSync(path.join(target,'package.json'),JSON.stringify({private:true,packageManager:manifest.packageManager,dependencies:{ethers:venue.dependencies.ethers,'@msgpack/msgpack':venue.dependencies['@msgpack/msgpack'],'zklink-web-sdk-alpha':venue.dependencies['zklink-web-sdk-alpha'],starknet:'^7.6.4'}},null,2)+'\n');
fs.copyFileSync(path.join(root,'yarn.lock'),path.join(target,'yarn.lock'));
