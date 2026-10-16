const assert = require('node:assert/strict');
const {load, root} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const path = require('node:path');
const ws = load(path.join(root, 'packages/safe-cex/src/utils/wsDiagnostics.ts'));
const rest = load(path.join(root, 'packages/safe-cex/src/utils/venueRestDiagnostics.ts'));
const redacted = JSON.parse(ws.redactWsDiagnosticPayload(JSON.stringify({op:'login', args:['credential',{ordinary:'visible',params:['private']}], 'API-Key':'sensitive', market:'BTC'})));
assert.equal(redacted.args[0], '[redacted]'); assert.equal(redacted.args[1].ordinary, 'visible'); assert.equal(redacted.args[1].params[0], '[redacted]');
assert.equal(redacted['API-Key'],'[redacted]'); assert.equal(redacted.market,'BTC');
const config = {url:'/wallet-balance?signature=private',baseURL:'https://proxy/https://venue?private',headers:{get: name => name === 'x-bapi-timestamp' ? '900' : undefined}};
assert.equal(rest.markRestRequestStart(config,1000),config); rest.markRestSigned(config,17,1100); rest.markRestProxyHeader(config,'present');
assert.equal(config.safeCexRestDiagSignMs,17); assert.equal(config.safeCexRestDiagSignatureAgeMs,200);
config.headers={Authorization:'Bearer private'};
const response={config,status:403,data:'<html>CloudFront denied ABC123456789xyz</html>',headers:{'X-BAPI-LIMIT-STATUS':'12',server:'edge',Authorization:'do not expose'}};
rest.noteRestSuccess('bybit',response,1120); response.headers['X-BAPI-LIMIT-STATUS']='7'; rest.noteRestSuccess('bybit',response,1140);
rest.noteRestFailure('bybit',{config,response,code:'ERR_BAD_RESPONSE',message:'Request failed 403',name:'AxiosError'},1200);
const stats=rest.getVenueRestStats(1200).bybit, sample=rest.drainVenueRestFailureSamples()[0];
assert.equal(stats.requests,3); assert.equal(stats.totalP50Ms,140); assert.equal(stats.totalP95Ms,200); assert.equal(stats.minQuotaRemaining,7);
assert.equal(sample.restProxyHeaderSent,'bearer'); assert.equal(sample.bodyKind,'cloudfront'); assert.equal(sample.bodySnippet,'<html>CloudFront denied <redacted></html>'); assert.equal(sample.path,'/wallet-balance'); assert.deepEqual(Object.keys(sample.headers),['server','x-bapi-limit-status']);
for (const arrivals of [['0','-0'],['-0','0']]) {
 rest.resetVenueRestDiagnostics();for(const value of arrivals)rest.noteRestSuccess('zero',{headers:{'x-bapi-limit-status':value}},1);
 assert.ok(Object.is(rest.getVenueRestStats(1).zero.minQuotaRemaining,-0),'REST native quota Math.min signed zero');
 for(const age of [-0,0]) {rest.noteRestFailure('zero',{config:{safeCexRestDiagSignatureAgeMs:age}},1);assert.ok(Object.is(rest.getVenueRestStats(1).zero.maxSignatureAgeMs,0),'REST native age Math.max signed zero');}
}
console.log(JSON.stringify({redacted,stats,sample,stoppedAfterCases:2}));
