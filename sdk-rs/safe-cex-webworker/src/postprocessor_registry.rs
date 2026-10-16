use std::{cell::RefCell, rc::Rc};

/// Imported constructor objects. The host supplies actual callable factories;
/// the registry never constructs substitute processors or compares their names.
pub struct ImportedPostprocessors<Independent: ?Sized, Dependent: ?Sized> {
    pub displayed_positions: Rc<Independent>,
    pub displayed_margin: Rc<Independent>,
    pub chaser_order_enrichment: Rc<Independent>,
    pub orders_risk_profile: Rc<Independent>,
    pub chaser_order: Rc<Dependent>,
    pub twap: Rc<Dependent>,
    pub message_handler_stats: Rc<Dependent>,
}

/// One source-module registry instance. Hosts retain/share this object across
/// worker initialization and extension registration, just as the source imports
/// refer to the same module arrays. Both lists remain live and mutable.
pub struct PostprocessorRegistry<Independent: ?Sized, Dependent: ?Sized> {
    pub independent_postprocessors: Rc<RefCell<Vec<Rc<Independent>>>>,
    pub exchange_dependent_postprocessors: Rc<RefCell<Vec<Rc<Dependent>>>>,
}
impl<Independent: ?Sized, Dependent: ?Sized> PostprocessorRegistry<Independent, Dependent> {
    pub fn new(imports: ImportedPostprocessors<Independent, Dependent>) -> Self {
        Self {
            independent_postprocessors: Rc::new(RefCell::new(vec![
                imports.displayed_positions,
                imports.displayed_margin,
                imports.chaser_order_enrichment,
                imports.orders_risk_profile,
            ])),
            exchange_dependent_postprocessors: Rc::new(RefCell::new(vec![
                imports.chaser_order,
                imports.twap,
                imports.message_handler_stats,
            ])),
        }
    }

    /// Callers must release outstanding list borrows before mutation.
    // ts: registerIndependentPostprocessor. Array.includes compares object identity.
    pub fn register_independent_postprocessor(&self, class: Rc<Independent>) {
        let mut classes = self.independent_postprocessors.borrow_mut();
        if !classes.iter().any(|existing| Rc::ptr_eq(existing, &class)) {
            classes.push(class);
        }
    }
}
