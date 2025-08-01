/// Monkeytype-style WPM calculation: (correct_chars / 5) * (60 / seconds)
/// This calculates effective typing speed based only on correct characters
pub fn wpm(correct_chars: usize, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    (correct_chars as f64 / 5.0) * (60.0 / seconds)
}

/// Raw WPM calculation: (total_chars / 5) * (60 / seconds)
/// This includes both correct and incorrect characters
pub fn raw_wpm(total_chars: usize, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    (total_chars as f64 / 5.0) * (60.0 / seconds)
}

/// Legacy gross WPM function (kept for compatibility)
pub fn gross_wpm(chars: usize, seconds: f64) -> f64 {
    wpm(chars, seconds)
}

/// Legacy net WPM function (kept for compatibility)
/// Note: This is different from Monkeytype's approach
pub fn net_wpm(chars: usize, seconds: f64, errors: usize) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    gross_wpm(chars, seconds) - errors as f64 * 60.0 / seconds
}

/// Calculate accuracy percentage
pub fn accuracy(correct_chars: usize, total_chars: usize) -> f64 {
    if total_chars == 0 {
        return 100.0;
    }
    (correct_chars as f64 / total_chars as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wpm() {
        // 300 correct chars in 60 seconds = 60 WPM
        assert_eq!(wpm(300, 60.0), 60.0);
        
        // 150 correct chars in 30 seconds = 60 WPM
        assert_eq!(wpm(150, 30.0), 60.0);
        
        // 100 correct chars in 20 seconds = 60 WPM
        assert_eq!(wpm(100, 20.0), 60.0);
        
        // Edge case: 0 seconds
        assert_eq!(wpm(100, 0.0), 0.0);
    }

    #[test]
    fn test_raw_wpm() {
        // 350 total chars (300 correct + 50 errors) in 60 seconds = 70 raw WPM
        assert_eq!(raw_wpm(350, 60.0), 70.0);
        
        // 175 total chars in 30 seconds = 70 raw WPM
        assert_eq!(raw_wpm(175, 30.0), 70.0);
        
        // Edge case: 0 seconds
        assert_eq!(raw_wpm(100, 0.0), 0.0);
    }

    #[test]
    fn test_monkeytype_example() {
        // Example: 240 correct chars in 48 seconds
        // WPM = (240 / 5) * (60 / 48) = 48 * 1.25 = 60 WPM
        assert_eq!(wpm(240, 48.0), 60.0);
        
        // If there were 260 total chars attempted (240 correct + 20 errors)
        // Raw WPM = (260 / 5) * (60 / 48) = 52 * 1.25 = 65 raw WPM
        assert_eq!(raw_wpm(260, 48.0), 65.0);
        
        // Accuracy = 240 / 260 = 92.3%
        assert!((accuracy(240, 260) - 92.30769230769231).abs() < 0.0001);
    }

    #[test]
    fn test_accuracy() {
        assert_eq!(accuracy(90, 100), 90.0);
        assert_eq!(accuracy(0, 0), 100.0);
        assert_eq!(accuracy(100, 100), 100.0);
        assert_eq!(accuracy(240, 260), 240.0 / 260.0 * 100.0); // ~92.31%
    }
}
