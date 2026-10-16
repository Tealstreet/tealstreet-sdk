from pathlib import Path
script=Path('sdk-rs/safe-cex-venue-cryptocom/scripts/bridge-controls.py').resolve()
source=script.read_text()
changes={
 "('bridge-controls-'+sha[:10])":"('bridge-controls-preflight-'+sha[:10])",
 'for label,filename,old,new,test,assertion in controls:':"assert [item[0] for item in controls[20:]] == ['exchange-constructor-binding','proof-admission-guard','source-noop']\nfor label,filename,old,new,test,assertion in controls[20:]:",
 'assert len(records)==46':'assert len(records)==6',
 "print('COMPLETE floor23 stopped index22 source-noop; host await-turn control is a witness-model control.',flush=True)":"print('PREFLIGHT COMPLETE floor3 stopped index2 source-noop; not the full23 control ledger.',flush=True)",
}
for old,new in changes.items():
 assert source.count(old)==1,old
 source=source.replace(old,new)
exec(compile(source,str(script),'exec'),{'__file__':str(script),'__name__':'__main__'})
