// Private data dependency of accounts only; no claim to port the metadata API.
const fs=require('node:fs'),path=require('node:path');const {root,load}=require('./accounts-source.cjs');
const metadata=load(path.join(root,'packages/safe-cex/src/exchangeMetadata.ts'));
const {defaultProxyRegion}=load(path.join(root,'packages/safe-cex/src/transport/tealstreet/transportResolution.ts'));
const fields=['prettyName','requiresNoSecret','requiresPassphrase','requiresApplicationId','requiresApiKeyIndex','requiresOmniKeySeed','requiresParadexAddress','supportsVault','supportsSubaccount','supportsTestnet','enabled','hiddenFromNewAccountPicker','requiresCors'];
const policies={};for(const exchange of [...Object.keys(metadata.exchangeMetadata),'constructor','__proto__']){const source=metadata.getExchangeMetadata(exchange);const policy=Object.fromEntries(fields.filter(f=>source[f]!==undefined).map(f=>[f,source[f]]));policy.defaultProxyRegion=defaultProxyRegion(source);Object.defineProperty(policies,exchange,{value:policy,enumerable:true});}
const generated=JSON.stringify(policies,null,2)+'\n';const target=path.join(__dirname,'../src/account_policy.json');if(process.argv.includes('--check')){if(fs.readFileSync(target,'utf8')!==generated)throw Error('account policy projection drift');}else fs.writeFileSync(target,generated);
