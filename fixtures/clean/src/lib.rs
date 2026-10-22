//! Fixture crate with one justified unsafe block.

/// Returns the first byte of a non-empty slice.
pub fn first(v: &[u8]) -> Option<u8> {
    if v.is_empty() {
        return None;
    }
    // SAFETY: the slice is non-empty, so index 0 is in bounds.
    Some(unsafe { *v.get_unchecked(0) })
}

#[cfg(test)]
mod tests {
    #[test]
    fn first_byte() {
        assert_eq!(super::first(b"ab"), Some(b'a'));
        assert_eq!(super::first(b""), None);
    }
}
