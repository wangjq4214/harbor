//! Overlap-safe cloning without an intermediate payload array.
use std::ops::Range;

/// Like slice::copy_within, but clones each source value at most once.
pub(super) fn clone_within<T: Clone>(values: &mut [T], source: Range<usize>, dst: usize) {
    let len = values[source.clone()].len();
    let _ = &values[dst..dst + len];
    if dst < source.start {
        for offset in 0..len {
            values[dst + offset] = values[source.start + offset].clone();
        }
    } else if dst > source.start {
        for offset in (0..len).rev() {
            values[dst + offset] = values[source.start + offset].clone();
        }
    }
}

/// Copies equal-sized blocks with snapshot semantics, allowing both ranges to
/// wrap. Uses constant scratch space and clones each source element once (zero
/// times for a self-copy), even when overlap forms a cycle.
pub(super) fn clone_ring_blocks_within<T: Clone>(
    values: &mut [T],
    block_size: usize,
    src: usize,
    count: usize,
    dst: usize,
) {
    debug_assert!(block_size > 0 && values.len().is_multiple_of(block_size));
    let total = values.len() / block_size;
    debug_assert!(src < total && dst < total && count < total);
    if count == 0 || src == dst {
        return;
    }

    // In source-relative coordinates, source blocks are [0, count) and
    // each copy goes from i to (i + shift) % total.
    let shift = (dst + total - src) % total;
    let previous = |i: usize| (i + total - shift) % total;
    let physical = |i: usize| ((src + i) % total) * block_size;

    // A path ends where its destination is outside the source. Copy it
    // backwards so no write destroys a source block still needed by that path.
    for tail in 0..count {
        if (tail + shift) % total < count {
            continue;
        }
        let mut current = tail;
        let mut target = (tail + shift) % total;
        loop {
            let from = physical(current);
            let to = physical(target);
            for col in 0..block_size {
                values[to + col] = values[from + col].clone();
            }
            target = current;
            current = previous(current);
            if current >= count {
                break;
            }
        }
    }

    // Remaining components are complete cycles. A shift has gcd(total, shift)
    // cycles, one per residue modulo gcd. A cycle fits wholly in [0, count)
    // iff its largest member (residue + total - gcd) is below count.
    let (mut gcd, mut remainder) = (total, shift);
    while remainder != 0 {
        (gcd, remainder) = (remainder, gcd % remainder);
    }
    for head in 0..count.saturating_sub(total - gcd) {
        // Save just one element, not a row or the entire source range.
        for col in 0..block_size {
            let saved = values[physical(head) + col].clone();
            let mut target = head;
            let mut current = previous(target);
            while current != head {
                values[physical(target) + col] = values[physical(current) + col].clone();
                target = current;
                current = previous(current);
            }
            values[physical(target) + col] = saved;
        }
    }
}

#[cfg(test)]
mod tests;
