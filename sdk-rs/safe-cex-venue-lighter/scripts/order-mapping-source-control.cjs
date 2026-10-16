'use strict';
const {spawnSync}=require('node:child_process'),path=require('node:path'),assert=require('node:assert/strict');
const out=spawnSync(process.execPath,[path.join(__dirname,'order-mapping-oracle.cjs')],{env:{...process.env,LIGHTER_ORDER_MAPPING_MUTATION:'remaining'},encoding:'utf8'});
assert.equal(out.status,1,'sourceMappingMutationMustFailNamed');assert.match(out.stderr,/authored open remaining recovers amount10/,'sourceMappingMutationMustFailNamed');
