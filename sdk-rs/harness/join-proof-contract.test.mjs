import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
const require = createRequire(import.meta.url);
const ts = require('typescript');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const vectors = JSON.parse(readFileSync(path.join(root, 'sdk-rs/crypto/tests/fixtures/join-proof.json'), 'utf8')).cases;

test('independent Node SHA-256 reproduces the frozen identity and challenge vectors', () => {
  for (const v of vectors) {
    const components = [v.owner, v.scope, v.keyId].map((value) => {
      const bytes = Buffer.from(value);
      const length = Buffer.alloc(8);
      length.writeBigUInt64BE(BigInt(bytes.length));
      return Buffer.concat([length, bytes]);
    });
    const digest = createHash('sha256')
      .update(Buffer.concat([Buffer.from('tealstreet/identity/v1'), ...components]))
      .digest();
    assert.equal(digest.toString('hex'), v.identityDigest);
    assert.deepEqual([...digest], v.statement.identityDigest);
    const attempt = Buffer.from(v.statement.attempt);
    const length = Buffer.alloc(4);
    length.writeUInt32BE(attempt.length);
    const stamp = Buffer.alloc(8);
    stamp.writeBigUInt64BE(BigInt(v.statement.issuedAtMs));
    assert.equal(
      createHash('sha256')
        .update(
          Buffer.concat([
            Buffer.from('tealstreet/admission/challenge/v1\0'),
            length,
            attempt,
            digest,
            Buffer.from(v.statement.nonce),
            stamp,
          ]),
        )
        .digest('hex'),
      v.challengeDigest,
    );
  }
});

test('public join types require v1/registration and exclude mixed signing replies', () => {
  const fixture = path.join(root, 'packages/safe-cex/src/__join_proof_contract_check__.ts');
  const body = {
    id: 'request-1',
    type: 'join-proof',
    exchangeName: 'backpack',
    accountName: 'account',
    instanceId: 'instance',
    joinProof: { version: 1, statement: vectors[0].statement },
  };
  const source = `import type { JoinProofRequest, JoinProofResponse, JoinProofEnvelope } from './joinProof.types';
const literal = ${JSON.stringify(body)} as const;
const valid: JoinProofRequest = literal;
const frame: JoinProofEnvelope = {t:'sign-req', d:valid};
const response: JoinProofResponse = {id:'id',joinProof:{version:1,signature:'fixed'}};
const refusal: JoinProofResponse = {id:'id',error:'Join proof refused'};
// @ts-expect-error HTTP material cannot be mixed into a challenge.
const mixed: JoinProofRequest = {...valid,config:{}};
// @ts-expect-error Future versions are not v1.
const version: JoinProofRequest = {...valid,joinProof:{...valid.joinProof,version:2}};
const {instanceId,...withoutInstance}=valid;
// @ts-expect-error Registration identity is mandatory.
const absent: JoinProofRequest = withoutInstance;
// @ts-expect-error Legacy signing outputs cannot be mixed into a proof reply.
const mixedReply: JoinProofResponse = {...response,wsAuthSignature:'fixed'};
// @ts-expect-error Arbitrary callback text cannot become a protocol refusal.
const rawError: JoinProofResponse = {id:'id',error:'raw callback text'};
// @ts-expect-error Proof statement facts are readonly.
valid.joinProof.statement.attempt='changed';
`;
  const options = {
    strict: true,
    exactOptionalPropertyTypes: true,
    noEmit: true,
    types: [],
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.CommonJS,
  };
  const host = ts.createCompilerHost(options);
  const getSource = host.getSourceFile.bind(host);
  host.getSourceFile = (file, version, ...rest) =>
    file === fixture ? ts.createSourceFile(file, source, version, true) : getSource(file, version, ...rest);
  const diagnostics = ts.getPreEmitDiagnostics(ts.createProgram([fixture], options, host));
  assert.deepEqual(
    diagnostics.map((d) => ts.flattenDiagnosticMessageText(d.messageText, '\n')),
    [],
  );
  const publicTypes = readFileSync(path.join(root, 'packages/safe-cex/src/types.ts'), 'utf8');
  assert.match(publicTypes, /export type \{[^}]*JoinProofRequest[^}]*\} from '\.\/joinProof\.types'/);
});
