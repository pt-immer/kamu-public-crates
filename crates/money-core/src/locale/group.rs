//! Digit grouping, read right to left with the last size repeating.

/// Insert `separator` into `digits` per `sizes`, read right-to-left, last size repeating.
pub(super) fn group(digits: &str, sizes: &[u8], separator: &str) -> String {
    if sizes.is_empty() || separator.is_empty() {
        return digits.to_owned();
    }

    // `digits` comes from the fixed-point formatter and is ASCII, so byte offsets are char
    // boundaries here and no slice below can split a character.
    //
    // Fail loudly if the ASCII boundary invariant breaks; never drop displayed digits.
    let mut chunks: Vec<&str> = Vec::new();
    let mut end = digits.len();
    let mut step: usize = 0;

    while end > 0 {
        let size = sizes.get(step).or_else(|| sizes.last()).map_or(0, |s| usize::from(*s));
        if size == 0 {
            // A zero size would consume nothing and loop forever. Stop and emit the rest
            // ungrouped, which is what a caller writing `&[0]` can only have meant.
            break;
        }
        let start = end.saturating_sub(size);
        chunks.push(digits.get(start..end).expect("start..end is inside an all-ASCII digit string"));
        end = start;
        step = step.saturating_add(1);
    }
    if end > 0 {
        chunks.push(digits.get(..end).expect("end is inside an all-ASCII digit string"));
    }

    chunks.reverse();
    chunks.join(separator)
}

#[cfg(test)]
mod tests {
    use super::group;
    use crate::Iso4217;
    use crate::Money;
    use crate::iso::USD;
    use crate::locale::EN_USD;
    use crate::locale::LocalePolicy;

    /// Grouping is exercised through the public API in `super::render`; these pin the
    /// degenerate inputs that have no reachable public spelling.
    #[test]
    fn grouping_handles_its_degenerate_inputs() {
        assert_eq!(group("1234", &[], ","), "1234", "no sizes");
        assert_eq!(group("1234", &[3], ""), "1234", "no separator");
        assert_eq!(group("", &[3], ","), "", "no digits");
        assert_eq!(group("12", &[3], ","), "12", "shorter than one group");
        assert_eq!(group("123", &[3], ","), "123", "exactly one group, no leader");
        // A zero size must terminate rather than spin, and must not lose digits.
        assert_eq!(group("1234", &[0], ","), "1234");
        assert_eq!(group("1234567", &[3, 0], ","), "1234,567");
    }

    /// The last grouping size repeats. `[3]` is the western group-of-three; `[3, 2]` is the
    /// Indian lakh/crore shape, which is the reason this is a slice and not a single number.
    #[test]
    fn the_last_grouping_size_repeats() {
        let big = Money::<USD>::try_from_major(12_345_678).unwrap();
        assert_eq!(EN_USD.render(big).unwrap(), "$12,345,678.00");

        // Indian digit grouping: 3, then 2 forever. Built by hand rather than shipped as a
        // named locale, because the table has no INR and inventing one to decorate a test
        // would be a fact this crate did not measure.
        let indian = LocalePolicy::new(Iso4217::USD, "$").try_with_grouping(&[3, 2]).unwrap();
        assert_eq!(indian.render(big).unwrap(), "$1,23,45,678.00");

        let ungrouped = LocalePolicy::new(Iso4217::USD, "$").try_with_grouping(&[]).unwrap();
        assert_eq!(ungrouped.render(big).unwrap(), "$12345678.00");
    }
}
