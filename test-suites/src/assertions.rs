use crate::test_fixture::SCALAR_7;
use soroban_fixed_point_math::FixedPoint;

pub fn assert_approx_eq_abs(a: i128, b: i128, delta: i128) {
    assert!(
        a > b - delta && a < b + delta,
        "assertion failed: `(left != right)` \
         (left: `{:?}`, right: `{:?}`, epsilon: `{:?}`)",
        a,
        b,
        delta
    );
}

pub fn assert_approx_eq_rel(a: i128, b: i128, delta: i128) {
    assert!(
        a > b
            - (b.fixed_mul_floor(delta, SCALAR_7)
                .unwrap()
                .fixed_div_floor(100_0000000, SCALAR_7))
            .unwrap()
            && a < b
                + (b.fixed_mul_floor(delta, SCALAR_7)
                    .unwrap()
                    .fixed_div_floor(100_0000000, SCALAR_7))
                .unwrap(),
        "assertion failed: `(left != right)` \
         (left: `{:?}`, right: `{:?}`, epsilon: `{:?}`)",
        a,
        b,
        delta
    );
}
