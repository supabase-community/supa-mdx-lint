use std::cmp::Ordering;
use std::mem;
use std::ops::{Add, Deref, DerefMut, Range, SubAssign};

use serde::{Deserialize, Serialize};

use crate::{rope::Rope, rules::RuleContext};

/// An offset in the source document, accounting for frontmatter lines.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct AdjustedOffset(usize);

impl Deref for AdjustedOffset {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AdjustedOffset {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<usize> for AdjustedOffset {
    fn from(offset: usize) -> Self {
        Self(offset)
    }
}

impl From<&usize> for AdjustedOffset {
    fn from(offset: &usize) -> Self {
        Self(*offset)
    }
}

impl From<AdjustedOffset> for usize {
    fn from(offset: AdjustedOffset) -> Self {
        offset.0
    }
}

impl From<&AdjustedOffset> for usize {
    fn from(offset: &AdjustedOffset) -> Self {
        offset.0
    }
}

impl Add for AdjustedOffset {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl SubAssign for AdjustedOffset {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl AdjustedOffset {
    pub fn increment(&mut self, steps: usize) {
        self.0 += steps;
    }
}

impl AdjustedOffset {
    pub(crate) fn from_unadjusted(
        offset: UnadjustedOffset,
        mut from_start: AdjustedOffset,
    ) -> Self {
        from_start.increment(offset.0);
        from_start
    }

    pub(crate) fn from_unist(point: &markdown::unist::Point, from_start: AdjustedOffset) -> Self {
        Self::from_unadjusted(UnadjustedOffset::from(point), from_start)
    }

    pub(crate) fn into_usize(self) -> usize {
        Into::<usize>::into(self)
    }
}

/// An offset in the source document, not accounting for frontmatter lines.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct UnadjustedOffset(usize);

impl Deref for UnadjustedOffset {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for UnadjustedOffset {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<usize> for UnadjustedOffset {
    fn from(offset: usize) -> Self {
        Self(offset)
    }
}

impl From<&usize> for UnadjustedOffset {
    fn from(offset: &usize) -> Self {
        Self(*offset)
    }
}

impl From<markdown::unist::Point> for UnadjustedOffset {
    fn from(value: markdown::unist::Point) -> Self {
        Self(value.offset)
    }
}

impl From<&markdown::unist::Point> for UnadjustedOffset {
    fn from(value: &markdown::unist::Point) -> Self {
        Self(value.offset)
    }
}

/// A point in the source document, accounting for frontmatter lines.
#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdjustedPoint {
    pub row: usize,
    pub column: usize,
}

impl PartialOrd for AdjustedPoint {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AdjustedPoint {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.row.cmp(&other.row) {
            Ordering::Less => Ordering::Less,
            Ordering::Equal => self.column.cmp(&other.column),
            Ordering::Greater => Ordering::Greater,
        }
    }
}

impl AdjustedPoint {
    pub(crate) fn from_adjusted_offset(offset: &AdjustedOffset, rope: &Rope) -> Self {
        let (row, column) = rope.line_column_of_byte(offset.into());
        Self { row, column }
    }
}

/// A range in the source document, accounting for frontmatter lines.
/// The start point is inclusive, the end point is exclusive.
#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdjustedRange(Range<AdjustedOffset>);

impl Deref for AdjustedRange {
    type Target = Range<AdjustedOffset>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AdjustedRange {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<AdjustedRange> for Range<usize> {
    fn from(range: AdjustedRange) -> Self {
        Self::from(&range)
    }
}

impl From<&AdjustedRange> for Range<usize> {
    fn from(range: &AdjustedRange) -> Self {
        Self {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl AdjustedRange {
    pub(crate) fn new(start: AdjustedOffset, end: AdjustedOffset) -> Self {
        Self(Range { start, end })
    }

    pub(crate) fn from_unadjusted_position(
        position: &markdown::unist::Position,
        context: &RuleContext,
    ) -> Self {
        let adjusted_start =
            AdjustedOffset::from_unist(&position.start, context.content_start_offset());
        let adjusted_end =
            AdjustedOffset::from_unist(&position.end, context.content_start_offset());
        Self(Range {
            start: adjusted_start,
            end: adjusted_end,
        })
    }

    pub(crate) fn span_between(first: &Self, second: &Self) -> Self {
        let start = first.start.min(second.start);
        let end = first.end.max(second.end);
        Self(Range { start, end })
    }

    pub(crate) fn overlaps_or_abuts(&self, other: &Self) -> bool {
        if self.start > other.start {
            other.overlaps_or_abuts(self)
        } else {
            self.end >= other.start
        }
    }

    // Helper method to avoid having to call the ridiculous
    // `Into::<Range<usize>>::into` in many places.
    pub fn to_usize_range(&self) -> Range<usize> {
        Into::<Range<usize>>::into(self)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MaybeEndedLineRange(MaybeEndedRange<usize>);

impl Deref for MaybeEndedLineRange {
    type Target = MaybeEndedRange<usize>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MaybeEndedLineRange {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl MaybeEndedLineRange {
    pub fn new(start: usize, end: Option<usize>) -> Self {
        Self(MaybeEndedRange { start, end })
    }

    pub fn overlaps_lines(&self, range: &AdjustedRange, rope: &Rope) -> bool {
        let range_start_line = AdjustedPoint::from_adjusted_offset(&range.start, rope).row;
        let range_end_line = AdjustedPoint::from_adjusted_offset(&range.end, rope).row;
        self.start <= range_start_line && self.end.map_or(true, |end| end > range_start_line)
            || self.start <= range_end_line && self.end.map_or(true, |end| end > range_end_line)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MaybeEndedRange<T>
where
    T: Ord,
{
    pub start: T,
    pub end: Option<T>,
}

impl<T: Ord> MaybeEndedRange<T> {
    pub(crate) fn is_open_ended(&self) -> bool {
        self.end.is_none()
    }

    /**
     * Check if two MaybeEndedRanges overlap strictly. (Strict: ranges are
     * not considered to overlap if they are merely adjoining.)
     */
    pub(crate) fn overlaps_strict(&self, other: &Self) -> bool {
        self.start <= other.start && self.end.as_ref().map_or(true, |end| *end > other.start)
            || other.start <= self.start && other.end.as_ref().map_or(true, |end| *end > self.start)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct DenormalizedLocation {
    pub offset_range: AdjustedRange,
    pub start: AdjustedPoint,
    pub end: AdjustedPoint,
}

impl DenormalizedLocation {
    pub(crate) fn from_offset_range(range: AdjustedRange, context: &RuleContext) -> Self {
        let start = AdjustedPoint::from_adjusted_offset(&range.start, context.rope());
        let end = AdjustedPoint::from_adjusted_offset(&range.end, context.rope());
        Self {
            offset_range: range,
            start,
            end,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RangeSet(Vec<AdjustedRange>);

impl RangeSet {
    pub(crate) fn new() -> Self {
        Default::default()
    }

    pub(crate) fn push(&mut self, range: AdjustedRange) {
        match self.overlaps_impl(&range) {
            Ok(index) => {
                self.0[index] = AdjustedRange::span_between(&self.0[index], &range);
                if index < self.0.len() - 1 && self.0[index].overlaps_or_abuts(&self.0[index + 1]) {
                    let taken_vec = mem::take(&mut self.0);
                    self.0 = taken_vec.into_iter().fold(Vec::new(), |mut accum, range| {
                        if !accum.is_empty() {
                            let last_index = accum.len() - 1;
                            if accum[last_index].overlaps_or_abuts(&range) {
                                accum[last_index] =
                                    AdjustedRange::span_between(&accum[last_index], &range);
                            } else {
                                accum.push(range);
                            }
                        } else {
                            accum.push(range);
                        }
                        accum
                    });
                }
            }
            Err(index) => {
                self.0.insert(index, range);
            }
        }
    }

    pub(crate) fn completely_contains(&self, range: &AdjustedRange) -> bool {
        match self.overlaps_impl(range) {
            Err(_) => false,
            Ok(index) => {
                let potential_container = &self.0[index];
                potential_container.start <= range.start && potential_container.end >= range.end
            }
        }
    }

    fn overlaps_impl(&self, range: &AdjustedRange) -> Result<usize, usize> {
        self.0
            .binary_search_by(|probe| {
                if probe.end < range.start {
                    Ordering::Less
                } else if probe.start > range.end {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .map(|index| {
                // Ensure we return the first matching index
                let mut first_index = index;
                while first_index > 0 && self.0[first_index - 1].overlaps_or_abuts(range) {
                    first_index -= 1;
                }
                first_index
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{AdjustedOffset, AdjustedPoint, AdjustedRange, DenormalizedLocation};

    impl DenormalizedLocation {
        pub fn dummy(
            start_offset: usize,
            end_offset: usize,
            start_row: usize,
            start_column: usize,
            end_row: usize,
            end_column: usize,
        ) -> Self {
            Self {
                offset_range: AdjustedRange::new(
                    AdjustedOffset::from(start_offset),
                    AdjustedOffset::from(end_offset),
                ),
                start: AdjustedPoint {
                    row: start_row,
                    column: start_column,
                },
                end: AdjustedPoint {
                    row: end_row,
                    column: end_column,
                },
            }
        }
    }

    #[test]
    fn test_range_set_merges_overlapping_ranges() {
        let mut set = super::RangeSet::new();

        let range1 = AdjustedRange::new(AdjustedOffset::from(0), AdjustedOffset::from(5));
        let range2 = AdjustedRange::new(AdjustedOffset::from(3), AdjustedOffset::from(8));

        set.push(range1);
        set.push(range2);

        assert_eq!(set.0.len(), 1);
        assert_eq!(set.0[0].start, AdjustedOffset::from(0));
        assert_eq!(set.0[0].end, AdjustedOffset::from(8));
    }

    #[test]
    fn test_range_set_merges_multiple_overlapping_ranges() {
        let mut set = super::RangeSet::new();

        let range1 = AdjustedRange::new(AdjustedOffset::from(0), AdjustedOffset::from(5));
        let range2 = AdjustedRange::new(AdjustedOffset::from(3), AdjustedOffset::from(18));
        let range3 = AdjustedRange::new(AdjustedOffset::from(10), AdjustedOffset::from(15));
        let range4 = AdjustedRange::new(AdjustedOffset::from(17), AdjustedOffset::from(20));
        let range5 = AdjustedRange::new(AdjustedOffset::from(23), AdjustedOffset::from(25));

        set.push(range1);
        set.push(range3);
        set.push(range4);
        set.push(range5);
        set.push(range2);

        assert_eq!(set.0.len(), 2);
        assert_eq!(set.0[0].start, AdjustedOffset::from(0));
        assert_eq!(set.0[0].end, AdjustedOffset::from(20));
    }

    #[test]
    fn test_range_set_merges_adjacent_ranges() {
        let mut set = super::RangeSet::new();

        let range1 = AdjustedRange::new(AdjustedOffset::from(0), AdjustedOffset::from(5));
        let range2 = AdjustedRange::new(AdjustedOffset::from(5), AdjustedOffset::from(8));

        set.push(range1);
        set.push(range2);

        assert_eq!(set.0.len(), 1);
        assert_eq!(set.0[0].start, AdjustedOffset::from(0));
        assert_eq!(set.0[0].end, AdjustedOffset::from(8));

        let mut set = super::RangeSet::new();

        let range1 = AdjustedRange::new(AdjustedOffset::from(5), AdjustedOffset::from(8));
        let range2 = AdjustedRange::new(AdjustedOffset::from(0), AdjustedOffset::from(5));

        set.push(range1);
        set.push(range2);

        assert_eq!(set.0.len(), 1);
        assert_eq!(set.0[0].start, AdjustedOffset::from(0));
        assert_eq!(set.0[0].end, AdjustedOffset::from(8));
    }

    #[test]
    fn test_range_set_keeps_non_overlapping_ranges_separate() {
        let mut set = super::RangeSet::new();

        let range1 = AdjustedRange::new(AdjustedOffset::from(0), AdjustedOffset::from(3));
        let range2 = AdjustedRange::new(AdjustedOffset::from(5), AdjustedOffset::from(8));

        set.push(range1);
        set.push(range2);

        assert_eq!(set.0.len(), 2);
        assert_eq!(set.0[0].start, AdjustedOffset::from(0));
        assert_eq!(set.0[0].end, AdjustedOffset::from(3));
        assert_eq!(set.0[1].start, AdjustedOffset::from(5));
        assert_eq!(set.0[1].end, AdjustedOffset::from(8));
    }

    #[test]
    fn test_range_set_completely_contains() {
        let mut set = super::RangeSet::new();

        // Add a range from 0-10
        let container = AdjustedRange::new(AdjustedOffset::from(0), AdjustedOffset::from(10));
        set.push(container);

        // Test contained range
        let contained = AdjustedRange::new(AdjustedOffset::from(2), AdjustedOffset::from(8));
        assert!(set.completely_contains(&contained));

        // Test partially overlapping range
        let partial = AdjustedRange::new(AdjustedOffset::from(5), AdjustedOffset::from(12));
        assert!(!set.completely_contains(&partial));

        // Test non-overlapping range
        let outside = AdjustedRange::new(AdjustedOffset::from(15), AdjustedOffset::from(20));
        assert!(!set.completely_contains(&outside));

        // Test exact same range
        let same = AdjustedRange::new(AdjustedOffset::from(0), AdjustedOffset::from(10));
        assert!(set.completely_contains(&same));
    }
}
