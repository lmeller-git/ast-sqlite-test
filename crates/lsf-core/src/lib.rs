use std::sync::atomic::{AtomicU64, Ordering};

pub mod ast;
pub mod entry;

pub trait AtomicF64Ext {
    fn load_f64(&self, order: Ordering) -> f64;
    fn add_f64(&self, val: f64, order: Ordering);
    fn set_f64(&self, val: f64, order: Ordering);
    fn multiply_f64(&self, val: f64, order: Ordering);
}

impl AtomicF64Ext for AtomicU64 {
    fn load_f64(&self, order: Ordering) -> f64 {
        f64::from_bits(self.load(order))
    }

    fn add_f64(&self, val: f64, order: Ordering) {
        self.fetch_update(order, order, |current| {
            let sum = f64::from_bits(current) + val;
            Some(sum.to_bits())
        })
        .unwrap();
    }

    fn set_f64(&self, val: f64, order: Ordering) {
        self.store(val.to_bits(), order);
    }

    fn multiply_f64(&self, factor: f64, order: Ordering) {
        self.fetch_update(order, order, |current| {
            let decayed = f64::from_bits(current) * factor;
            Some(decayed.to_bits())
        })
        .unwrap();
    }
}
