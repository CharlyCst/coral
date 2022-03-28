#![no_std]
//! ## Collections used through the kernel and compiler.
//!
//! For now most of the collections comes directly from Cranelift (through the `cranelift_entity`
//! crate that is re-exported from `cranelift_codegen`).

extern crate alloc;
use alloc::vec::Vec;

use cranelift_codegen::entity;

use core::marker::PhantomData;
use core::ops::{Index, IndexMut};

// ——————————————————————————————— Re-Exports ——————————————————————————————— //

pub use entity::entity_impl;
pub use entity::{EntityRef, PrimaryMap, SecondaryMap};
pub use hashbrown::HashMap;

// ———————————————————————————— New Collections ————————————————————————————— //

/// A fixed lenght map with tagged indexes.
///
/// The values can still be modified, but the set of key is fixed. A new FrozenMap can be created
/// either by consuming a PrimaryMap, or by mapping another FrozenMap.
pub struct FrozenMap<K, V> {
    elems: Vec<V>,
    unused: PhantomData<K>,
}

impl<K, V> FrozenMap<K, V>
where
    K: EntityRef,
{
    /// Freeze a PrimaryMap, meaning that no new items can be added. It is still possible to mutate
    /// the existing entries.
    pub fn freeze(map: PrimaryMap<K, V>) -> Self {
        // PrimaryMap does not expose its internal vector, therefore we nedlessly re-allocate a
        // brand new vector and move all the elements inside.
        //
        // This could be fixed by upstreaming FrozenMap.
        let elems = map.into_iter().map(|(_, v)| v).collect();
        Self {
            elems,
            unused: PhantomData,
        }
    }

    pub fn map<F, U>(&self, f: F) -> FrozenMap<K, U>
    where
        F: FnMut(&V) -> U,
    {
        let elems = self.elems.iter().map(f).collect();
        FrozenMap {
            elems,
            unused: PhantomData,
        }
    }

    pub fn map_enumerate<F, U>(&self, mut f: F) -> FrozenMap<K, U>
    where
        F: FnMut(K, &V) -> U,
    {
        let elems = self
            .elems
            .iter()
            .enumerate()
            .map(|(idx, elem)| f(K::new(idx), elem))
            .collect();
        FrozenMap {
            elems,
            unused: PhantomData,
        }
    }

    pub fn try_map<F, U, E>(&self, mut f: F) -> Result<FrozenMap<K, U>, E>
    where
        F: FnMut(&V) -> Result<U, E>,
    {
        let mut elems = Vec::with_capacity(self.len());
        for elem in &self.elems {
            elems.push(f(elem)?);
        }
        Ok(FrozenMap {
            elems,
            unused: PhantomData,
        })
    }

    /// Change the type of the index in place.
    pub fn reindex<Q>(self) -> FrozenMap<Q, V> {
        FrozenMap {
            elems: self.elems,
            unused: PhantomData,
        }
    }

    /// Get the element at `k` if it exists.
    pub fn get(&self, k: K) -> Option<&V> {
        self.elems.get(k.index())
    }

    /// Get the element at `k` if it exists, mutable version.
    pub fn get_mut(&mut self, k: K) -> Option<&mut V> {
        self.elems.get_mut(k.index())
    }

    /// Get the number of elements in the map.
    pub fn len(&self) -> usize {
        self.elems.len()
    }

    /// Iterate over all keys and values in the map.
    pub fn iter(&self) -> entity::Iter<K, V> {
        entity::Iter::new(self.elems.iter())
    }

    /// Iterate over all keys and velues in the map.
    pub fn iter_mut(&mut self) -> entity::IterMut<K, V> {
        entity::IterMut::new(self.elems.iter_mut())
    }

    /// Iterate over all the values.
    pub fn values(&self) -> core::slice::Iter<V> {
        self.elems.iter()
    }

    /// Iterate over all the keys.
    pub fn keys(&self) -> entity::Keys<K> {
        entity::Keys::with_len(self.len())
    }
}

/// Immutable indexing into a `FrozenMap`.
impl<K, V> Index<K> for FrozenMap<K, V>
where
    K: EntityRef,
{
    type Output = V;

    fn index(&self, k: K) -> &V {
        &self.elems[k.index()]
    }
}

/// Mutable indexing into a `FrozenMap`.
impl<K, V> IndexMut<K> for FrozenMap<K, V>
where
    K: EntityRef,
{
    fn index_mut(&mut self, k: K) -> &mut V {
        &mut self.elems[k.index()]
    }
}
