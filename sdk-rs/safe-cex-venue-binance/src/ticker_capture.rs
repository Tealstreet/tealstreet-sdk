//! Stable ticker reference views over the typed Store. A ticker generation change
//! replaces the dictionary; in-place patches update cells in the captured graph.
use safe_cex_store::Store;
use safe_cex_types::Ticker;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::{Rc, Weak},
};
pub type TickerReference = Rc<RefCell<Ticker>>;
pub type Dictionary = Rc<RefCell<BTreeMap<String, TickerReference>>>;
#[derive(Clone)]
pub enum Target {
    Row(TickerReference),
    StockInherited,
}
impl Target {
    pub fn id(&self) -> String {
        match self {
            Self::Row(row) => row.borrow().id.clone(),
            Self::StockInherited => "undefined".into(),
        }
    }
}
pub fn lookup(dictionary: &Dictionary, symbol: &str) -> Option<Target> {
    dictionary
        .borrow()
        .get(symbol)
        .cloned()
        .map(Target::Row)
        .or_else(|| {
            matches!(
                safe_cex_num::js_table_lookup(&[] as &[(&str, ())], symbol),
                safe_cex_num::TableHit::Inherited(_)
            )
            .then_some(Target::StockInherited)
        })
}

pub struct Capture {
    store: Weak<dyn Store>,
    generation: Cell<f64>,
    dictionary: RefCell<Dictionary>,
    rows: RefCell<Vec<TickerReference>>,
    stop: RefCell<Option<Box<dyn Fn()>>>,
}
impl Capture {
    pub fn new(store: Rc<dyn Store>) -> Result<Rc<Self>, safe_cex_types::StoreReadError> {
        let capture = Rc::new(Self {
            store: Rc::downgrade(&store),
            generation: Cell::new(f64::NAN),
            dictionary: Default::default(),
            rows: Default::default(),
            stop: Default::default(),
        });
        capture.refresh()?;
        let weak = Rc::downgrade(&capture);
        let stop = store.subscribe(Box::new(move |_, _| {
            if let Some(capture) = weak.upgrade() {
                capture.refresh().map_err(|error| {
                    safe_cex_types::StoreSubscriberError::new("Store ticker projection failed")
                        .with_cause(Rc::new(error))
                })?;
            }
            Ok(())
        }));
        *capture.stop.borrow_mut() = Some(stop);
        Ok(capture)
    }
    fn refresh(&self) -> Result<(), safe_cex_types::StoreReadError> {
        let Some(store) = self.store.upgrade() else {
            return Ok(());
        };
        let generation = store.get_tickers_version();
        let tickers = store.tickers()?;
        if self.generation.get() != generation {
            let rows: Vec<_> = tickers
                .iter()
                .map(|ticker| Rc::new(RefCell::new(ticker.clone())))
                .collect();
            let map = rows
                .iter()
                .map(|ticker| (ticker.borrow().symbol.clone(), ticker.clone()))
                .collect();
            *self.rows.borrow_mut() = rows;
            *self.dictionary.borrow_mut() = Rc::new(RefCell::new(map));
            self.generation.set(generation);
        } else {
            let rows = self.rows.borrow();
            for (row, ticker) in rows.iter().zip(tickers.iter()) {
                *row.borrow_mut() = ticker.clone();
            }
            let by_id: HashMap<_, _> = rows
                .iter()
                .map(|row| (row.borrow().id.clone(), row.clone()))
                .collect();
            let dictionary = self.dictionary.borrow();
            let mut dictionary = dictionary.borrow_mut();
            for (symbol, ticker) in store.tickers_by_symbol()?.iter() {
                if let Some(reference) = by_id.get(&ticker.id) {
                    dictionary.insert(symbol.clone(), reference.clone());
                }
            }
        }
        Ok(())
    }
    pub fn belongs_to(&self, store: &Rc<dyn Store>) -> bool {
        self.store
            .upgrade()
            .is_some_and(|original| Rc::ptr_eq(&original, store))
    }
    pub fn dictionary(&self) -> Result<Dictionary, safe_cex_types::StoreReadError> {
        self.refresh()?;
        Ok(self.dictionary.borrow().clone())
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.get_mut().take() {
            stop();
        }
    }
}
