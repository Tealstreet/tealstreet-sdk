# Offline vector capture from the pinned official signer. Never submits a transaction.
import ctypes as c,json,pathlib,hashlib,sys,base64
library=pathlib.Path(sys.argv[1]).resolve()
expected='5dc6de23dfac70dac56b6005e2b2697db333939f76f570da281dd08d3bf1144d'
if hashlib.sha256(library.read_bytes()).hexdigest()!=expected:
 raise SystemExit('Official darwin-arm64 SDK library hash mismatch')
root=pathlib.Path(__file__).resolve().parent.parent/'venues/lighter/tests/fixtures'
lib=c.CDLL(str(library))
class Response(c.Structure):
 _fields_=[('txType',c.c_uint8),('txInfo',c.c_void_p),('txHash',c.c_void_p),('message',c.c_void_p),('err',c.c_void_p)]
lib.Free.argtypes=[c.c_void_p]
lib.CreateClient.argtypes=[c.c_char_p,c.c_char_p,c.c_int,c.c_int,c.c_longlong];lib.CreateClient.restype=c.c_void_p
lib.SignModifyOrder.argtypes=[c.c_longlong]*6+[c.c_int]*2+[c.c_uint8]*3+[c.c_longlong]*2+[c.c_int,c.c_longlong];lib.SignModifyOrder.restype=Response
secret='01'+'00'*39
out=[]
for chain in [300,304,466324]:
 err=lib.CreateClient(b'http://127.0.0.1:1',secret.encode(),chain,3,42)
 if err: raise RuntimeError(c.string_at(err))
 for market,index,amount,price,trigger,nonce in [(1,42,2000,7900,8500,7),(4098,(1<<59)+17,1,1,2,8),(32767,(1<<60)-1,(1<<48)-1,4294967295,4294967294,9)]:
  r=lib.SignModifyOrder(market,index,amount,price,trigger,0,0,0,0,0,0,nonce,0,3,42)
  def take(p):
   if not p:return None
   s=c.string_at(p).decode();lib.Free(p);return s
  info,hash_,msg,err=map(take,[r.txInfo,r.txHash,r.message,r.err])
  if err: raise RuntimeError(err)
  out.append(dict(chainId=chain,txType=r.txType,txInfo=json.loads(info),txHash=hash_,message=msg,signatureHex=base64.b64decode(json.loads(info)['Sig']).hex()))
root.joinpath('modify-signing.json').write_text(json.dumps(dict(purpose='Public offline test scalar and official signing vectors; never a venue credential',source='elliottech/lighter-python@38a34672e7cb43c67d807ece25f125fda2dfdecf',librarySha256=expected,secret=secret,cases=out),indent=2)+'\n')
print('Captured',len(out),'offline official tx17 vectors; no submission')

