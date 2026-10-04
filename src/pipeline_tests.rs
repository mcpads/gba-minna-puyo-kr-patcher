use super::*;

#[test]
fn baseline_preserves_input_and_bps_detects_corruption() {
    let source = b"controlled baseline fixture";
    let (target, mut patch) = baseline_artifacts(source).unwrap();
    assert_eq!(target, source);
    patch[4] ^= 1;
    assert!(apply_patch(source, &patch, BpsLimits::new(1024, 1024, 1024, 1024, 1024)).is_err());
}

#[test]
fn protected_plan_rejects_unregistered_final_difference() {
    let source = b"unchanged";
    let mut target = source.to_vec();
    target[2] ^= 1;
    assert!(WritePlan::new().audit(source, &target, None).is_err());
}
