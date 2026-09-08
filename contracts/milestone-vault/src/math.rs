/// Denominator for basis-points fractions (10_000 bps = 100%).
const BPS_DENOMINATOR: i128 = 10_000;

/// Computes the payout for a tranche worth `payout_bps` (out of 10,000) of
/// `total_deposited`, saturating instead of overflowing/panicking if the
/// multiplication would exceed i128's range, and capped so it can never
/// exceed `total_deposited` even if `payout_bps` were ever misconfigured
/// above 100%.
pub fn tranche_payout(total_deposited: i128, payout_bps: u32) -> i128 {
    if total_deposited <= 0 || payout_bps == 0 {
        return 0;
    }

    let scaled = total_deposited.saturating_mul(payout_bps as i128);
    let payout = scaled / BPS_DENOMINATOR;
    payout.min(total_deposited)
}

#[cfg(test)]
mod test {
    use super::tranche_payout;

    #[test]
    fn zero_deposited_pays_nothing() {
        assert_eq!(tranche_payout(0, 3_000), 0);
    }

    #[test]
    fn negative_deposited_pays_nothing() {
        assert_eq!(tranche_payout(-100, 3_000), 0);
    }

    #[test]
    fn zero_bps_pays_nothing() {
        assert_eq!(tranche_payout(1_000, 0), 0);
    }

    #[test]
    fn computes_fraction_of_deposited() {
        assert_eq!(tranche_payout(1_000, 3_000), 300); // 30%
        assert_eq!(tranche_payout(1_000, 10_000), 1_000); // 100%
    }

    #[test]
    fn rounds_down_on_fractional_bps_share() {
        // 33.33% of 999 is 332.9967, which truncates to 332.
        assert_eq!(tranche_payout(999, 3_333), 332);
    }

    #[test]
    fn caps_at_total_deposited_even_over_100_percent_bps() {
        // Shouldn't happen given schedule validation, but the math itself
        // must never let a misconfigured bps value pay out more than what
        // was actually deposited.
        assert_eq!(tranche_payout(1_000, 15_000), 1_000);
    }

    #[test]
    fn saturates_instead_of_overflowing() {
        assert_eq!(tranche_payout(i128::MAX, 10_000), i128::MAX);

        let payout = tranche_payout(i128::MAX, 5_000);
        assert!(payout > 0);
        assert!(payout <= i128::MAX);
    }

    #[test]
    fn invariants_hold_across_a_grid_of_inputs() {
        let deposited_values = [0i128, 1, 999, 1_000_000, i128::MAX];
        let bps_values = [0u32, 1, 2_500, 5_000, 10_000, 15_000, u32::MAX];

        for &deposited in &deposited_values {
            let mut prev = 0i128;
            for &bps in &bps_values {
                let payout = tranche_payout(deposited, bps);

                // Never negative, never more than what was deposited.
                assert!(payout >= 0, "negative payout: deposited={deposited} bps={bps}");
                assert!(
                    payout <= deposited.max(0),
                    "payout exceeds deposited: deposited={deposited} bps={bps}"
                );

                // More bps never pays out less (monotonic non-decreasing).
                assert!(
                    payout >= prev,
                    "payout decreased as bps grew: deposited={deposited} bps={bps}"
                );
                prev = payout;
            }
        }
    }
}
