use std::sync::atomic::{AtomicU64, Ordering};

pub mod ast;
pub mod entry;

pub trait AtomicF64Ext {
    fn atomic_load_f64(&self, order: Ordering) -> f64;
    fn atomic_add_f64(&self, val: f64, order: Ordering);
    fn atomic_set_f64(&self, val: f64, order: Ordering);
    fn atomic_multiply_f64(&self, val: f64, order: Ordering);
}

impl AtomicF64Ext for AtomicU64 {
    fn atomic_load_f64(&self, order: Ordering) -> f64 {
        f64::from_bits(self.load(order))
    }

    fn atomic_add_f64(&self, val: f64, order: Ordering) {
        self.fetch_update(order, order, |current| {
            let sum = f64::from_bits(current) + val;
            Some(sum.to_bits())
        })
        .unwrap();
    }

    fn atomic_set_f64(&self, val: f64, order: Ordering) {
        self.store(val.to_bits(), order);
    }

    fn atomic_multiply_f64(&self, factor: f64, order: Ordering) {
        self.fetch_update(order, order, |current| {
            let decayed = f64::from_bits(current) * factor;
            Some(decayed.to_bits())
        })
        .unwrap();
    }
}
