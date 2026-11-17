//! Dichte Matrix und LU-Zerlegung mit Spaltenpivotisierung.
//!
//! Lernschaltungen haben selten mehr als ein paar hundert Unbekannte, daher
//! reicht eine dichte Zerlegung völlig aus und bleibt leicht nachvollziehbar.
//! Ein dünnbesetzter Löser kann später hinter derselben Schnittstelle folgen.

/// Quadratische Matrix in Zeilenreihenfolge.
#[derive(Clone, Debug)]
pub struct Matrix {
    n: usize,
    a: Vec<f64>,
}

impl Matrix {
    pub fn zeros(n: usize) -> Self {
        Self {
            n,
            a: vec![0.0; n * n],
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.n
    }

    pub fn clear(&mut self) {
        self.a.iter_mut().for_each(|v| *v = 0.0);
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.a[r * self.n + c]
    }

    #[inline]
    pub fn add(&mut self, r: usize, c: usize, v: f64) {
        self.a[r * self.n + c] += v;
    }

    /// Größter Betrag aller Einträge (für relative Schwellen).
    pub fn max_abs(&self) -> f64 {
        self.a.iter().fold(0.0_f64, |m, v| m.max(v.abs()))
    }

    /// Löst `A x = b` in-place: `self` wird zerstört, `b` enthält danach `x`.
    ///
    /// Gibt bei (numerisch) singulärer Matrix die Zeile zurück, an der die
    /// Elimination keinen brauchbaren Pivot mehr gefunden hat.
    pub fn solve_in_place(&mut self, b: &mut [f64]) -> Result<(), usize> {
        let n = self.n;
        debug_assert_eq!(b.len(), n);
        let scale = self.max_abs().max(1e-30);
        let tiny = scale * 1e-18;
        let a = &mut self.a;

        for k in 0..n {
            // Pivotsuche in Spalte k
            let mut p = k;
            let mut best = a[k * n + k].abs();
            for r in (k + 1)..n {
                let v = a[r * n + k].abs();
                if v > best {
                    best = v;
                    p = r;
                }
            }
            if best <= tiny {
                return Err(k);
            }
            if p != k {
                for c in 0..n {
                    a.swap(k * n + c, p * n + c);
                }
                b.swap(k, p);
            }
            let piv = a[k * n + k];
            for r in (k + 1)..n {
                let f = a[r * n + k] / piv;
                if f == 0.0 {
                    continue;
                }
                a[r * n + k] = 0.0;
                for c in (k + 1)..n {
                    a[r * n + c] -= f * a[k * n + c];
                }
                b[r] -= f * b[k];
            }
        }
        // Rückwärtseinsetzen
        for k in (0..n).rev() {
            let mut s = b[k];
            for c in (k + 1)..n {
                s -= a[k * n + c] * b[c];
            }
            b[k] = s / a[k * n + k];
        }
        if b.iter().any(|v| !v.is_finite()) {
            return Err(n.saturating_sub(1));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_small_system() {
        // 2x + y = 5, x + 3y = 10  ->  x = 1, y = 3
        let mut m = Matrix::zeros(2);
        m.add(0, 0, 2.0);
        m.add(0, 1, 1.0);
        m.add(1, 0, 1.0);
        m.add(1, 1, 3.0);
        let mut b = vec![5.0, 10.0];
        m.solve_in_place(&mut b).unwrap();
        assert!((b[0] - 1.0).abs() < 1e-12);
        assert!((b[1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn needs_pivoting() {
        // 0x + y = 2, x + y = 3
        let mut m = Matrix::zeros(2);
        m.add(0, 1, 1.0);
        m.add(1, 0, 1.0);
        m.add(1, 1, 1.0);
        let mut b = vec![2.0, 3.0];
        m.solve_in_place(&mut b).unwrap();
        assert!((b[0] - 1.0).abs() < 1e-12);
        assert!((b[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn detects_singular() {
        let mut m = Matrix::zeros(2);
        m.add(0, 0, 1.0);
        m.add(0, 1, 2.0);
        m.add(1, 0, 2.0);
        m.add(1, 1, 4.0);
        let mut b = vec![1.0, 2.0];
        assert!(m.solve_in_place(&mut b).is_err());
    }
}
