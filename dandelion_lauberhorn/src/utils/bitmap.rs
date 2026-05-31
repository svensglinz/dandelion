use std::sync::atomic::{AtomicU64, Ordering};

// 64 bit bitmap
pub struct BitMap<const N: usize> {
    slots: AtomicU64,
}

impl<const N: usize> BitMap<N> {

    const VALID_MASK: u64 = if N == 64 { u64::MAX } else { (1u64 << N) - 1 };

    // size between 1 and 64
    pub fn new() -> Self {
        assert!(N > 0 && N <= 64);
        BitMap {
           slots: AtomicU64::new(0),
        }
    }

    pub fn reserve_slot(&self) -> Option<usize> {
        let mut cur = self.slots.load(Ordering::Acquire);
        while cur != u64::MAX {
            let free = !cur & Self::VALID_MASK; 
            if free == 0 {
                return None; 
            }
            let bit = free.trailing_zeros() as usize;
            let new = cur | (1u64 << bit); 

            match self.slots.compare_exchange_weak(
                cur, new, Ordering::AcqRel,
                Ordering::Relaxed
            ) {
                Ok(_) => return Some(bit),
                Err(x) => cur = x
            }
        }
        None
    }

    pub fn release_slot(&self, idx: usize) {
        debug_assert!(idx < N);
        self.slots.fetch_and(1u64 << idx, Ordering::AcqRel);
    }

}