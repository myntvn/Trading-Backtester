/// Exponential moving average, aligned index-for-index with `values`.
///
/// Returns `None` for indices before enough data exists to seed the average.
/// The seed at index `period - 1` is the simple mean of the first `period`
/// values; every index after that uses the standard recurrence.
pub fn ema(values: &[f64], period: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; values.len()];

    if period == 0 || period > values.len() {
        return out;
    }

    let k = 2.0 / (period as f64 + 1.0);

    let mut prev = values.iter().take(period).sum::<f64>() / period as f64;
    out[period - 1] = Some(prev);

    for i in period..values.len() {
        prev = (values[i] - prev) * k + prev;
        out[i] = Some(prev);
    }

    out
}

#[cfg(test)]
mod tests {

    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn matches_hand_computed_values() {
        // k = 2/4 = 0.5
        // ema[2] = mean(1,2,3)   = 2.0
        // ema[3] = (4-2)*0.5 + 2 = 3.0
        // ema[4] = (5-3)*0.5 + 3 = 4.0
        let out = ema(&[1.0, 2.0, 3.0, 4.0, 5.0], 3);
        assert_eq!(out, vec!(None, None, Some(2.0), Some(3.0), Some(4.0)));
    }

    #[test]
    fn period_one_is_the_input_itself() {
        let values = [3.0, 1.0, 4.0, 1.0, 5.0];
        let out = ema(&values, 1);

        for (got, want) in out.iter().zip(values.iter()) {
            assert!(approx(got.expect("period 1 is never None"), *want));
        }
    }

    #[test]
    fn period_longer_than_input_is_all_none() {
        let out = ema(&[1.0, 2.0], 5);
        assert_eq!(out, vec![None, None]);
    }

    #[test]
    fn empty_input_is_empty_output() {
        assert!(ema(&[], 3).is_empty());
        assert!(ema(&[], 0).is_empty());
    }

    #[test]
    fn output_length_always_matches_input() {
        let values = [1.0; 100];
        for period in [0, 1, 7, 99, 100, 101] {
            assert_eq!(ema(&values, period).len(), values.len());
        }
    }
}
