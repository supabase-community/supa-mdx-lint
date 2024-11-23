use std::cmp::Ordering;
use std::ops::{Add, Deref, DerefMut, Range};

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

impl AdjustedOffset {
    pub fn increment(&mut self, steps: usize) {
        self.0 += steps;
    }
}

impl AdjustedOffset {
    pub fn from_unadjusted(offset: UnadjustedOffset, context: &RuleContext) -> Self {
        let mut content_start_offset = *context.content_start_offset();
        content_start_offset.increment(offset.0);
        content_start_offset
    }

    pub fn from_unist(point: &markdown::unist::Point, context: &RuleContext) -> Self {
        Self::from_unadjusted(UnadjustedOffset::from(point), context)
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

impl AdjustedRange {
    pub fn new(start: AdjustedOffset, end: AdjustedOffset) -> Self {
        Self(Range { start, end })
    }

    pub fn from_unadjusted_position(
        position: &markdown::unist::Position,
        context: &RuleContext,
    ) -> Self {
        let adjusted_start = AdjustedOffset::from_unist(&position.start, context);
        let adjusted_end = AdjustedOffset::from_unist(&position.end, context);
        Self(Range {
            start: adjusted_start,
            end: adjusted_end,
        })
    }

    pub fn span_between(first: &Self, second: &Self) -> Self {
        let start = first.start.min(second.start);
        let end = first.end.max(second.end);
        Self(Range { start, end })
    }
}

#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
pub(crate) struct MaybeEndedRange<T>
where
    T: PartialOrd,
{
    pub start: T,
    pub end: Option<T>,
}

impl<T: PartialOrd> MaybeEndedRange<T> {
    pub fn is_open_ended(&self) -> bool {
        self.end.is_none()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct DenormalizedLocation {
    pub offset_range: AdjustedRange,
    pub start: AdjustedPoint,
    pub end: AdjustedPoint,
}

impl DenormalizedLocation {
    pub fn from_offset_range(range: AdjustedRange, context: &RuleContext) -> Self {
        let start = AdjustedPoint::from_adjusted_offset(&range.start, context.rope());
        let end = AdjustedPoint::from_adjusted_offset(&range.end, context.rope());
        Self {
            offset_range: range,
            start,
            end,
        }
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
}
