//! Class allocation, `super` and method installation for the three venue
//! classes. Constructors enter the mapped `construct_core_*` bodies; methods
//! and arrows enter `call_core_*`.
use super::{
    Realm,
    value::{
        FunctionData, Kind, NativeConstruct, NativeFn, ObjectData, Prop, PropKey, Slot, Thrown,
        Value,
    },
};
use crate::{
    core_exchange_class::{self, CoreExchangeCapture, CoreExchangeClassHost},
    helper_tasks::HelperObserver,
    private_class::{self, CorePrivateClassHost, CorePrivateMethodCapture},
    private_constructor::{CorePrivateCapture, PrivateConstructorHost},
    public_class::{self, CorePublicClassHost},
    public_constructor::{PublicConstructorHost, PublicSocketCapture},
    spot_json::JsString,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

type ClassBody = Rc<dyn Fn(&mut Realm, &Value, &[Value], &Value) -> Result<Value, Thrown>>;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

impl Realm {
    /// `class Name extends base`: a constructor whose [[Prototype]] is `base`,
    /// a prototype inheriting `base.prototype`, and [[Construct]] → `body`.
    /// Calling it without `new` throws, as a class constructor does.
    pub(crate) fn derived_class(
        &mut self,
        name: &str,
        length: u8,
        base: &Value,
        body: ClassBody,
    ) -> Result<(Value, Value), Thrown> {
        let base_proto = match base {
            Value::Null => None,
            b => match self.get_str(b, "prototype")? {
                Value::Object(o) => Some(o),
                Value::Null => None,
                _ => {
                    return Err(self
                        .type_error("Class extends value does not have valid prototype property"));
                }
            },
        };
        let prototype = self.alloc(Kind::Ordinary, base_proto.as_ref());
        let me: Rc<RefCell<Weak<RefCell<ObjectData>>>> = Rc::default();
        let slot = me.clone();
        let construct: NativeConstruct = Rc::new(move |r, args, new_target| {
            let class = slot
                .borrow()
                .upgrade()
                .map(Value::Object)
                .ok_or_else(|| r.error("class was collected"))?;
            body(r, &class, args, new_target)
        });
        let class = self.function_data(FunctionData {
            name: JsString(name.encode_utf16().collect()),
            length,
            call: None,
            construct: Some(construct),
        });
        let o = class.object().unwrap();
        *me.borrow_mut() = Rc::downgrade(o);
        o.borrow_mut().proto = match base {
            Value::Object(b) => Some(b.clone()),
            _ => Some(self.intrinsics().function_proto.clone()),
        };
        o.borrow_mut().props.push(Prop {
            key: PropKey::str("prototype"),
            slot: Slot::Data(prototype.clone()),
            enumerable: false,
            writable: false,
            configurable: false,
        });
        self.define_hidden(&prototype, PropKey::str("constructor"), class.clone());
        Ok((class, prototype))
    }
    /// `super(...args)`: construct the class's *current* superclass with the original newTarget.
    pub(crate) fn super_construct(
        &mut self,
        class: &Value,
        args: &[Value],
        new_target: &Value,
    ) -> Result<Value, Thrown> {
        let parent = class
            .object()
            .and_then(|o| o.borrow().proto.clone())
            .map(Value::Object)
            .unwrap_or(Value::Null);
        if parent
            .function()
            .and_then(|f| f.construct.clone())
            .is_none()
        {
            return Err(self.type_error("Super constructor is not a constructor"));
        }
        Realm::construct(self, &parent, args, Some(new_target))
    }
    fn method_function(&self, name: &str, length: u8, call: NativeFn) -> Value {
        self.function(name, length, call)
    }
}

impl CoreExchangeClassHost for Realm {
    fn allocate_exchange_class(&mut self, base: &Value) -> Result<(Value, Value), Value> {
        self.derived_class(
            "HyperliquidExchange",
            3,
            base,
            Rc::new(|r, class, args, new_target| {
                let args = [arg(args, 0), arg(args, 1), arg(args, 2)];
                core_exchange_class::construct_core_exchange_class(
                    r,
                    class,
                    &args,
                    Some(new_target),
                )
            }),
        )
    }
    fn core_exchange_function(
        &mut self,
        capture: CoreExchangeCapture<Value>,
        name: &str,
        length: u8,
    ) -> Result<Value, Value> {
        Ok(self.method_function(
            name,
            length,
            Rc::new(move |r, this, args| {
                core_exchange_class::call_core_exchange(r, &capture, this, args)
            }),
        ))
    }
    fn define_exchange_method(
        &mut self,
        prototype: &Value,
        name: &str,
        value: &Value,
        getter: bool,
    ) -> Result<(), Value> {
        if getter {
            self.define_getter(prototype, PropKey::str(name), value.clone());
        } else {
            self.define_hidden(prototype, PropKey::str(name), value.clone());
        }
        Ok(())
    }
    fn exchange_super(
        &mut self,
        class: &Value,
        args: &[Value; 3],
        target: &Value,
    ) -> Result<Value, Value> {
        self.super_construct(class, args, target)
    }
    fn with_exchange_class(&self, class: &Value) -> Self {
        self.with_lexical(class)
    }
    fn observe_exchange_task(&self, observer: HelperObserver<Value, Value>) -> Value {
        self.spawn_value(Box::pin(observer))
    }
}

impl CorePrivateClassHost for Realm {
    fn allocate_private_class(&mut self, base: &Value) -> Result<(Value, Value), Value> {
        self.derived_class(
            "HyperliquidPrivateWebSocket",
            1,
            base,
            Rc::new(|r, class, args, new_target| {
                private_class::construct_core_private_class(
                    r,
                    class,
                    &arg(args, 0),
                    Some(new_target),
                )
            }),
        )
    }
    fn private_method(
        &mut self,
        capture: CorePrivateMethodCapture<Value>,
        name: &str,
        length: u8,
    ) -> Result<Value, Value> {
        Ok(self.method_function(
            name,
            length,
            Rc::new(move |r, this, args| {
                private_class::call_core_private_method(r, &capture, this, args)
            }),
        ))
    }
    fn define_private_method(
        &mut self,
        prototype: &Value,
        name: &str,
        value: &Value,
    ) -> Result<(), Value> {
        self.define_hidden(prototype, PropKey::str(name), value.clone());
        Ok(())
    }
}

impl PrivateConstructorHost for Realm {
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn array(&mut self) -> Result<Value, Value> {
        Ok(self.new_array(vec![]))
    }
    fn construct(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, constructor, args, None)
    }
    fn private_super(
        &mut self,
        class: &Value,
        parent: &Value,
        new_target: &Value,
    ) -> Result<Value, Value> {
        self.super_construct(class, std::slice::from_ref(parent), new_target)
    }
    fn private_arrow(
        &mut self,
        capture: CorePrivateCapture<Value>,
        name: &str,
        length: u8,
    ) -> Result<Value, Value> {
        Ok(self.method_function(
            name,
            length,
            Rc::new(move |r, _, args| private_class::call_core_private_arrow(r, &capture, args)),
        ))
    }
}

impl CorePublicClassHost for Realm {
    fn allocate_public_class(&mut self, base: &Value) -> Result<(Value, Value), Value> {
        self.derived_class(
            "HyperliquidPublicWebSocket",
            1,
            base,
            Rc::new(|r, class, args, new_target| {
                public_class::construct_core_public_class(r, class, &arg(args, 0), Some(new_target))
            }),
        )
    }
    fn public_dispose_method(&mut self, class: &Value) -> Result<Value, Value> {
        let class = class.clone();
        Ok(self.method_function(
            "dispose",
            0,
            Rc::new(move |r, this, _| public_class::call_core_public_dispose(r, &class, this)),
        ))
    }
    fn define_public_method(
        &mut self,
        prototype: &Value,
        name: &str,
        value: &Value,
    ) -> Result<(), Value> {
        self.define_hidden(prototype, PropKey::str(name), value.clone());
        Ok(())
    }
    fn public_super_get(
        &mut self,
        class: &Value,
        receiver: &Value,
        name: &str,
    ) -> Result<Value, Value> {
        let home = self.get_str(class, "prototype")?;
        self.super_get(&home, receiver, name)
    }
}

impl PublicConstructorHost for Realm {
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn construct(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, constructor, args, None)
    }
    fn public_super(
        &mut self,
        class: &Value,
        parent: &Value,
        new_target: &Value,
    ) -> Result<Value, Value> {
        self.super_construct(class, std::slice::from_ref(parent), new_target)
    }
    fn public_arrow(
        &mut self,
        capture: PublicSocketCapture<Value>,
        name: &str,
        length: u8,
    ) -> Result<Value, Value> {
        Ok(self.method_function(
            name,
            length,
            Rc::new(move |r, _, args| public_class::call_core_public_arrow(r, &capture, args)),
        ))
    }
}
