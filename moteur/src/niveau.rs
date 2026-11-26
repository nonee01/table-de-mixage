//! Niveaux : dB et valeurs linéaires, valeur efficace, crête.

/// dB -> facteur linéaire (−∞ dB donne 0).
pub fn db_vers_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Facteur linéaire -> dB (0 donne −∞).
pub fn lin_vers_db(x: f32) -> f32 {
    20.0 * x.abs().log10()
}

/// Valeur efficace.
pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Valeur crête (max de |x|).
pub fn crete(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, v| m.max(v.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essais;

    #[test]
    fn zero_db_vaut_un() {
        assert!((db_vers_lin(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn moins_six_db_vaut_un_demi() {
        assert!((db_vers_lin(-6.0206) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn moins_infini_vaut_zero() {
        assert_eq!(db_vers_lin(f32::NEG_INFINITY), 0.0);
        assert_eq!(lin_vers_db(0.0), f32::NEG_INFINITY);
    }

    #[test]
    fn rms_sinus_pleine_echelle() {
        let x = essais::sinus(1000.0, 1.0, 4800);
        assert!((lin_vers_db(rms(&x)) + 3.01).abs() < 0.05);
    }

    #[test]
    fn crete_prend_la_valeur_absolue() {
        assert_eq!(crete(&[0.1, -0.7, 0.3]), 0.7);
    }
}
