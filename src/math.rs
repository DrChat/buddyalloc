/// Calculate the base-2 logarithm of this value.
///
/// This will normally round down, except for the case of `0.log2()`,
/// which will return 0.
///
/// Based on the obvious code at
/// http://graphics.stanford.edu/~seander/bithacks.html#IntegerLogObvious
pub const fn log2(n: usize) -> u8 {
    let mut temp = n;
    let mut result = 0;
    temp >>= 1;
    while temp != 0 {
        result += 1;
        temp >>= 1;
    }
    result
}

#[test]
fn test_log2() {
    assert_eq!(0, log2(0));
    assert_eq!(0, log2(1));
    assert_eq!(1, log2(2));
    assert_eq!(5, log2(32));
    assert_eq!(10, log2(1024));
}
