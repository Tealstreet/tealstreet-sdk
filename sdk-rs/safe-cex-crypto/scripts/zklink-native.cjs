const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {createHash}=require('node:crypto');
const revision='3152525e2615ecbbdb56fd6728ce265ad2dde485';
const fixtures=path.resolve(__dirname,'../tests/fixtures/zklink');
function run(command,args,options={}) {const r=spawnSync(command,args,{stdio:'inherit',...options});assert.equal(r.status,0,`native SDK oracle command failed: ${command} ${args.join(' ')}`)}
function verifySource(directory) {
 const result=spawnSync('git',['-C',directory,'rev-parse','HEAD'],{encoding:'utf8'});assert.equal(result.status,0);assert.equal(result.stdout.trim(),revision,'native SDK revision changed');
 const expected=JSON.parse(fs.readFileSync(path.join(fixtures,'native-sdk-lock.json'),'utf8'));
 const actual=createHash('sha256').update(fs.readFileSync(path.join(directory,'Cargo.lock'))).digest('hex');
 assert.equal(actual,expected.compatibleSha256,'native SDK lock/pins changed');
 run('git',['-C',directory,'diff','--exit-code','--','.',':(exclude)Cargo.lock']);
}
if(process.argv[2]==='--prepare') {
 const directory=process.argv[3];assert(directory&&path.isAbsolute(directory),'absolute fresh SDK checkout required');
 fs.mkdirSync(directory);run('git',['-C',directory,'init','--quiet']);
 run('git',['-C',directory,'fetch','--quiet','--depth=1','https://github.com/zkLinkProtocol/zklink_sdk.git',revision]);
 run('git',['-C',directory,'checkout','--quiet','--detach','FETCH_HEAD']);
 const expected=JSON.parse(fs.readFileSync(path.join(fixtures,'native-sdk-lock.json'),'utf8'));
 assert.equal(createHash('sha256').update(fs.readFileSync(path.join(directory,'Cargo.lock'))).digest('hex'),expected.originalSha256,'upstream SDK lock changed');
 run('git',['-C',directory,'apply',path.join(fixtures,'native-sdk-time.patch')]);verifySource(directory);
 const examples=path.join(directory,'signers/examples');fs.mkdirSync(examples,{recursive:true});fs.copyFileSync(path.join(fixtures,'native-sdk.rs'),path.join(examples,'safe_cex_boundary_oracle.rs'));
} else {
 const directory=process.env.SAFE_CEX_ZKLINK_NATIVE_ORACLE;assert(directory,'prepare SAFE_CEX_ZKLINK_NATIVE_ORACLE before native boundary replay');verifySource(directory);
 assert.equal(fs.readFileSync(path.join(directory,'signers/examples/safe_cex_boundary_oracle.rs'),'utf8'),fs.readFileSync(path.join(fixtures,'native-sdk.rs'),'utf8'),'native SDK harness changed');
 // Old upstream warning noise is not a production-crate lint relaxation.
 const env={...process.env,RUSTFLAGS:''};
 run('cargo',['+stable','run','--locked','--manifest-path',path.join(directory,'Cargo.toml'),'-p','zklink_sdk_signers','--example','safe_cex_boundary_oracle','--',path.join(fixtures,'npm-wasm.json'),path.join(fixtures,'native-sdk-boundaries.json'),...process.argv.slice(2)],{env});
}
