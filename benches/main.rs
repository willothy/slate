use bencher::{benchmark_group, benchmark_main, Bencher};
use tmpkey::Slab;

benchmark_group!(primary, insert, insert_and_get, insert_and_remove);
benchmark_group!(
    comparison,
    slotmap_insert,
    slotmap_insert_and_get,
    slotmap_insert_and_remove
);
benchmark_main!(primary, comparison);

fn insert(b: &mut Bencher) {
    let mut map = Slab::default();
    b.iter(|| {
        map.insert(5);
    });
}

fn insert_and_get(b: &mut Bencher) {
    let mut map = Slab::default();
    b.iter(|| {
        let k = map.insert(5);
        map.get(k);
    });
}

fn insert_and_remove(b: &mut Bencher) {
    let mut map = Slab::default();
    b.iter(|| {
        let k = map.insert(5);
        map.remove(k);
    });
}

fn slotmap_insert(b: &mut Bencher) {
    let mut map = slotmap::SlotMap::new();
    b.iter(|| map.insert(5));
}

fn slotmap_insert_and_get(b: &mut Bencher) {
    let mut map = slotmap::SlotMap::new();
    b.iter(|| {
        let k = map.insert(5);
        map.get(k);
    });
}

fn slotmap_insert_and_remove(b: &mut Bencher) {
    let mut map = slotmap::SlotMap::new();
    b.iter(|| {
        let k = map.insert(5);
        map.remove(k);
    });
}
