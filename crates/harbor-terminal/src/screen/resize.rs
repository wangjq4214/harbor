//! Transactional multi-buffer screen resize and selection projection.

use std::collections::HashSet;

use crate::NormalBuf;
use crate::content_anchor::{Affinity, ContentAnchor, ContentProjection};
use crate::primary_reflow::{
    CursorRetention, PreparationError, PreparedPrimaryResize, PreparedProjection, ReflowViewport,
};
use crate::selection_model::GenPos;

use super::cursor::CursorEngine;
use super::edit::PenState;
use super::hyperlink::HyperlinkRegistry;
use super::{BufferState, Screen};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BufferRole {
    Primary,
    Alternate,
}

#[derive(Debug)]
struct RectangularSelectionProjection {
    source: ContentProjection,
    source_live_top: u64,
    target_live_top: u64,
    rows: usize,
    cols: usize,
    retained_cells: Vec<bool>,
}

/// Buffer-level result of preparing one resize role, before shared session state.
#[derive(Debug)]
struct RoleBuffers {
    normal: NormalBuf,
    projection: ContentProjection,
    cursor: CursorEngine,
    review_anchor: Option<ContentAnchor>,
    rectangular_selection: Option<RectangularSelectionProjection>,
}

#[derive(Debug)]
pub(super) struct PreparedBufferResize {
    pub(super) normal: NormalBuf,
    pub(super) review_anchor: Option<ContentAnchor>,
    pub(super) cursor: CursorEngine,
    pub(super) pen_state: PenState,
    pub(super) hyperlinks: HyperlinkRegistry,
    projection: ContentProjection,
    rectangular_selection: Option<RectangularSelectionProjection>,
}

#[derive(Debug)]
pub(crate) struct PreparedScreenResize {
    history_limits: harbor_config::HistoryLimits,
    active: PreparedBufferResize,
    immediate_saved_primary: Vec<PreparedBufferResize>,
    saved_primary: Vec<PreparedBufferResize>,
    deferred_primary_source: Option<ContentProjection>,
    parked_alt: Vec<PreparedBufferResize>,
}

impl PreparedBufferResize {
    pub(crate) fn snapshot(&self, source: &Screen) -> crate::model::TerminalSnapshot {
        let rows = self.normal.rows();
        let cols = self.normal.cols();
        let mut snapshot = source.terminal_snapshot();
        snapshot.rows = rows;
        snapshot.cols = cols;
        snapshot.cells = self.normal.snapshot_cells();
        snapshot.cursor_x = self.cursor.cursor_x();
        snapshot.cursor_y = self.cursor.cursor_y(&self.normal);
        snapshot.scroll_count = self.normal.scroll_count();
        snapshot.view_offset = self.normal.view_offset();
        snapshot.history_start = self.normal.history_start();
        snapshot.wrapped = (0..rows).map(|r| self.normal.is_wrapped(r)).collect();
        snapshot.dirty_ranges = self.normal.dirty_ranges();
        snapshot
    }

    fn project_selection(&self, anchor: ContentAnchor) -> Option<(GenPos, ContentAnchor)> {
        let position = if let Some(rectangular) = &self.rectangular_selection {
            let source = rectangular.source.resolve_selection(anchor)?;
            let row = usize::try_from(source.generation.checked_sub(rectangular.source_live_top)?)
                .ok()?;
            if row >= rectangular.rows || source.col >= rectangular.cols {
                return None;
            }
            let index = row.checked_mul(rectangular.cols)?.checked_add(source.col)?;
            if !rectangular
                .retained_cells
                .get(index)
                .copied()
                .unwrap_or(false)
            {
                return None;
            }
            GenPos::new(
                rectangular
                    .target_live_top
                    .checked_add(u64::try_from(row).ok()?)?,
                source.col,
            )
        } else {
            self.projection.resolve_selection(anchor)?
        };
        let refreshed = self.projection.to_anchor(position, anchor.affinity)?;
        Some((position, refreshed))
    }
}

impl PreparedScreenResize {
    pub(crate) fn snapshot(&self, source: &Screen) -> crate::model::TerminalSnapshot {
        self.active.snapshot(source)
    }

    pub(crate) fn project_active_selection(
        &self,
        anchor: ContentAnchor,
    ) -> Option<(GenPos, ContentAnchor)> {
        self.active.project_selection(anchor)
    }

    pub(crate) fn project_saved_primary_selection(
        &self,
        anchor: ContentAnchor,
    ) -> Option<(GenPos, ContentAnchor)> {
        if let Some(source) = &self.deferred_primary_source {
            let position = source.resolve_selection(anchor)?;
            return Some((position, source.to_anchor(position, anchor.affinity)?));
        }
        self.saved_primary.first()?.project_selection(anchor)
    }

    pub(crate) fn project_parked_alt_selection(
        &self,
        anchor: ContentAnchor,
    ) -> Option<(GenPos, ContentAnchor)> {
        self.parked_alt.first()?.project_selection(anchor)
    }

    pub(crate) fn has_saved_primary(&self) -> bool {
        !self.saved_primary.is_empty()
    }

    pub(crate) fn has_parked_alt(&self) -> bool {
        !self.parked_alt.is_empty()
    }
}

impl Screen {
    pub(crate) fn prepare_resize(
        &self,
        rows: usize,
        cols: usize,
    ) -> Result<PreparedScreenResize, PreparationError> {
        self.prepare_resize_with_viewport(rows, cols, ReflowViewport::PullHistory)
    }

    pub(crate) fn prepare_resize_with_viewport(
        &self,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
    ) -> Result<PreparedScreenResize, PreparationError> {
        self.prepare_resize_with_history(rows, cols, viewport, self.history_limits)
    }

    pub(crate) fn prepare_resize_with_history(
        &self,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
    ) -> Result<PreparedScreenResize, PreparationError> {
        let rows = rows.max(1);
        let cols = cols.max(2);
        let role = if self.is_alt() {
            BufferRole::Alternate
        } else {
            BufferRole::Primary
        };
        let active = self
            .active
            .prepare_resize_for_role(rows, cols, role, viewport, limits, None)?;
        let mut immediate_saved_primary = Vec::new();
        let mut saved_primary = Vec::new();
        let mut deferred_primary_source = None;
        if let Some(saved) = self.saved_primary.as_deref() {
            saved_primary
                .try_reserve_exact(1)
                .map_err(|_| PreparationError::AllocationFailed)?;
            if viewport == ReflowViewport::PreserveLiveTop {
                if limits != saved.normal.history_limits() {
                    immediate_saved_primary
                        .try_reserve_exact(1)
                        .map_err(|_| PreparationError::AllocationFailed)?;
                    immediate_saved_primary.push(saved.prepare_resize_for_role(
                        saved.rows(),
                        saved.cols(),
                        BufferRole::Primary,
                        viewport,
                        limits,
                        None,
                    )?);
                }
                let source = immediate_saved_primary
                    .first()
                    .map_or(&*saved.normal, |p| &p.normal);
                deferred_primary_source = Some(ContentProjection::build(source)?);
                // Deferred geometry must use the already-trimmed source, never resurrect
                // content evicted by the immediate accepted shrink.
                saved_primary.push(saved.prepare_resize_for_role(
                    rows,
                    cols,
                    BufferRole::Primary,
                    viewport,
                    limits,
                    Some(source),
                )?);
            } else {
                saved_primary.push(saved.prepare_resize_for_role(
                    rows,
                    cols,
                    BufferRole::Primary,
                    viewport,
                    limits,
                    None,
                )?);
            }
        }
        let mut parked_alt = Vec::new();
        if let Some(alt) = self.parked_alt.as_deref() {
            parked_alt
                .try_reserve_exact(1)
                .map_err(|_| PreparationError::AllocationFailed)?;
            parked_alt.push(alt.prepare_resize_for_role(
                rows,
                cols,
                BufferRole::Alternate,
                viewport,
                limits,
                None,
            )?);
        }

        Ok(PreparedScreenResize {
            history_limits: limits,
            active,
            immediate_saved_primary,
            saved_primary,
            deferred_primary_source,
            parked_alt,
        })
    }

    pub(crate) fn commit_resize(&mut self, prepared: PreparedScreenResize) {
        let PreparedScreenResize {
            history_limits,
            active,
            mut immediate_saved_primary,
            mut saved_primary,
            deferred_primary_source,
            mut parked_alt,
        } = prepared;
        self.active.commit_resize(active);
        self.history_limits = history_limits;
        if deferred_primary_source.is_some() {
            if let (Some(primary), Some(prepared)) = (
                self.saved_primary.as_deref_mut(),
                immediate_saved_primary.pop(),
            ) {
                primary.commit_resize(prepared);
            }
            self.deferred_primary_resize = saved_primary;
        } else {
            self.deferred_primary_resize.clear();
            match (self.saved_primary.as_deref_mut(), saved_primary.pop()) {
                (Some(buffer), Some(prepared)) => buffer.commit_resize(prepared),
                (None, None) => {}
                _ => debug_assert!(false, "saved primary changed during resize preparation"),
            }
        }
        match (self.parked_alt.as_deref_mut(), parked_alt.pop()) {
            (Some(buffer), Some(prepared)) => buffer.commit_resize(prepared),
            (None, None) => {}
            _ => debug_assert!(false, "parked alternate changed during resize preparation"),
        }
    }

    /// Resizes this standalone model through the same detached preparation path as `Terminal`.
    ///
    /// Callers that also own a PTY should use `Terminal` so PTY and model commit transactionally.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        match self.prepare_resize(rows, cols) {
            Ok(prepared) => self.commit_resize(prepared),
            Err(error) => {
                tracing::error!(?error, "failed to prepare standalone screen resize");
            }
        }
    }
}

impl BufferState {
    pub(super) fn prepare_primary_resize_with_viewport(
        &self,
        requested_rows: usize,
        requested_cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: &NormalBuf,
    ) -> Result<PreparedPrimaryResize, PreparationError> {
        let cursor_floor = Some(
            self.cursor
                .cursor
                .y
                .max(self.cursor.cursor.saved.as_ref().map_or(0, |s| s.cursor_y)),
        );
        let mut retention = vec![CursorRetention::Position(self.live_cursor_position())];
        if let Some(saved) = self.cursor.cursor.saved.as_ref() {
            if let Some(anchor) = saved.anchor {
                if let Some(anchor) = self.anchor_mutations.apply(anchor) {
                    retention.push(CursorRetention::Anchor(anchor));
                }
            } else {
                let live_top = self.normal.history_start() + self.normal.scroll_count() as u64;
                retention.push(CursorRetention::Position(GenPos::new(
                    live_top.saturating_add(saved.cursor_y as u64),
                    saved.cursor_x,
                )));
            }
        }
        let prepared = PreparedPrimaryResize::prepare_geometry_with_limits(
            normal_source,
            requested_rows,
            requested_cols,
            cursor_floor,
            viewport,
            &retention,
            limits,
        )?;
        let live_cursor = prepared
            .source_cursor_anchor(self.live_cursor_position(), self.cursor.modes.pending_wrap)
            .ok_or(PreparationError::UnresolvedLiveCursor)?;
        let target_live_top = checked_live_top(prepared.normal())?;
        let target_live_end = target_live_top
            .checked_add(
                u64::try_from(requested_rows.max(1))
                    .map_err(|_| PreparationError::ArithmeticOverflow)?,
            )
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let live_is_retained = prepared
            .project_cursor(live_cursor)
            .is_some_and(|projection| {
                projection.position.generation >= target_live_top
                    && projection.position.generation < target_live_end
            });
        let live_cursor = if !live_is_retained
            && self
                .normal
                .live_row_metadata(self.cursor.cursor.y)
                .meaningful_extent
                == 0
        {
            let source_live_top = checked_live_top(&self.normal)?;
            let active_visible = self
                .normal
                .active_retained_row_count(cursor_floor)
                .saturating_sub(self.normal.scroll_count());
            let fallback_row = active_visible.min(self.normal.rows()).saturating_sub(1);
            let fallback_generation = source_live_top
                .checked_add(
                    u64::try_from(fallback_row)
                        .map_err(|_| PreparationError::ArithmeticOverflow)?,
                )
                .ok_or(PreparationError::ArithmeticOverflow)?;
            prepared
                .source_cursor_anchor(
                    GenPos::new(
                        fallback_generation,
                        self.cursor
                            .cursor
                            .x
                            .min(self.normal.cols().saturating_sub(1)),
                    ),
                    false,
                )
                .ok_or(PreparationError::UnresolvedLiveCursor)?
        } else {
            live_cursor
        };
        let live_top = self.normal.live_top_generation();
        let saved_cursor = match self.cursor.cursor.saved.as_ref() {
            None => PreparedProjection::Absent,
            Some(saved) => {
                let anchor = if let Some(anchor) = saved.anchor {
                    self.anchor_mutations.apply(anchor)
                } else {
                    prepared.source_cursor_anchor(
                        GenPos::new(
                            live_top.saturating_add(saved.cursor_y as u64),
                            saved.cursor_x,
                        ),
                        saved.pending_wrap,
                    )
                };
                anchor.map_or(PreparedProjection::Invalid, PreparedProjection::Projected)
            }
        };
        let review = if self.normal.view_offset() == 0 {
            PreparedProjection::Absent
        } else if let Some(anchor) = self.review_anchor {
            self.anchor_mutations
                .apply(anchor)
                .map_or(PreparedProjection::Invalid, PreparedProjection::Projected)
        } else {
            let generation = self.normal.history_start()
                + self
                    .normal
                    .scroll_count()
                    .saturating_sub(self.normal.view_offset()) as u64;
            prepared
                .source_anchor(GenPos::new(generation, 0), Affinity::Before)
                .map_or(PreparedProjection::Invalid, PreparedProjection::Projected)
        };

        prepared.attach_screen_anchors(live_cursor, saved_cursor, review)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_resize_for_role(
        &self,
        rows: usize,
        cols: usize,
        role: BufferRole,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: Option<&NormalBuf>,
    ) -> Result<PreparedBufferResize, PreparationError> {
        let RoleBuffers {
            mut normal,
            projection,
            cursor,
            review_anchor,
            rectangular_selection,
        } = self.prepare_role_buffers(role, rows, cols, viewport, limits, normal_source)?;
        normal.mark_all_dirty();
        let pen_state = self.pen_state.prepare_resize(cols)?;
        let mut reachable = HashSet::new();
        let reachable_cells = normal
            .retained_rows()
            .flat_map(|row| row.cells.iter())
            .filter(|cell| cell.hyperlink.is_some())
            .count();
        reachable
            .try_reserve(
                reachable_cells
                    .checked_add(2)
                    .ok_or(PreparationError::ArithmeticOverflow)?,
            )
            .map_err(|_| PreparationError::AllocationFailed)?;
        reachable.extend(
            normal
                .retained_rows()
                .flat_map(|row| row.cells.iter())
                .filter_map(|cell| cell.hyperlink),
        );
        reachable.extend(pen_state.hyperlink_ids());
        let hyperlinks = self.hyperlinks.prepare_retained(&reachable)?;

        Ok(PreparedBufferResize {
            normal,
            review_anchor,
            cursor,
            pen_state,
            hyperlinks,
            projection,
            rectangular_selection,
        })
    }

    /// Prepares only the role-owned buffer contents; callers share session-level state.
    fn prepare_role_buffers(
        &self,
        role: BufferRole,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: Option<&NormalBuf>,
    ) -> Result<RoleBuffers, PreparationError> {
        match role {
            BufferRole::Primary => {
                self.prepare_primary_buffer(rows, cols, viewport, limits, normal_source)
            }
            BufferRole::Alternate => self.prepare_alternate_buffer(rows, cols),
        }
    }

    /// Primary resize, or a geometry-preserving retrim of the history budget.
    fn prepare_primary_buffer(
        &self,
        rows: usize,
        cols: usize,
        viewport: ReflowViewport,
        limits: harbor_config::HistoryLimits,
        normal_source: Option<&NormalBuf>,
    ) -> Result<RoleBuffers, PreparationError> {
        if rows == self.rows() && cols == self.cols() && limits != self.normal.history_limits() {
            let normal = normal_source
                .unwrap_or(&self.normal)
                .prepare_history_limits(limits)?;
            let projection = ContentProjection::build(&normal)?;
            let review_anchor = if normal.view_offset() == 0 {
                None
            } else {
                self.review_anchor
                    .filter(|anchor| projection.resolve_selection(*anchor).is_some())
                    .or_else(|| projection.oldest_anchor())
            };
            return Ok(RoleBuffers {
                normal,
                projection,
                cursor: (*self.cursor).clone(),
                review_anchor,
                rectangular_selection: None,
            });
        }
        let prepared = self.prepare_primary_resize_with_viewport(
            rows,
            cols,
            viewport,
            limits,
            normal_source.unwrap_or(&self.normal),
        )?;
        let (normal, live_cursor, saved_cursor, review) = prepared.into_screen_parts();
        let projection = ContentProjection::build(&normal)?;
        let live_top = checked_live_top(&normal)?;
        let mut cursor = (*self.cursor).clone();
        cursor.clamp_to_grid(rows, cols);
        let PreparedProjection::Projected(live) = live_cursor else {
            return Err(PreparationError::UnresolvedLiveCursor);
        };
        cursor.cursor.y = usize::try_from(live.position.generation - live_top)
            .map_err(|_| PreparationError::ArithmeticOverflow)?;
        cursor.cursor.x = live.position.col;
        cursor.modes.pending_wrap = live.pending_wrap;
        match saved_cursor {
            PreparedProjection::Projected(projected) => {
                if let Some(saved) = cursor.cursor.saved.as_mut() {
                    saved.cursor_y = usize::try_from(projected.position.generation - live_top)
                        .map_err(|_| PreparationError::ArithmeticOverflow)?;
                    saved.cursor_x = projected.position.col;
                    saved.pending_wrap = projected.pending_wrap;
                    saved.anchor =
                        projection.cursor_anchor(projected.position, projected.pending_wrap);
                }
            }
            PreparedProjection::Absent | PreparedProjection::Invalid => {
                cursor.cursor.saved = None;
            }
        }
        let review_anchor = match review {
            PreparedProjection::Projected(position) => {
                projection.to_anchor(position, Affinity::Before)
            }
            PreparedProjection::Absent | PreparedProjection::Invalid => None,
        };
        Ok(RoleBuffers {
            normal,
            projection,
            cursor,
            review_anchor,
            rectangular_selection: None,
        })
    }

    /// Alternate resize is rectangular: retained cells keep their position and content.
    fn prepare_alternate_buffer(
        &self,
        rows: usize,
        cols: usize,
    ) -> Result<RoleBuffers, PreparationError> {
        let normal = self.normal.prepare_rectangular_resize(rows, cols)?;
        let projection = ContentProjection::build(&normal)?;
        let source_projection = ContentProjection::build(&self.normal)?;
        let source_live_top = checked_live_top(&self.normal)?;
        let target_live_top = normal.history_start();
        let copied_rows = self.normal.rows().min(rows);
        let copied_cols = self.normal.cols().min(cols);
        let retained_count = copied_rows
            .checked_mul(copied_cols)
            .ok_or(PreparationError::ArithmeticOverflow)?;
        let mut retained_cells = Vec::new();
        retained_cells
            .try_reserve_exact(retained_count)
            .map_err(|_| PreparationError::AllocationFailed)?;
        for row in 0..copied_rows {
            let source_generation = source_live_top
                .checked_add(u64::try_from(row).map_err(|_| PreparationError::ArithmeticOverflow)?)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            let target_generation = target_live_top
                .checked_add(u64::try_from(row).map_err(|_| PreparationError::ArithmeticOverflow)?)
                .ok_or(PreparationError::ArithmeticOverflow)?;
            for col in 0..copied_cols {
                retained_cells.push(
                    self.normal.cell_at_generation(source_generation, col)
                        == normal.cell_at_generation(target_generation, col),
                );
            }
        }
        let rectangular_selection = Some(RectangularSelectionProjection {
            source: source_projection,
            source_live_top,
            target_live_top,
            rows: copied_rows,
            cols: copied_cols,
            retained_cells,
        });
        let mut cursor = (*self.cursor).clone();
        cursor.clamp_to_grid(rows, cols);
        let live_top = normal.history_start();
        if let Some(saved) = cursor.cursor.saved.as_mut() {
            let saved_position = GenPos::new(
                live_top
                    .checked_add(
                        u64::try_from(saved.cursor_y)
                            .map_err(|_| PreparationError::ArithmeticOverflow)?,
                    )
                    .ok_or(PreparationError::ArithmeticOverflow)?,
                saved.cursor_x,
            );
            saved.anchor = projection.cursor_anchor(saved_position, saved.pending_wrap);
        }
        Ok(RoleBuffers {
            normal,
            projection,
            cursor,
            review_anchor: None,
            rectangular_selection,
        })
    }
}

fn checked_live_top(normal: &NormalBuf) -> Result<u64, PreparationError> {
    normal
        .history_start()
        .checked_add(
            u64::try_from(normal.scroll_count())
                .map_err(|_| PreparationError::ArithmeticOverflow)?,
        )
        .ok_or(PreparationError::ArithmeticOverflow)
}
