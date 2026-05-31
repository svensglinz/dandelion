use std::{cell::UnsafeCell, mem::size_of, ops::{Deref, DerefMut}};
use crate::utils::bitmap::BitMap;

// SAFETY: ObjectPool manages exclusive access via bitmap CAS
unsafe impl<const N: usize, T: Send> Send for ObjectPool<N, T> {}
unsafe impl<const N: usize, T: Send> Sync for ObjectPool<N, T> {}

pub struct ObjectPool<const N: usize, T> {
    elements: UnsafeCell<Vec<T>>,
    map: BitMap<N>,
}

pub struct PoolGuard<'a, const N: usize, T> {
    pool: &'a ObjectPool<N, T>,
    elem: &'a mut T,
}

impl<'a, const N: usize, T> PoolGuard<'a, N, T> {
    pub fn index(&self) -> usize {
        self.pool.index_of_ref(self.elem).unwrap()
    }
}

impl<'a, const N: usize, T> Deref for PoolGuard<'a, N, T> {
    type Target = T;
    fn deref(&self) -> &T { self.elem }
}

impl<'a, const N: usize, T> DerefMut for PoolGuard<'a, N, T> {
    fn deref_mut(&mut self) -> &mut T { self.elem }
}

impl<'a, const N: usize, T> Drop for PoolGuard<'a, N, T> {
    fn drop(&mut self) { self.pool.release(self.elem); }
}

impl<const N: usize, T> ObjectPool<N, T> {
    pub fn new(elements: Vec<T>) -> Self {
        println!("N = {}, len = {}", N, elements.len());
        assert!(N > 0 && N <= 64);
        assert!(elements.len() == N, "must provide exactly N elements");
        ObjectPool {
            map: BitMap::new(),
            elements: UnsafeCell::new(elements),
        }
    }

    pub fn size(&self) -> usize {
        return N;
    }
    
    pub fn claim(&self) -> Option<PoolGuard<N, T>> {
        Some(PoolGuard { pool: self, elem: self.claim_mut_manual()? })
    }

    // manual variants for cross-message lifetimes
    pub fn claim_manual(&self) -> Option<(usize, &T)> {
        let idx = self.map.reserve_slot()?;
        unsafe { Some((idx, &(*(*self.elements.get()))[idx])) }
    }

    pub fn claim_mut_manual(&self) -> Option<&mut T> {
        let idx = self.map.reserve_slot()?;
        unsafe { Some(&mut (*(*self.elements.get()))[idx]) }
    }

    fn index_of_ref(&self, val: &T) -> Option<usize> {
        let base = unsafe { (*self.elements.get()).as_ptr() } as usize;
        let elem = val as *const T as usize;
        if elem < base { return None; }
        let idx = (elem - base) / size_of::<T>();
        if idx < N { Some(idx) } else { None }
    }

    pub fn release(&self, obj: &T) {
        let idx = self.index_of_ref(obj).expect("element not in pool");
        self.map.release_slot(idx);
    }

    pub fn index_of(&self, obj: &T) -> usize {
        self.index_of_ref(obj).expect("element not in pool")
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        if idx >= N { return None; }
        unsafe { Some(&(*(*self.elements.get()))[idx]) }
    }

    pub fn get_mut(&self, idx: usize) -> Option<&mut T> {
        if idx >= N { return None; }
        unsafe { Some(&mut (*(*self.elements.get()))[idx]) }
    }
}