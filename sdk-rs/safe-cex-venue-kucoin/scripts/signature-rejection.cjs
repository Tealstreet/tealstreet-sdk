// The real isKucoinSignatureRejection over a corpus read from stdin.
const {capture}=require('./api-retry-capture.cjs');
const signatureCorpus=JSON.parse(require('node:fs').readFileSync(0,'utf8'));
capture('kucoin','get','/',undefined,'network',0,{signatureCorpus})
 .then(results=>process.stdout.write(JSON.stringify(results)))
 .catch(error=>{console.error(error);process.exitCode=1;});
