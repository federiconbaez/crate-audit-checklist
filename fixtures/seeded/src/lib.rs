//! Fixture crate with seeded findings: a clippy lint, formatting drift,
//! unjustified unsafe and a failing test.

pub struct Handle(*mut u8);

unsafe impl Send for Handle {}

pub unsafe fn read(p: *const u8) -> u8 {
    *p
}

pub fn twice(x: u32) -> u32 {
    return x * 2;
}

pub fn peek(v: &[u8]) -> u8 {
    unsafe { *v.as_ptr() }
}

pub fn   badly_formatted( ) -> u8 { 1 }

#[cfg(test)]
mod tests {
    #[test]
    fn passes() {
        assert_eq!(super::twice(2), 4);
    }

    #[test]
    fn seeded_failure() {
        assert_eq!(super::twice(2), 5, "seeded failing test");
    }
}
