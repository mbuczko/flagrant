-- Adds a global (not per-environment), monotonically-incrementing optimistic-concurrency
-- version counter to features and segments, bumped by exactly one on every successful
-- COMMIT that touches the row (property change or delete) - see flagrant::models::commit::apply
-- and the version-check it performs before feature::patch/segment::patch. Groups and rules
-- are always addressed through their owning segment's SegmentPatchOp, so bumping the
-- segment's version on any op targeting it transitively covers them too - they get no
-- version column of their own.
ALTER TABLE features ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE segments ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
