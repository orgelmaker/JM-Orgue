//! Temperament definitions
//!
//! Historical and modern tuning temperaments for organ pipes.

use serde::{Deserialize, Serialize};
use crate::Frequency;

/// A tuning temperament
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Temperament {
    /// Temperament name
    pub name: String,
    
    /// Description
    pub description: String,
    
    /// Cent deviations from equal temperament for each pitch class
    /// Index 0 = C, 1 = C#, 2 = D, etc.
    pub cents: [f32; 12],
}

impl Temperament {
    /// Equal temperament (modern standard)
    pub fn equal() -> Self {
        Self {
            name: "Equal Temperament".to_string(),
            description: "Modern equal temperament with all semitones equal".to_string(),
            cents: [0.0; 12],
        }
    }
    
    /// Werkmeister III (common baroque temperament)
    pub fn werkmeister_iii() -> Self {
        Self {
            name: "Werkmeister III".to_string(),
            description: "Andreas Werckmeister's third temperament (1691)".to_string(),
            cents: [
                0.0,    // C
                -10.0,  // C#
                -8.0,   // D
                -6.0,   // D#
                -10.0,  // E
                -2.0,   // F
                -12.0,  // F#
                -4.0,   // G
                -8.0,   // G#
                -12.0,  // A
                -4.0,   // A#
                -8.0,   // B
            ],
        }
    }
    
    /// Kirnberger III
    pub fn kirnberger_iii() -> Self {
        Self {
            name: "Kirnberger III".to_string(),
            description: "Johann Kirnberger's third temperament".to_string(),
            cents: [
                0.0,    // C
                -10.0,  // C#
                -7.0,   // D
                -3.0,   // D#
                -14.0,  // E
                2.0,    // F
                -12.0,  // F#
                -4.0,   // G
                -7.0,   // G#
                -10.5,  // A
                0.0,    // A#
                -10.0,  // B
            ],
        }
    }
    
    /// Vallotti temperament
    pub fn vallotti() -> Self {
        Self {
            name: "Vallotti".to_string(),
            description: "Francesco Vallotti's temperament (18th century)".to_string(),
            cents: [
                0.0,    // C
                -6.0,   // C#
                -4.0,   // D
                -2.0,   // D#
                -8.0,   // E
                2.0,    // F
                -8.0,   // F#
                -2.0,   // G
                -4.0,   // G#
                -6.0,   // A
                0.0,    // A#
                -6.0,   // B
            ],
        }
    }
    
    /// Meantone temperament (1/4 comma)
    pub fn meantone_quarter() -> Self {
        Self {
            name: "1/4 Comma Meantone".to_string(),
            description: "Quarter-comma meantone temperament".to_string(),
            cents: [
                0.0,    // C
                -24.0,  // C#
                -7.0,   // D
                10.0,   // D#
                -14.0,  // E
                3.0,    // F
                -21.0,  // F#
                -3.0,   // G
                -28.0,  // G#
                -10.0,  // A
                7.0,    // A#
                -17.0,  // B
            ],
        }
    }
    
    /// Young temperament (Thomas Young, 1799)
    pub fn young() -> Self {
        Self {
            name: "Young".to_string(),
            description: "Thomas Young's well temperament (1799)".to_string(),
            cents: [
                0.0,    // C
                -6.0,   // C#
                -4.0,   // D
                -2.0,   // D#
                -8.0,   // E
                0.0,    // F
                -6.0,   // F#
                -2.0,   // G
                -4.0,   // G#
                -6.0,   // A
                0.0,    // A#
                -6.0,   // B
            ],
        }
    }
    
    /// Get the cent deviation for a pitch class (0-11)
    pub fn get_cents(&self, pitch_class: u8) -> f32 {
        self.cents[(pitch_class % 12) as usize]
    }
    
    /// Calculate the frequency ratio for a pitch class
    pub fn get_ratio(&self, pitch_class: u8) -> f32 {
        let cents = self.get_cents(pitch_class);
        2.0_f32.powf(cents / 1200.0)
    }
    
    /// Apply temperament to a frequency
    pub fn apply(&self, freq: Frequency, pitch_class: u8) -> Frequency {
        freq * self.get_ratio(pitch_class)
    }
}

impl Default for Temperament {
    fn default() -> Self {
        Self::equal()
    }
}

/// Common historical temperaments
pub fn available_temperaments() -> Vec<Temperament> {
    vec![
        Temperament::equal(),
        Temperament::werkmeister_iii(),
        Temperament::kirnberger_iii(),
        Temperament::vallotti(),
        Temperament::meantone_quarter(),
        Temperament::young(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_temperament() {
        let temp = Temperament::equal();
        for i in 0..12 {
            assert_eq!(temp.get_cents(i), 0.0);
        }
    }

    #[test]
    fn test_temperament_ratio() {
        let temp = Temperament::equal();
        let ratio = temp.get_ratio(0);
        assert!((ratio - 1.0).abs() < 0.001);
    }
}
