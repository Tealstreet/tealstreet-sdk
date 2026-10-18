//! Class assembly over canonical shared constructor and prototype operations.
use super::Host;
use crate::{
    core_exchange_class::{self, CoreExchangeCapture, CoreExchangeClassHost},
    helper_tasks::HelperObserver,
    private_class::{self, CorePrivateClassHost, CorePrivateMethodCapture},
    private_constructor::{CorePrivateCapture, PrivateConstructorHost},
    public_class::{self, CorePublicClassHost},
    public_constructor::{PublicConstructorHost, PublicSocketCapture},
};
use safe_cex_exchange::js_runtime::value::{FunctionData, Js, Thrown};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

type ClassBody = Rc<dyn Fn(&mut Host, &Js, &[Js], &Js) -> Result<Js, Thrown>>;
fn arg(args: &[Js], index: usize) -> Js {
    args.get(index).cloned().unwrap_or(Js::Undefined)
}
impl Host {
    pub(crate) fn derived_class(
        &mut self,
        name: &str,
        length: u8,
        base: &Js,
        body: ClassBody,
    ) -> Result<(Js, Js), Thrown> {
        let parent = if matches!(base, Js::Null) {
            Js::Null
        } else {
            base.get("prototype")?
        };
        if !parent.is_object() && !matches!(parent, Js::Null) {
            return Err(Thrown::type_error(
                "Class extends value does not have valid prototype property",
            ));
        }
        let prototype = Js::object(vec![]);
        prototype.set("__proto__", parent)?;
        let slot: Rc<RefCell<Weak<FunctionData>>> = Rc::default();
        let own = slot.clone();
        let class = self.constructor(name, length, move |host, args, target| {
            let class = own
                .borrow()
                .upgrade()
                .map(Js::Function)
                .ok_or_else(|| host.error("class was collected"))?;
            body(host, &class, args, target)
        });
        let Js::Function(function) = &class else {
            unreachable!()
        };
        *slot.borrow_mut() = Rc::downgrade(function);
        if !matches!(base, Js::Null) {
            class.set("__proto__", base.clone())?;
        }
        class.define_property(
            &"prototype".encode_utf16().collect::<Vec<_>>(),
            &Js::object(vec![("value", prototype.clone())]),
        )?;
        self.define_hidden(&prototype, Js::str("constructor"), class.clone());
        Ok((class, prototype))
    }
    pub(crate) fn super_construct(
        &mut self,
        class: &Js,
        args: &[Js],
        target: &Js,
    ) -> Result<Js, Thrown> {
        class
            .super_constructor()?
            .construct(args.to_vec(), Some(target))
    }
    fn method_function(
        &self,
        name: &str,
        length: u8,
        body: Rc<dyn Fn(&mut Host, &Js, &[Js]) -> Result<Js, Thrown>>,
    ) -> Js {
        self.native(name, length, move |host, receiver, args| {
            body(host, receiver, args)
        })
    }
}

impl CoreExchangeClassHost for Host {
    fn allocate_exchange_class(&mut self, base: &Js) -> Result<(Js, Js), Thrown> {
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
        capture: CoreExchangeCapture<Js>,
        name: &str,
        length: u8,
    ) -> Result<Js, Thrown> {
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
        prototype: &Js,
        name: &str,
        value: &Js,
        getter: bool,
    ) -> Result<(), Thrown> {
        if getter {
            self.define_getter(prototype, Js::str(name), value.clone());
        } else {
            self.define_hidden(prototype, Js::str(name), value.clone());
        }
        Ok(())
    }
    fn exchange_super(&mut self, class: &Js, args: &[Js; 3], target: &Js) -> Result<Js, Thrown> {
        self.super_construct(class, args, target)
    }
    fn with_exchange_class(&self, class: &Js) -> Self {
        self.with_lexical(class)
    }
    fn observe_exchange_task(&self, observer: HelperObserver<Js, Thrown>) -> Js {
        self.spawn_value(Box::pin(observer))
    }
}

impl CorePrivateClassHost for Host {
    fn allocate_private_class(&mut self, base: &Js) -> Result<(Js, Js), Thrown> {
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
        capture: CorePrivateMethodCapture<Js>,
        name: &str,
        length: u8,
    ) -> Result<Js, Thrown> {
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
        prototype: &Js,
        name: &str,
        value: &Js,
    ) -> Result<(), Thrown> {
        self.define_hidden(prototype, Js::str(name), value.clone());
        Ok(())
    }
}

impl PrivateConstructorHost for Host {
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn array(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_array(vec![]))
    }
    fn construct(&mut self, constructor: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::construct(self, constructor, args, None)
    }
    fn private_super(&mut self, class: &Js, parent: &Js, new_target: &Js) -> Result<Js, Thrown> {
        self.super_construct(class, std::slice::from_ref(parent), new_target)
    }
    fn private_arrow(
        &mut self,
        capture: CorePrivateCapture<Js>,
        name: &str,
        length: u8,
    ) -> Result<Js, Thrown> {
        Ok(self.method_function(
            name,
            length,
            Rc::new(move |r, _, args| private_class::call_core_private_arrow(r, &capture, args)),
        ))
    }
}

impl CorePublicClassHost for Host {
    fn allocate_public_class(&mut self, base: &Js) -> Result<(Js, Js), Thrown> {
        self.derived_class(
            "HyperliquidPublicWebSocket",
            1,
            base,
            Rc::new(|r, class, args, new_target| {
                public_class::construct_core_public_class(r, class, &arg(args, 0), Some(new_target))
            }),
        )
    }
    fn public_dispose_method(&mut self, class: &Js) -> Result<Js, Thrown> {
        let class = class.clone();
        Ok(self.method_function(
            "dispose",
            0,
            Rc::new(move |r, this, _| public_class::call_core_public_dispose(r, &class, this)),
        ))
    }
    fn define_public_method(
        &mut self,
        prototype: &Js,
        name: &str,
        value: &Js,
    ) -> Result<(), Thrown> {
        self.define_hidden(prototype, Js::str(name), value.clone());
        Ok(())
    }
    fn public_super_get(&mut self, class: &Js, receiver: &Js, name: &str) -> Result<Js, Thrown> {
        let home = self.get_str(class, "prototype")?;
        self.super_get(&home, receiver, name)
    }
}

impl PublicConstructorHost for Host {
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn construct(&mut self, constructor: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::construct(self, constructor, args, None)
    }
    fn public_super(&mut self, class: &Js, parent: &Js, new_target: &Js) -> Result<Js, Thrown> {
        self.super_construct(class, std::slice::from_ref(parent), new_target)
    }
    fn public_arrow(
        &mut self,
        capture: PublicSocketCapture<Js>,
        name: &str,
        length: u8,
    ) -> Result<Js, Thrown> {
        Ok(self.method_function(
            name,
            length,
            Rc::new(move |r, _, args| public_class::call_core_public_arrow(r, &capture, args)),
        ))
    }
}
