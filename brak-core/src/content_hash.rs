pub trait ContentHash {
    fn content_hash(&self) -> u64;
}

impl ContentHash for u64 {
    fn content_hash(&self) -> u64 {
        *self
    }
}

pub fn combine_hash(a: u64, b: u64) -> u64 {
    a.wrapping_mul(6364136223846793005).wrapping_add(b.wrapping_add(1))
}

impl ContentHash for String {
    fn content_hash(&self) -> u64 {
        let mut h: u64 = 0;
        for b in self.bytes() {
            h = combine_hash(h, b as u64);
        }
        h
    }
}

impl ContentHash for &String {
    fn content_hash(&self) -> u64 {
        let mut h: u64 = 0;
        for b in self.bytes() {
            h = combine_hash(h, b as u64);
        }
        h
    }
}

impl ContentHash for &str {
    fn content_hash(&self) -> u64 {
        let mut h: u64 = 0;
        for b in self.bytes() {
            h = combine_hash(h, b as u64);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_u64() {
        assert_eq!(42u64.content_hash(), 42);
    }

    #[test]
    fn test_content_hash_string() {
        let a = "hello".to_string();
        let b = "hello".to_string();
        let c = "world".to_string();
        assert_eq!(a.content_hash(), b.content_hash());
        assert_ne!(a.content_hash(), c.content_hash());
    }

    #[test]
    fn test_combine_hash_deterministic() {
        let h1 = combine_hash(100, 200);
        let h2 = combine_hash(100, 200);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_combine_hash_different() {
        let h1 = combine_hash(100, 200);
        let h2 = combine_hash(100, 201);
        assert_ne!(h1, h2);
    }
}
