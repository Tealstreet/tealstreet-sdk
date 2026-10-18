//! Array slice index normalization through installed builtins on the native host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, globals, scope::ScopeShared, value::Js},
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};

fn index(tag: &str) -> Js {
    match tag {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "bad" => Js::str("bad"),
        "NaN" => Js::Number(f64::NAN),
        "Infinity" => Js::Number(f64::INFINITY),
        "-Infinity" => Js::Number(f64::NEG_INFINITY),
        _ => Js::Number(tag.parse().unwrap()),
    }
}

#[test]
fn native_array_slice_matches_node_nan_bounds_and_holes() {
    let output = std::process::Command::new("node").args(["-e", r#"
const tags=['NaN','bad','undefined','null','Infinity','-Infinity','-5','-1','-0','0','1','2','4','9'];
const index=tag=>tag==='undefined'?undefined:tag==='null'?null:tag==='bad'?'bad':Number(tag);
const args=[[],...tags.map(tag=>[tag]),...tags.flatMap(start=>tags.map(end=>[start,end]))];
const rows=[];
for(const sparse of [false,true])for(const input of args){
 const result=(sparse?[5,,undefined,4]:[5,1,4]).slice(...input.map(index));
 rows.push({sparse,input,slots:Array.from({length:result.length},(_,i)=>({own:Object.hasOwn(result,i),value:String(result[i])}))});
}
if(rows.length!==422)throw Error('slice case floor');
console.log(JSON.stringify(rows));
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        422,
        "independent Node bound/argument/array case floor"
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt,async {
        let runtime=native_runtime();
        let http=client::rest_http_host(runtime.clone(),Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()));
        let shared=Rc::new(ScopeShared::new(runtime,http,Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),Js::object(vec![]),()));
        globals::install_builtins(&shared.globals,Rc::downgrade(&shared));
        for row in rows {
            let array=if row["sparse"]==true {Js::holey_array(vec![Some(Js::Number(5.)),None,Some(Js::Undefined),Some(Js::Number(4.))])}else{Js::array(vec![Js::Number(5.),Js::Number(1.),Js::Number(4.)])};
            let args=row["input"].as_array().unwrap().iter().map(|value|index(value.as_str().unwrap())).collect();
            let result=array.call_method("slice",args).unwrap();
            let Js::Array(cells)=result else {panic!("slice returns an array");};
            let slots:Vec<Value>=cells.borrow().iter().map(|slot|json!({"own":slot.is_some(),"value":slot.as_ref().unwrap_or(&Js::Undefined).to_display()})).collect();
            assert_eq!(json!(slots),row["slots"],"SLICE source bounds {} sparse={}",row["input"],row["sparse"]);
        }
    });
}
