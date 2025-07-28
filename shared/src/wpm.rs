/// Gross words per minute (no error penalty)
pub fn gross_wpm(chars: usize, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    (chars as f64 / 5.0) / (seconds / 60.0)
}

/// Net WPM = gross – unfixed errors penalty
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
    fn test_gross_wpm() {
        // 300 chars in 60 seconds = 60 WPM
        assert_eq!(gross_wpm(300, 60.0), 60.0);
        
        // 150 chars in 30 seconds = 60 WPM
        assert_eq!(gross_wpm(150, 30.0), 60.0);
        
        // Edge case: 0 seconds
        assert_eq!(gross_wpm(100, 0.0), 0.0);
    }

    #[test]
    fn test_net_wpm() {
        // 300 chars, 60 seconds, 10 errors = 60 - 10 = 50 WPM
        assert_eq!(net_wpm(300, 60.0, 10), 50.0);
        
        // Edge case: 0 seconds
        assert_eq!(net_wpm(100, 0.0, 5), 0.0);
    }

    #[test]
    fn test_accuracy() {
        assert_eq!(accuracy(90, 100), 90.0);
        assert_eq!(accuracy(0, 0), 100.0);
        assert_eq!(accuracy(100, 100), 100.0);
    }
}
