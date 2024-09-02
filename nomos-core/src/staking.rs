use cl::balance::Unit;

// derive-unit(NOMOS_NMO)
pub const NMO_UNIT: Unit = [
    67, 50, 140, 228, 181, 204, 226, 242, 254, 193, 239, 51, 237, 68, 36, 126, 124, 227, 60, 112,
    223, 195, 146, 236, 5, 21, 42, 215, 48, 122, 25, 195,
];

#[cfg(test)]
mod test {
    use super::*;
    use cl::note::derive_unit;

    #[test]
    fn test_unit_derivation() {
        assert_eq!(NMO_UNIT, derive_unit("NOMOS_NMO"));
    }
}
