use super::*;
use std::cell::Cell;
use std::rc::Rc;

struct Counted {
    original: usize,
    clones: Rc<Vec<Cell<usize>>>,
    suffix: String,
}

impl Clone for Counted {
    fn clone(&self) -> Self {
        let counter = &self.clones[self.original];
        counter.set(counter.get() + 1);
        Self {
            original: self.original,
            clones: Rc::clone(&self.clones),
            suffix: self.suffix.clone(),
        }
    }
}

fn fixture(len: usize) -> (Vec<Counted>, Rc<Vec<Cell<usize>>>) {
    let counters = Rc::new((0..len).map(|_| Cell::new(0)).collect::<Vec<_>>());
    let values = (0..len)
        .map(|original| Counted {
            original,
            clones: Rc::clone(&counters),
            suffix: format!("\u{301}{original}\u{200d}\u{1f680}"),
        })
        .collect();
    (values, counters)
}

fn assert_values_and_clones(
    values: &[Counted],
    expected: &[usize],
    counters: &[Cell<usize>],
    copied: &[bool],
) {
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.original, *expected);
        assert_eq!(value.suffix, format!("\u{301}{expected}\u{200d}\u{1f680}"));
    }
    for (counter, copied) in counters.iter().zip(copied) {
        assert_eq!(counter.get(), usize::from(*copied), "one clone per source");
    }
}

#[test]
fn linear_copy_matches_snapshot_and_clones_each_source_at_most_once() {
    for len in 0..=10 {
        for start in 0..=len {
            for end in start..=len {
                let count = end - start;
                for dst in 0..=len - count {
                    let (mut values, counters) = fixture(len);
                    let mut expected: Vec<_> = (0..len).collect();
                    expected[dst..dst + count].copy_from_slice(&(start..end).collect::<Vec<_>>());
                    clone_within(&mut values, start..end, dst);
                    let copied: Vec<_> = (0..len)
                        .map(|i| dst != start && (start..end).contains(&i))
                        .collect();
                    assert_values_and_clones(&values, &expected, &counters, &copied);
                }
            }
        }
    }
}

#[test]
fn ring_copy_matches_snapshot_and_clones_each_source_at_most_once() {
    // Exhaust all source/destination wraps, both overlap directions, empty and
    // self copies, disjoint copies, paths, and complete dependency cycles.
    for total in 1..=12 {
        for block_size in 1..=3 {
            for src in 0..total {
                for count in 0..total {
                    for dst in 0..total {
                        let (mut values, counters) = fixture(total * block_size);
                        let mut expected: Vec<_> = (0..values.len()).collect();
                        let mut copied = vec![false; values.len()];
                        for offset in 0..count {
                            let from = ((src + offset) % total) * block_size;
                            let to = ((dst + offset) % total) * block_size;
                            for col in 0..block_size {
                                expected[to + col] = from + col;
                                copied[from + col] = src != dst;
                            }
                        }
                        clone_ring_blocks_within(&mut values, block_size, src, count, dst);
                        assert_values_and_clones(&values, &expected, &counters, &copied);
                    }
                }
            }
        }
    }
}
