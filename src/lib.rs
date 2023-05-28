use std::{fmt::Debug, iter::FilterMap, num::NonZeroU32, ops::Not};

pub struct KeyData<'a, T> {
    index: u32,
    version: NonZeroU32,
    __phantom: std::marker::PhantomData<&'a T>,
}

impl<'a, T> PartialEq for KeyData<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.version == other.version
    }
}

impl<'a, T> Eq for KeyData<'a, T> {}

impl<'a, T> PartialOrd for KeyData<'a, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.index.partial_cmp(&other.index) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.version.partial_cmp(&other.version)
    }
}

impl<'a, T> Ord for KeyData<'a, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.index.cmp(&other.index) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.version.cmp(&other.version)
    }
}

impl<'a, T> Clone for KeyData<'a, T> {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            version: self.version,
            __phantom: self.__phantom,
        }
    }
}

impl<'a, T> Copy for KeyData<'a, T> {}

impl<'a, T> Debug for KeyData<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<{}, {}v{}>",
            std::any::type_name::<T>(),
            self.index,
            self.version
        )
    }
}

pub struct Slot<T> {
    version: NonZeroU32,
    value: Option<T>,
}

impl<T> Slot<T> {
    pub const fn new() -> Self {
        Self {
            version: unsafe { NonZeroU32::new_unchecked(1) },
            value: None,
        }
    }

    #[inline(always)]
    pub fn occupied(&self) -> bool {
        self.version.get() % 2 == 0
    }

    #[inline(always)]
    pub fn vacant(&self) -> bool {
        self.version.get() % 2 != 0
    }

    pub fn older_than(&self, version: NonZeroU32) -> bool {
        self.version < version
    }

    pub fn newer_than(&self, version: NonZeroU32) -> bool {
        self.version > version
    }

    pub fn same_version(&self, version: NonZeroU32) -> bool {
        self.version == version
    }

    pub fn update(&mut self, value: T) -> Option<T> {
        self.version = self.version.checked_add(1).unwrap();
        self.value.replace(value)
    }

    pub fn swap(&mut self, value: T) -> Option<T> {
        self.value.replace(value)
    }

    pub fn vacate(&mut self) -> Option<T> {
        if self.vacant() {
            None
        } else {
            self.version = self.version.checked_add(1).unwrap();
            self.value.take()
        }
    }
}

pub trait Key<T> {
    fn data(&self) -> KeyData<T>;
    fn init(version: NonZeroU32, idx: u32) -> Self;

    fn same_version(&self, other: &Self) -> bool {
        self.version() == other.version()
    }

    fn index(&self) -> u32 {
        self.data().index
    }

    fn version(&self) -> NonZeroU32 {
        self.data().version
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DefaultKey<'a, T> {
    data: KeyData<'a, T>,
}

impl<'a, T> Key<T> for DefaultKey<'a, T> {
    fn data(&self) -> KeyData<'a, T> {
        self.data
    }

    fn init(version: NonZeroU32, index: u32) -> Self {
        Self {
            data: KeyData {
                index,
                version,
                __phantom: std::marker::PhantomData,
            },
        }
    }
}

pub struct Slab<K, V>
where
    K: Key<V>,
{
    values: Vec<Slot<V>>,
    free: Vec<u32>,
    taken: u32,
    __phantom: std::marker::PhantomData<K>,
}

impl<'a, V> Default for Slab<DefaultKey<'a, V>, V> {
    fn default() -> Self {
        Self {
            values: vec![],
            free: vec![],
            taken: 0,
            __phantom: std::marker::PhantomData,
        }
    }
}

pub struct AccessKey<'a, K, V>
where
    K: Key<V> + Clone,
{
    key: K,
    table: *const Slab<K, V>,
    __phantom: std::marker::PhantomData<&'a V>,
}

impl<'a, K, V> Clone for AccessKey<'a, K, V>
where
    K: Key<V> + Clone,
{
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            table: self.table,
            __phantom: self.__phantom,
        }
    }
}

impl<'a, K, V> AccessKey<'a, K, V>
where
    K: Key<V> + Clone,
{
    pub fn new(key: K, table: &Slab<K, V>) -> Self {
        Self {
            key,
            table: table as *const Slab<K, V>,
            __phantom: std::marker::PhantomData,
        }
    }

    pub fn get(&self) -> Option<&V> {
        let table = unsafe { &*self.table };
        let slot = &table.values[self.data().index as usize];
        if slot.version == self.data().version {
            slot.value.as_ref()
        } else {
            None
        }
    }
}

impl<'a, K, V> Key<V> for AccessKey<'a, K, V>
where
    K: Key<V> + Clone,
{
    fn data(&self) -> KeyData<V> {
        self.key.data()
    }

    fn init(_: NonZeroU32, _: u32) -> Self {
        panic!("init should not be used for AccessKey")
    }
}

impl<'a, K, V> Debug for AccessKey<'a, K, V>
where
    K: Key<V> + Clone + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessKey").field("key", &self.key).finish()
    }
}

impl<K: Key<V> + Clone, V> Slab<K, V> {
    pub fn insert<'a>(&mut self, value: V) -> K
    where
        K: Key<V>,
    {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.values[index as usize];
            slot.value = Some(value);
            slot.version = slot.version.saturating_add(1);
            self.taken += 1;
            K::init(slot.version, index)
        } else {
            let index = self.values.len() as u32;
            let version = unsafe { NonZeroU32::new_unchecked(2) };
            self.values.push(Slot {
                version,
                value: Some(value),
            });
            self.taken += 1;
            K::init(version, index)
        }
    }

    pub fn insert_with_access<'a>(&mut self, value: V) -> AccessKey<K, V> {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.values[index as usize];
            slot.value = Some(value);
            slot.version = slot.version.saturating_add(1);
            self.taken += 1;
            AccessKey::new(K::init(slot.version, index), self)
        } else {
            let index = self.values.len() as u32;
            let version = unsafe { NonZeroU32::new_unchecked(2) };
            self.values.push(Slot {
                version,
                value: Some(value),
            });
            self.taken += 1;
            AccessKey::new(K::init(version, index), self)
        }
    }

    pub fn new() -> Self {
        Self {
            values: vec![],
            free: vec![],
            taken: 0,
            __phantom: std::marker::PhantomData,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            free: Vec::new(),
            taken: 0,
            __phantom: std::marker::PhantomData,
        }
    }

    pub fn remove(&mut self, key: K) -> Option<V> {
        let slot = &mut self.values[key.index() as usize];
        slot.same_version(key.version())
            .then(|| {
                let value = slot.value.take();
                self.free.push(key.index());
                self.taken -= 1;
                slot.version = slot.version.saturating_add(1);
                value
            })
            .flatten()
    }

    pub fn get(&self, key: K) -> Option<&V> {
        let slot = &self.values[key.index() as usize];
        slot.same_version(key.version())
            .then(|| slot.value.as_ref())
            .flatten()
    }

    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        let slot = &mut self.values[key.index() as usize];
        slot.same_version(key.version())
            .then(|| slot.value.as_mut())
            .flatten()
    }

    pub fn len(&self) -> usize {
        self.taken as usize
    }

    pub fn is_empty(&self) -> bool {
        self.taken == 0
    }

    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }

    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.values.iter().enumerate().filter_map(|(i, v)| {
            v.occupied()
                .then(|| (K::init(v.version, i as u32), v.value.as_ref().unwrap()))
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (K, &mut V)> {
        self.values.iter_mut().enumerate().filter_map(|(i, v)| {
            v.occupied()
                .then(|| (K::init(v.version, i as u32), v.value.as_mut().unwrap()))
        })
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.values
            .iter()
            .filter_map(|v| v.vacant().then(|| v.value.as_ref()).flatten())
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.values
            .iter_mut()
            .filter_map(|v| v.vacant().then(|| v.value.as_mut()))
            .flatten()
    }

    pub fn clear(&mut self) {
        self.values.iter_mut().enumerate().for_each(|(i, v)| {
            v.occupied().then(|| {
                v.version = v.version.saturating_add(1);
                v.value.take();
                self.free.push(i as u32);
            });
        });
        self.taken = 0;
    }

    pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, mut f: F) {
        let freed = self
            .values
            .iter_mut()
            .enumerate()
            .filter_map(|(i, v)| {
                v.occupied().then(|| {
                    let key = K::init(v.version, i as u32);
                    f(&key, v.value.as_mut().unwrap()).not().then(|| {
                        self.free.push(i as u32);
                        v.vacate()
                    })
                })
            })
            .count();
        self.taken = (self.values.len() - freed) as u32;
    }
}

impl<K: Key<V>, V> IntoIterator for Slab<K, V> {
    type Item = V;

    type IntoIter = FilterMap<std::vec::IntoIter<Slot<V>>, Box<dyn FnMut(Slot<V>) -> Option<V>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter().filter_map(Box::new(|v: Slot<V>| {
            if v.version.get() % 2 == 0 {
                v.value
            } else {
                None
            }
        }))
    }
}

impl<K: Key<V> + Clone, V: Debug> Debug for Slab<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct(&format!(
            "Slab<{}, {}>",
            std::any::type_name::<K>(),
            std::any::type_name::<V>()
        ));
        self.values
            .iter()
            .enumerate()
            .filter(|(_, v)| v.version.get() % 2 == 0)
            .for_each(|(i, v)| {
                dbg.field(&format!("{}v{}", i, v.version), v.value.as_ref().unwrap());
            });
        dbg.finish()
    }
}

pub struct AssociatedData<K: Key<N>, V, N> {
    items: Vec<Slot<V>>,
    taken: u32,
    __phantom: std::marker::PhantomData<(K, N)>,
}

impl<K: Key<N>, V: Debug, N> Debug for AssociatedData<K, V, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct(&format!(
            "AssociatedData<{}, {}>",
            std::any::type_name::<K>(),
            std::any::type_name::<V>()
        ));
        self.items
            .iter()
            .enumerate()
            .filter(|(_, v)| v.version.get() % 2 == 0)
            .for_each(|(i, v)| {
                dbg.field(&format!("{}v{}", i, v.version), v.value.as_ref().unwrap());
            });
        dbg.finish()
    }
}

impl<K: Key<N>, V, N> AssociatedData<K, V, N> {
    pub fn new() -> Self {
        Self {
            items: vec![],
            taken: 0,
            __phantom: std::marker::PhantomData,
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let data = key.data();
        let index = data.index as usize;
        if index >= self.items.len() {
            self.items
                .extend((self.items.len()..=index as usize).map(|_| Slot::new()));
        }
        let slot = &mut self.items[index];
        if slot.vacant() {
            self.taken += 1;
        } else if slot.same_version(data.version) {
            return slot.swap(value);
        } else if slot.newer_than(data.version) {
            // Don't replace newer versions
            return None;
        }
        slot.version = data.version;
        slot.value = Some(value);
        None
    }

    pub fn remove(&mut self, key: K) -> Option<V> {
        let data = key.data();
        let index = data.index as usize;
        if index >= self.items.len() {
            return None;
        }
        let slot = &mut self.items[index];
        if slot.occupied() && slot.same_version(data.version) {
            self.taken -= 1;
            return slot.vacate();
        }
        None
    }

    pub fn get(&self, key: K) -> Option<&V> {
        let data = key.data();
        let index = data.index as usize;
        if index >= self.items.len() {
            return None;
        }
        let slot = &self.items[index];
        if slot.occupied() && slot.same_version(data.version) {
            return slot.value.as_ref();
        }
        None
    }

    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        let data = key.data();
        let index = data.index as usize;
        if index >= self.items.len() {
            return None;
        }
        let slot = &mut self.items[index];
        if slot.occupied() && slot.same_version(data.version) {
            return slot.value.as_mut();
        }
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.items.iter().enumerate().filter_map(|(i, v)| {
            v.occupied().then(|| {
                let key = K::init(v.version, i as u32);
                (key, v.value.as_ref().unwrap())
            })
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (K, &mut V)> {
        self.items.iter_mut().enumerate().filter_map(|(i, v)| {
            v.occupied().then(|| {
                let key = K::init(v.version, i as u32);
                (key, v.value.as_mut().unwrap())
            })
        })
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.items.iter().filter_map(|v| v.value.as_ref())
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.items.iter_mut().filter_map(|v| v.value.as_mut())
    }

    pub fn len(&self) -> usize {
        self.taken as usize
    }

    pub fn is_empty(&self) -> bool {
        self.taken == 0
    }

    pub fn clear(&mut self) {
        self.items.iter_mut().for_each(|v| {
            v.value.take();
            v.version = v.version.saturating_add(1);
        });
        self.taken = 0;
    }

    pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, mut f: F) {
        let freed = self
            .items
            .iter_mut()
            .enumerate()
            .filter_map(|(i, v)| {
                v.occupied().then(|| {
                    let key = K::init(v.version, i as u32);
                    f(&key, v.value.as_mut().unwrap()).not().then(|| {
                        v.vacate();
                        1
                    })
                })
            })
            .count();
        self.taken = (self.items.len() - freed) as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn associated() {
        let mut map = Slab::default();
        let mut associated = AssociatedData::new();
        let mut keys = vec![];
        (0..10).for_each(|i| {
            let k = map.insert(i);
            if i % 2 == 0 {
                associated.insert(k, map.len());
            }
            keys.push(k);
        });
        assert_eq!(associated.len(), 5, "{associated:?}");
        assert_eq!(associated.get(keys[0]), Some(&1), "{associated:?}");
        assert!(false, "\n{associated:#?}\n{map:#?}")
    }

    #[test]
    fn into_iter() {
        let mut map = Slab::default();
        (0..10).for_each(|i| {
            map.insert(i);
        });
        let mut iter = map.into_iter();
        assert_eq!(iter.next(), Some(0));
    }

    #[test]
    fn iter() {
        let mut map = Slab::default();
        (0..10).for_each(|i| {
            map.insert(i);
        });
        let mut iter = map.iter();
        assert_eq!(iter.next().map(|v| v.1), Some(&0), "{map:?}");
    }

    #[test]
    fn clear() {
        let mut map = Slab::default();
        (0..10).for_each(|i| {
            map.insert(i);
        });
        assert_eq!(map.capacity(), 16, "{map:?}");
        map.clear();
        assert_eq!(map.len(), 0, "{map:?}");
        assert_eq!(map.capacity(), 16, "{map:?}");
        (0..10).for_each(|i| {
            map.insert(i);
        });
        assert_eq!(map.len(), 10, "{map:?}");
        assert_eq!(map.capacity(), 16, "{map:?}");
    }

    #[test]
    fn access_key() {
        let mut map = Slab::default();
        let _k = map.insert_with_access(5);
    }

    #[test]
    fn it_works() {
        let mut map = Slab::default();
        let (k, v) = {
            let k = map.insert(5);
            (k, map.get(k))
        };
        assert_eq!(v, Some(&5));
        assert_eq!(map.remove(k), Some(5));
        assert_eq!(map.get(k), None);
    }

    #[test]
    fn insert_many() {
        let mut map = Slab::default();
        for i in 0..1000 {
            let k = map.insert(i);
            assert_eq!(map.get(k), Some(&i));
        }
    }

    #[test]
    fn remove_many() {
        let mut map = Slab::default();
        let mut keys = vec![];
        for i in 0..1000 {
            let k = map.insert(i);
            keys.push(k);
        }
        for k in keys {
            assert_eq!(map.remove(k), Some(k.index()));
        }
    }

    #[test]
    fn reuse_slots() {
        let mut map = Slab::default();
        let mut keys = vec![];
        for i in 0..1000 {
            let k = map.insert(i);
            keys.push(k);
        }
        for k in keys {
            assert_eq!(map.remove(k), Some(k.index()));
        }
        for i in 999..=0 {
            let k = map.insert(i);
            assert_eq!(k.index(), i);
        }
    }
}
