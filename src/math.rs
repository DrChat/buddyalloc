/// Basic power-of-2 integer math.
pub trait PowersOf2 {
    fn log2(self) -> u8;
}

impl PowersOf2 for usize {
    /// Calculate the base-2 logarithm of this value.
    ///
    /// This will normally round down, except for the case of `0.log2()`,
    /// which will return 0.
    ///
    /// Based on the obvious code at
    /// http://graphics.stanford.edu/~seander/bithacks.html#IntegerLogObvious
    fn log2(self) -> u8 {
        let mut temp = self;
        let mut result = 0;
        temp >>= 1;
        while temp != 0 {
            result += 1;
            temp >>= 1;
        }
        result
    }
}

#[test]
fn test_log2() {
    assert_eq!(0, 0.log2());
    assert_eq!(0, 1.log2());
    assert_eq!(1, 2.log2());
    assert_eq!(5, 32.log2());
    assert_eq!(10, 1024.log2());
}
