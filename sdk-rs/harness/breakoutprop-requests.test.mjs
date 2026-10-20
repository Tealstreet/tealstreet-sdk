import test from 'node:test';
import assert from 'node:assert/strict';
import { breakoutprop, BREAKOUT_ACCOUNT, BREAKOUT_REPLAY_TOKEN, verifyBreakoutRequest, sanitizeBreakoutRecord } from './breakoutprop-requests.mjs';
import { createRecordingPolicy } from './polling.mjs';
const ROOT = 'https://breakout.prosp.devexperts.com/dxsca-web';
const auth = { Authorization: `DXAPI ${BREAKOUT_REPLAY_TOKEN}` };
test('DXtrade login checks credentials and exact basic body', () => {
  const req = { method:'POST',url:`${ROOT}/login`,body:JSON.stringify({ domain:'default',username:'replay-key',password:'replay-secret' }) };
  verifyBreakoutRequest(req);
  for (const body of [{ domain:'default',username:'replay-key',password:'wrong' }, {domain:'default',username:'replay-key',password:'replay-secret',unused:true}]) assert.throws(()=>verifyBreakoutRequest({...req,body:JSON.stringify(body)}));
});
test('all market data retains the exact selected account and session', () => {
  const req = { method:'POST',url:`${ROOT}/marketdata`,headers:auth,body:JSON.stringify({ account:BREAKOUT_ACCOUNT,eventTypes:[{type:'Quote'}],symbols:['ETHUSD'] }) };
  verifyBreakoutRequest(req);
  assert.throws(()=>verifyBreakoutRequest({...req,headers:{Authorization:'DXAPI invalid'}}));
  assert.throws(()=>verifyBreakoutRequest({...req,body:req.body.replace(BREAKOUT_ACCOUNT,'default:12')}));
});
test('conditional cancellation header and account path are mandatory', () => {
  const req = { method:'DELETE',url:`${ROOT}/accounts/default%3A00002/orders/native%3A42`,headers:{...auth,'If-Match':'*'} };
  verifyBreakoutRequest(req);
  assert.throws(()=>verifyBreakoutRequest({...req,headers:auth}));
  assert.throws(()=>verifyBreakoutRequest({...req,url:req.url.replace('00002','12')}));
});
test('single-order economics, type and price-field exclusivity are validated', () => {
  const body={orderCode:'tealstreet-documented',type:'LIMIT',instrument:'ETHUSD',quantity:0.01,side:'BUY',limitPrice:3656.02,positionEffect:'OPEN',tif:'GTC'};
  const send=(body)=>verifyBreakoutRequest({method:'POST',url:`${ROOT}/accounts/default%3A00002/orders`,headers:auth,body:JSON.stringify(body)});
  send(body);
  for (const patch of [{quantity:0},{stopPrice:3600},{type:'TRAILING_STOP'},{positionEffect:'REDUCE'},{side:'LONG'}]) assert.throws(()=>send({...body,...patch}));
});
test('recording anonymizes escaped credentials, session and each account without changing economics', () => {
  const options={key:'live"username',secret:'live\\password"secret',subaccount:'2'};
  const policy=createRecordingPolicy(breakoutprop,options,true);
  const login=policy.sanitize({type:'http',url:`${ROOT}/login`,requestBody:JSON.stringify({domain:'default',username:options.key,password:options.secret}),body:JSON.stringify({sessionToken:'live-session-token',timeout:'00:30:00'})});
  assert.deepEqual(JSON.parse(login.requestBody),{domain:'default',username:'replay-key',password:'replay-secret'});
  assert.equal(JSON.parse(login.body).sessionToken,BREAKOUT_REPLAY_TOKEN);
  const users=policy.sanitize({type:'http',url:`${ROOT}/users/${encodeURIComponent(options.key)}`,body:JSON.stringify({userDetails:[{login:options.key,domain:'default',accounts:[{account:'default:12',accountStatus:'FULL_TRADING'},{account:'default:2',accountStatus:'FULL_TRADING'}]}]})});
  const rows=JSON.parse(users.body).userDetails[0].accounts;
  assert.equal(rows[1].account,BREAKOUT_ACCOUNT);
  assert.notEqual(rows[0].account,rows[1].account);
  const quote=policy.sanitize({type:'http',url:`${ROOT}/accounts/default%3A2/portfolio`,body:JSON.stringify({account:'default:2',value:4999.96})});
  assert.equal(JSON.parse(quote.body).value,4999.96);
  assert.ok(quote.url.includes('default%3A00002'));
});
test('safety check rejects credentials hidden in escaped JSON body strings', () => {
  const options={key:'very-live-user',secret:'real"secret\\value'};
  const policy=createRecordingPolicy({recordSession:true,recordReplayOptions:()=>({}),sanitizeRecordingRecord:(record)=>record},options);
  assert.throws(()=>policy.sanitize({type:'http',body:JSON.stringify({message:options.secret})}),/retained a live credential/);
});
test('single FULL_TRADING account without explicit selection remains selected after anonymization', () => {
  const options={key:'actual-user',secret:'actual-password'};
  const safe=sanitizeBreakoutRecord({type:'http',body:JSON.stringify({userDetails:[{login:options.key,domain:'default',accounts:[{account:'default:99',accountStatus:'FULL_TRADING'}]}]})},options);
  assert.equal(JSON.parse(safe.body).userDetails[0].accounts[0].account,BREAKOUT_ACCOUNT);
});
test('a sole eligible live account overrides a stale optional subaccount during recording', () => {
  const options = { key: 'actual-user', secret: 'actual-password', subaccount: '00002' };
  const policy = createRecordingPolicy(breakoutprop, options, true);
  const user = policy.sanitize({
    type: 'http', url: `${ROOT}/users/${options.key}`,
    body: JSON.stringify({ userDetails: [{ login: options.key, domain: 'default', accounts: [
      { account: 'default:12', accountStatus: 'CLOSE_ONLY' },
      { account: 'default:99', accountStatus: 'FULL_TRADING' },
    ] }] }),
  });
  const accounts = JSON.parse(user.body).userDetails[0].accounts;
  assert.equal(accounts[1].account, BREAKOUT_ACCOUNT);
  assert.notEqual(accounts[0].account, BREAKOUT_ACCOUNT);
  const marketdata = policy.sanitize({
    type: 'http', method: 'POST', url: `${ROOT}/marketdata`,
    requestBody: JSON.stringify({ account: 'default:99', eventTypes: [{ type: 'Quote' }], symbols: ['ETHUSD'] }),
    body: JSON.stringify({ events: [] }),
  });
  verifyBreakoutRequest({ method: 'POST', url: marketdata.url, body: marketdata.requestBody, headers: auth });
  const portfolio = policy.sanitize({ type: 'http', url: `${ROOT}/accounts/default%3A99/portfolio`, body: '{}' });
  assert.ok(portfolio.url.includes(`/accounts/${encodeURIComponent(BREAKOUT_ACCOUNT)}/`));
});
test('live recording omits the fixture selector unless the caller explicitly supplies one', () => {
  assert.equal(breakoutprop.options.subaccount, '00002');
  const configured = { ...breakoutprop.options, ...breakoutprop.publicOptions };
  const defaults = { ...configured, ...breakoutprop.recordOptions, key: 'live-user', secret: 'live-password' };
  assert.equal(defaults.subaccount, undefined);
  const explicit = { ...configured, ...breakoutprop.recordOptions, subaccount: '99' };
  assert.equal(explicit.subaccount, '99');
  assert.equal(breakoutprop.recordReplayOptions(defaults).subaccount, '00002');
});

test('opaque escaped error payloads cannot leak credentials to disk or diagnostics', () => {
  const secret = 'quoted"private\\password';
  const policy = createRecordingPolicy({recordSession:true,recordReplayOptions:()=>({}),sanitizeRecordingRecord:(row)=>row}, {key:'live-username',secret});
  const message = 'server rejected: '+JSON.stringify({password:secret});
  assert.throws(()=>policy.sanitize({type:'http',body:message}),/retained a live credential/);
  const previous = console.error;
  const logs=[];
  try { console.error=(...parts)=>logs.push(parts.join(' ')); policy.log('error',message); }
  finally { console.error=previous; }
  assert.deepEqual(logs,['[error] Session recording diagnostic redacted']);
});
