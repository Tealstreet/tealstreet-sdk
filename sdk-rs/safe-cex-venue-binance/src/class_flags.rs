//! Boolean source guards backed either by a fixture Cell or the owner's live Base.
use std::{cell::Cell, rc::Rc};
pub enum LiveFlag {
    Cell(Cell<bool>),
    Read(Rc<dyn Fn() -> bool>),
}
impl Default for LiveFlag {
    fn default() -> Self {
        Self::Cell(Cell::new(false))
    }
}
impl From<Cell<bool>> for LiveFlag {
    fn from(cell: Cell<bool>) -> Self {
        Self::Cell(cell)
    }
}
impl LiveFlag {
    pub fn read(get: Rc<dyn Fn() -> bool>) -> Self {
        Self::Read(get)
    }
    pub fn get(&self) -> bool {
        match self {
            Self::Cell(cell) => cell.get(),
            Self::Read(get) => get(),
        }
    }
    pub fn set(&self, value: bool) {
        match self {
            Self::Cell(cell) => cell.set(value),
            Self::Read(_) => panic!("live Base guard is read only; mutate its Base owner"),
        }
    }
}
