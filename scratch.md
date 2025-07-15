# Auto-Fixing Feature Explainer

## How Auto-Fixing Works

The auto-fixing feature is implemented in the `Linter::fix` method (see `src/fix.rs`). The process is as follows:

1. **Collect Fixes:**
   - For each file with fixable lint errors, collect all `LintCorrection` objects (which can be `Insert`, `Delete`, or `Replace`).
2. **Sort and Reverse:**
   - The fixes are sorted and then reversed so that they are applied from the end of the file to the start. This is intended to avoid shifting offsets when applying multiple fixes.
3. **Apply Fixes:**
   - Each fix is applied in order to a `Rope` (a mutable string buffer). Inserts add text, deletes remove ranges, and replaces swap out ranges for new text.
4. **Write Back:**
   - The fixed content is written back to the file.

## The Bug: Line-Changing Fixes Affect Offsets

When a fix (such as the Admonition blank space rule) inserts or deletes lines, it changes the offsets of all subsequent text in the file. If multiple fixes are scheduled for the same file, and their offsets are calculated based on the original file, then after the first fix is applied, the offsets for the remaining fixes may no longer be correct. This can cause subsequent fixes to be applied at the wrong locations, leading to incorrect or broken output.

### Example Scenario
- Suppose two fixes are scheduled:
  1. Insert a blank line after an Admonition opening tag (offset X)
  2. Replace a word at offset Y (where Y > X)
- After the first fix, all offsets after X are shifted by the length of the inserted text. If the second fix is still applied at the original offset Y, it will be misplaced.

## Why This Happens
- The current implementation sorts and reverses the fixes, but does not update the offsets of subsequent fixes after each change. This is only safe if all fixes are non-overlapping and do not change the length of the file before the next fix.
- When fixes add or remove lines, all subsequent offsets must be adjusted accordingly, or the fixes must be applied in a way that accounts for the shifting positions.

## Solution Direction
- To fix this, we need to either:
  - Recalculate offsets after each fix is applied, or
  - Apply fixes in a way that is robust to shifting offsets (e.g., always apply from the end of the file to the start, and ensure all offsets are relative to the original content).
- Test cases should be added or reviewed to ensure that multiple fixes, especially those that add or remove lines, are applied correctly.