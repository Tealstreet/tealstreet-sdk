const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-ws-session.ts');
const source = ts.createSourceFile(sourcePath, fs.readFileSync(sourcePath, 'utf8'), ts.ScriptTarget.Latest, true);
function readUnion(name) {
  const alias = source.statements.find((statement) =>
    ts.isTypeAliasDeclaration(statement) && statement.name.text === name,
  );
  assert.ok(alias && ts.isUnionTypeNode(alias.type), `${name} must remain a literal union`);
  const values = alias.type.types.map((type) => {
    assert.ok(ts.isLiteralTypeNode(type) && ts.isStringLiteral(type.literal), `${name} members must be string literals`);
    return type.literal.text;
  });
  assert.deepEqual(new Set(values).size, values.length, `${name} literals must be unique`);
  return values;
}

const accepted = readUnion('CoinbaseStarbaseWebsocketDisconnectReason');
const sessionStates = readUnion('CoinbaseStarbaseWebsocketSessionState');
const policy = source.statements.find((statement) =>
  ts.isTypeAliasDeclaration(statement) && statement.name.text === 'CoinbaseStarbaseCancelOnDisconnectPolicy',
);
assert.ok(policy && ts.isTypeLiteralNode(policy.type), 'cancel policy must remain an object type');
const policyFields = policy.type.members.map((member) => {
  assert.ok(ts.isPropertySignature(member) && member.type, 'cancel policy members must be typed properties');
  return {
    name: member.name.getText(source),
    optional: !!member.questionToken,
    type: member.type.getText(source),
  };
});

process.stdout.write(JSON.stringify({
  accepted,
  rejected: [null, false, 0, '', 'HEARTBEAT-TIMEOUT', 'transport ', [], ...accepted.map((value) => ({ [value]: null }))],
  sessionStates,
  sessionStateRejected: [null, false, 0, '', 'READY', 'ready ', [], ...sessionStates.map((value) => ({ [value]: null }))],
  policyFields,
}));
