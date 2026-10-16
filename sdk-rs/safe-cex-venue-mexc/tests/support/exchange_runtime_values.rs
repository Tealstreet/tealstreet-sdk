type FnValue = Rc<dyn Fn(N, Vec<N>) -> JsResult<N>>;
#[derive(Clone)]
enum N {
    U,
    M(Rc<RefCell<IndexMap<String, N>>>),
    Iter(Rc<RefCell<IndexMap<String, N>>>),
    Pair(Vec<N>),
    Null,
    B(bool),
    Num(f64),
    S(String),
    A(Rc<RefCell<Vec<N>>>),
    O(Rc<RefCell<IndexMap<String, N>>>),
    F(FnValue),
    P(Rc<Job>),
    E(Rc<RefCell<IndexMap<String, N>>>),
}
impl N {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::B(v),
            Value::Number(v) => Self::Num(v.as_f64().unwrap()),
            Value::String(v) => Self::S(v),
            Value::Array(v) => Self::array(v.into_iter().map(Self::from).collect()),
            Value::Object(v) => {
                Self::object(v.into_iter().map(|(k, v)| (k, Self::from(v))).collect())
            }
        }
    }
    fn object(fields: Vec<(String, N)>) -> Self {
        Self::O(Rc::new(RefCell::new(fields.into_iter().collect())))
    }
    fn obj(fields: Vec<(&str, N)>) -> Self {
        Self::object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    fn array(v: Vec<N>) -> Self {
        Self::A(Rc::new(RefCell::new(v)))
    }
    fn get(&self, key: &str) -> N {
        match self {
            Self::O(v) | Self::E(v) => v.borrow().get(key).cloned().unwrap_or(Self::U),
            Self::A(v) => key
                .parse::<usize>()
                .ok()
                .and_then(|i| v.borrow().get(i).cloned())
                .unwrap_or(Self::U),
            _ => Self::U,
        }
    }
    fn set(&self, key: &str, value: N) {
        match self {
            Self::O(v) | Self::E(v) => {
                v.borrow_mut().insert(key.into(), value);
            }
            _ => panic!("native object"),
        }
    }
    fn same(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::O(a), Self::O(b)) | (Self::E(a), Self::E(b)) => Rc::ptr_eq(a, b),
            (Self::A(a), Self::A(b)) => Rc::ptr_eq(a, b),
            (Self::M(a), Self::M(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    fn text(&self) -> String {
        match self {
            Self::U => "undefined".into(),
            Self::Null => "null".into(),
            Self::B(v) => v.to_string(),
            Self::Num(v) => safe_cex_num::js_number_string(*v),
            Self::S(v) => v.clone(),
            Self::E(_) => format!(
                "{}: {}",
                self.get("name").text(),
                self.get("message").text()
            ),
            _ => "[object Object]".into(),
        }
    }
    fn snapshot(&self) -> Self {
        match self {
            Self::A(v) => Self::array(v.borrow().iter().map(Self::snapshot).collect()),
            Self::O(v) => Self::object(
                v.borrow()
                    .iter()
                    .filter(|(_, v)| !matches!(v, N::F(_)))
                    .map(|(k, v)| (k.clone(), v.snapshot()))
                    .collect(),
            ),
            Self::M(v) => Self::M(Rc::new(RefCell::new(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.snapshot()))
                    .collect(),
            ))),
            _ => self.clone(),
        }
    }
}
fn error(name: &str, message: &str) -> N {
    let N::O(v) = N::obj(vec![
        ("name", N::S(name.into())),
        ("message", N::S(message.into())),
    ]) else {
        unreachable!()
    };
    N::E(v)
}
fn assert_native(v: &N, s: &Value, label: &str) {
    match v {
        N::M(v) => {
            let rows = s["map"].as_array().expect(label);
            assert_eq!(v.borrow().len(), rows.len(), "{label} native map floor");
            for ((k, v), r) in v.borrow().iter().zip(rows) {
                assert_eq!(k, r[0].as_str().unwrap(), "{label} native map key");
                assert_native(v, &r[1], label);
            }
        }
        N::U => assert_eq!(s["undefined"], true, "{label} native undefined"),
        N::Null => assert!(s.is_null(), "{label} native null"),
        N::B(v) => assert_eq!(Some(*v), s.as_bool(), "{label} native bool"),
        N::Num(v) if !v.is_finite() => {
            assert_eq!(
                s["number"],
                if v.is_nan() {
                    "NaN"
                } else if *v > 0. {
                    "Infinity"
                } else {
                    "-Infinity"
                },
                "{label} native nonfinite"
            )
        }
        N::Num(v) => assert_eq!(Some(*v), s.as_f64(), "{label} native number"),
        N::S(v) => assert_eq!(Some(v.as_str()), s.as_str(), "{label} native string"),
        N::A(v) => {
            let s = s.as_array().expect(label);
            assert_eq!(v.borrow().len(), s.len(), "{label} native array shape");
            for (v, s) in v.borrow().iter().zip(s) {
                assert_native(v, s, label);
            }
        }
        N::O(v) | N::E(v) => {
            let s = s.as_object().expect(label);
            let values = v.borrow();
            let values: Vec<_> = values
                .iter()
                .filter(|(_, v)| !matches!(v, N::F(_)))
                .collect();
            assert_eq!(values.len(), s.len(), "{label} native object shape");
            for (k, v) in values {
                assert!(s.contains_key(k), "{label} native key {k}");
                assert_native(v, &s[k], label);
            }
        }
        _ => panic!("{label} unresolved callable/promise"),
    }
}
struct Job {
    future: RefCell<Option<JsPromise<N>>>,
    result: RefCell<Option<JsResult<N>>>,
}
struct Handle(Rc<Job>);
impl Future for Handle {
    type Output = JsResult<N>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .result
            .borrow()
            .clone()
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}
impl std::fmt::Debug for N {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text())
    }
}
