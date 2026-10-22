//! Fixture: passes `cargo test` natively but has undefined behaviour that
//! only Miri reports (a read one element past the slice; the backing Vec is longer, so
//! it does not crash natively, but the slice reference has no provenance there).

pub fn sum_with_off_by_one(v: &[u32]) -> u32 {
    let mut s = 0;
    for i in 0..=v.len() {
        // SAFETY: deliberately wrong (i == len is out of bounds); seeded for Miri.
        s += unsafe { *v.as_ptr().add(i) } & 0;
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn looks_fine_natively() {
        let v = vec![1u32, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(super::sum_with_off_by_one(&v[..4]), 0);
    }
}
