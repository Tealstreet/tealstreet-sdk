'use strict';
const {spawnSync}=require('node:child_process'),path=require('node:path'),assert=require('node:assert/strict');
const out=spawnSync(process.execPath,[path.join(__dirname,'class-composition-oracle.cjs')],{env:{...process.env,LIGHTER_CLASS_MUTATION:'field'},encoding:'utf8'});
assert.equal(out.status,1,'sourceClassMutationMustFailNamed');assert.match(out.stderr,/authored initial cache version zero/,'sourceClassMutationMustFailNamed');
