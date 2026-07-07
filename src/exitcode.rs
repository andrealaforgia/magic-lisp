//! Process exit codes, one per distinguishable failure class (sysexits.h-style).

pub const SUCCESS: i32 = 0;
pub const USAGE_ERROR: i32 = 64;
pub const SOURCE_ERROR: i32 = 65;
pub const BAD_ARTIFACT: i32 = 66;
pub const RUNTIME_ERROR: i32 = 70;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_five_classes_are_pairwise_distinct() {
        let codes = [
            SUCCESS,
            USAGE_ERROR,
            SOURCE_ERROR,
            BAD_ARTIFACT,
            RUNTIME_ERROR,
        ];
        let unique: HashSet<_> = codes.iter().collect();
        assert_eq!(
            unique.len(),
            codes.len(),
            "exit codes must be pairwise distinct"
        );
    }
}
