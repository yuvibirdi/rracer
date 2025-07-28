/// Static passages for typing races
pub const PASSAGES: &[&str] = &[
    "The quick brown fox jumps over the lazy dog. This pangram contains every letter of the alphabet at least once.",
    "To be or not to be, that is the question: Whether 'tis nobler in the mind to suffer the slings and arrows of outrageous fortune.",
    "In the beginning was the Word, and the Word was with God, and the Word was God.",
    "It was the best of times, it was the worst of times, it was the age of wisdom, it was the age of foolishness.",
    "Call me Ishmael. Some years ago—never mind how long precisely—having little or no money in my purse.",
    "All happy families are alike; each unhappy family is unhappy in its own way.",
    "The only way to do great work is to love what you do. If you haven't found it yet, keep looking.",
    "Programming is not about typing, it's about thinking. The keyboard is just the interface between your thoughts and the computer.",
    "Rust empowers everyone to build reliable and efficient software. It prevents segfaults and guarantees thread safety.",
    "WebAssembly is a binary instruction format for a stack-based virtual machine designed as a portable compilation target."
];

/// Get a random passage for typing practice
pub fn get_random_passage() -> &'static str {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .hash(&mut hasher);
    
    let index = (hasher.finish() as usize) % PASSAGES.len();
    PASSAGES[index]
}

/// Get passage by index (for deterministic testing)
pub fn get_passage_by_index(index: usize) -> Option<&'static str> {
    PASSAGES.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passages_not_empty() {
        assert!(!PASSAGES.is_empty());
        assert!(PASSAGES.len() >= 5);
    }

    #[test]
    fn test_get_passage_by_index() {
        assert!(get_passage_by_index(0).is_some());
        assert!(get_passage_by_index(PASSAGES.len()).is_none());
    }

    #[test]
    fn test_random_passage() {
        let passage = get_random_passage();
        assert!(!passage.is_empty());
        assert!(PASSAGES.contains(&passage));
    }
}
