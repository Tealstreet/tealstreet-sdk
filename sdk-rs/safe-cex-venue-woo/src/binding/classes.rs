//! JS class semantics for the ported class definitions: `derive` creates a
//! constructor whose `super(...)` builds the superclass receiver, installs the
//! prototype members, then runs the derived class's field initializers.
use super::{
    rt::{ClassRecord, Constructor, Initialize, Rt},
    value::{Js, Thrown},
};
use safe_cex_exchange::js_runtime::scope::scope_of;
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};

pub(crate) fn derive(
    rt: &Rt,
    name: &str,
    superclass: Js,
    constructor: Constructor,
    initialize: Initialize,
) -> Result<Js, Thrown> {
    let me: Rc<OnceCell<Js>> = Rc::default();
    let own = me.clone();
    let class = Js::constructor(name, None, move |target, args| {
        let class = own
            .get()
            .cloned()
            .ok_or_else(|| Thrown::type_error("class constructor used before definition"))?;
        constructor(class, target, args)
    });
    let _ = me.set(class.clone());
    class.set("__proto__", superclass.clone())?;
    rt.shared
        .extra
        .classes
        .borrow_mut()
        .push(Rc::new(ClassRecord {
            class: class.clone(),
            superclass,
            initialize,
            prototype: RefCell::default(),
        }));
    Ok(class)
}

/// `super(...args)` inside `active`'s constructor.
pub(crate) fn super_construct(
    rt: &Rt,
    active: &Js,
    target: &Js,
    args: Vec<Js>,
) -> Result<Js, Thrown> {
    let record = rt
        .shared
        .extra
        .class(active)
        .ok_or_else(|| Thrown::type_error("super() outside a derived class constructor"))?;
    let receiver = record.superclass.construct(args, Some(target))?;
    for (name, member) in record.prototype.borrow().iter() {
        if !receiver.has_own(name) {
            receiver.define(name, member.clone())?;
        }
    }
    (record.initialize)(receiver.clone())?;
    Ok(receiver)
}

/// A prototype method: `this` is the caller's receiver, resolved per call.
pub(crate) fn prototype_member(rt: &Rt, class: &Js, name: &'static str) -> Result<(), Thrown> {
    let record = rt
        .shared
        .extra
        .class(class)
        .ok_or_else(|| Thrown::type_error("prototype member on an unknown class"))?;
    let weak = Rc::downgrade(&rt.shared);
    let member = Js::function(name, move |this, args| {
        let callable = super::hosts::bind(&scope_of(&weak)?.with_this(this.clone()), name)?;
        callable.call(this, args)
    });
    record.prototype.borrow_mut().push((name, member));
    Ok(())
}

/// The implicit-constructor derived socket classes: forward the arguments to the
/// superclass, then run the class-field initializer on the returned receiver.
pub(crate) fn derive_socket(
    name: &str,
    superclass: Js,
    initialize: Initialize,
) -> Result<Js, Thrown> {
    Ok(Js::constructor(name, None, move |target, args| {
        let receiver = superclass.construct(args, Some(&target))?;
        initialize(receiver.clone())?;
        Ok(receiver)
    }))
}
