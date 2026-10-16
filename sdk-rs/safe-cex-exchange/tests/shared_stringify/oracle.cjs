const raws = ['null','true','false','0','-0','1e400','-1e400','1e-400','""','"\\ufeff"','"\\u0085"','"😀"','"\\n\\t\\u0000"','[]','{}','[1e400,-1e400,-0]','{"b":1e400,"2":-0,"1":1.2,"01":3,"4294967295":4,"a":[1e400]}'];
for(let n=-400;n<=400;n++)raws.push(`1e${n}`, `{"x":-1e${n},"0":[1e${n}]}`);
process.stdout.write(JSON.stringify(raws.map(raw=>{const value=JSON.parse(raw);return {raw,json:JSON.stringify(value),wire:typeof value==='string'?value:JSON.stringify(value)};})));
