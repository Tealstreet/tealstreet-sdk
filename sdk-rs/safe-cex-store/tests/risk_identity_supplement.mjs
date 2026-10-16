// Later identity supplement; original derived oracle bytes are unchanged.
import fs from 'node:fs';
const original = new URL('./derived_oracle.mjs', import.meta.url);
const bytes = fs.readFileSync(original, 'utf8');
const boundary = bytes.indexOf('const scenarios=[');
if (boundary < 0) throw Error('RISK-IDENTITY source harness boundary');
const prefix = bytes.slice(0, boundary).replaceAll('import.meta.url', JSON.stringify(original.href));
const added = `
const input={markets:[market],tickers:[ticker],positions:[p('long',10)],orders:[o('first-identity',110,2),o('second-identity',120,3)]};
const risk=context.api.calculateOrdersRisk(input.markets,input.tickers,input.positions,input.orders);
const profiles=Object.values(risk).flat();
if (!profiles.some(row=>new Set([row.orderPnl,row.totalRealizedPnl,row.rollingUnrealizedPnl,row.positionSize]).size===4)) throw Error('RISK-IDENTITY four distinct native dimensions');
process.stdout.write(JSON.stringify({label:'later-field-identity-supplement',architecture:process.platform+'-'+process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),input,risk}));
`;
await import('data:text/javascript;base64,' + Buffer.from(prefix + added).toString('base64'));
