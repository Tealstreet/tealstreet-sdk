use serde_json::{Value, json};

#[derive(Default)]
pub struct DepthCorpus(Vec<(String, String, usize)>);
impl DepthCorpus {
    pub fn record(&mut self, value: &Value, pointer: &str, depth: usize) {
        self.0.push((
            serde_json::to_string(value).unwrap(),
            pointer.to_owned(),
            depth,
        ));
    }
    pub fn verify(self) {
        assert!(!self.0.is_empty(), "DEPTH-CORPUS cases must execute");
        let input = serde_json::to_string(&self.0).unwrap();
        let output = std::process::Command::new("node")
            .args([
                "-e",
                r#"
const rows=JSON.parse(process.argv[1]).map(([raw,path])=>{
 let value=JSON.parse(raw);for(const key of path.slice(1).split('/'))value=value[key];
 let depth=0;while(Array.isArray(value)){depth++;value=value[0]}return [depth,value];
});process.stdout.write(JSON.stringify(rows));
"#,
                &input,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "DEPTH-CORPUS Node failure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        let expected: Vec<_> = self
            .0
            .iter()
            .map(|(_, _, depth)| json!([depth, 1]))
            .collect();
        assert_eq!(
            actual,
            json!(expected),
            "DEPTH-CORPUS source acceptance and leaf identity"
        );
    }
}
