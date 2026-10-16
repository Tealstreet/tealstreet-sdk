"""Exercise the real runners' selection and completion guards without spawning Cargo.

Execute their setup plus inventory assertions; omit only the subprocess loop.
The completed-count control empties that loop while retaining its final guard.
"""
from pathlib import Path
import ast, os, sys

root=Path(__file__).resolve().parent
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
passed=failed=0
for filename,flag,loop_names,prefix in [
 ('cardinality_controls.py','--consuming-only',('name','target','mut','assertion'),'PHEMEX-CONTROLS'),
 ('source_cardinality_controls.py','--new-only',('target','oracle','path','assertion'),'PHEMEX-SOURCE-CONTROLS'),
]:
 path=root/filename
 tree=ast.parse(path.read_text(),filename=str(path))
 loops=[i for i,n in enumerate(tree.body) if isinstance(n,ast.For) and isinstance(n.target,ast.Tuple) and tuple(v.id for v in n.target.elts)==loop_names]
 assert len(loops)==1,'unique real execution loop'
 cut=loops[0]
 setup=ast.Module(body=tree.body[:cut],type_ignores=[])
 for args in [[],[flag]]:
  sys.argv=[str(path),*args]
  namespace={'__file__':str(path),'__name__':'__main__'}
  exec(compile(setup,str(path),'exec'),namespace)
  passed+=1
  for mode in ['empty','subset']:
   changed=ast.parse(ast.unparse(setup))
   guard=[i for i,n in enumerate(changed.body) if isinstance(n,ast.Assert) and isinstance(n.msg,ast.Constant) and n.msg.value==prefix+' selected inventory']
   assert len(guard)==1
   changed.body[guard[0]:guard[0]]=ast.parse('selected = []' if mode=='empty' else 'selected = selected[:1]').body
   try:exec(compile(ast.fix_missing_locations(changed),str(path),'exec'),{'__file__':str(path),'__name__':'__main__'})
   except AssertionError as error:
    assert str(error)==prefix+' selected inventory',error
    failed+=1
   else:raise AssertionError('selection control survived')
  # Keep valid selection, skip all actual executions, and run the real final guard.
  tail=ast.Module(body=tree.body[cut+1:],type_ignores=[])
  try:exec(compile(tail,str(path),'exec'),namespace)
  except AssertionError as error:
   assert str(error)==prefix+' executed inventory',error
   failed+=1
  else:raise AssertionError('execution control survived')
assert (passed,failed)==(4,12),'selection control execution census'
print('4 real selection inventories passed; 12 empty/subset/skipped-loop controls failed by name; 0 subprocesses')
