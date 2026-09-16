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

    // Split the multiply around the division so a large deposit doesn't
    // saturate and silently lose the fraction: t*b/d == (t/d)*b + (t%d)*b/d,
    // exactly, and the left term is the only one that can overflow.
    let bps = payout_bps as i128;
    let whole = (total_deposited / BPS_DENOMINATOR).saturating_mul(bps);
    let rest = (total_deposited % BPS_DENOMINATOR) * bps / BPS_DENOMINATOR;
    let payout = whole.saturating_add(rest);
    payout.min(total_deposited)
}

/// Computes `part`'s proportional share of `remaining`, where `part` is
/// out of `total`. Used to split what's left in a cancelled vault fairly
/// across donors based on how much each contributed, regardless of the
/// order they claim their refund in.
///
/// Returns 0 if any input is non-positive, and saturates instead of
/// overflowing/panicking if `part * remaining` would exceed i128's range.
/// Capped at `remaining` so rounding can never hand out more than what's
/// actually left to refund.
pub fn proportional_share(part: i128, remaining: i128, total: i128) -> i128 {
    if total <= 0 || part <= 0 || remaining <= 0 {
        return 0;
    }

    let scaled = part.saturating_mul(remaining);
    let share = scaled / total;
    share.min(remaining)
}

#[cfg(test)]
mod test {
    use super::{proportional_share, tranche_payout};

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
    fn tranche_payout_saturates_instead_of_overflowing() {
        assert_eq!(tranche_payout(i128::MAX, 10_000), i128::MAX);

        let payout = tranche_payout(i128::MAX, 5_000);
        assert!(payout > 0);
        assert!(payout <= i128::MAX / 2 + 1);
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
                assert!(
                    payout >= 0,
                    "negative payout: deposited={deposited} bps={bps}"
                );
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

    #[test]
    fn zero_total_shares_nothing() {
        assert_eq!(proportional_share(100, 500, 0), 0);
    }

    #[test]
    fn zero_or_negative_part_shares_nothing() {
        assert_eq!(proportional_share(0, 500, 1_000), 0);
        assert_eq!(proportional_share(-50, 500, 1_000), 0);
    }

    #[test]
    fn zero_remaining_shares_nothing() {
        assert_eq!(proportional_share(300, 0, 1_000), 0);
    }

    #[test]
    fn full_part_gets_all_of_remaining() {
        assert_eq!(proportional_share(1_000, 500, 1_000), 500);
    }

    #[test]
    fn splits_proportionally_across_donors() {
        // Donor A gave 700 of 1,000 total; 400 is left to refund.
        assert_eq!(proportional_share(700, 400, 1_000), 280);
        // Donor B gave the other 300 of 1,000.
        assert_eq!(proportional_share(300, 400, 1_000), 120);
        // The two shares add up to the full remaining amount.
        assert_eq!(280 + 120, 400);
    }

    #[test]
    fn proportional_share_saturates_instead_of_overflowing() {
        let share = proportional_share(i128::MAX, i128::MAX, 1);
        assert_eq!(share, i128::MAX);
    }

    #[test]
    fn never_exceeds_remaining() {
        let parts = [1i128, 100, 999, 1_000, i128::MAX];
        let remainders = [0i128, 1, 500, 1_000, i128::MAX];
        let totals = [1i128, 1_000, i128::MAX];

        for &part in &parts {
            for &remaining in &remainders {
                for &total in &totals {
                    let share = proportional_share(part, remaining, total);
                    assert!(share >= 0);
                    assert!(share <= remaining);
                }
            }
        }
    }
}
