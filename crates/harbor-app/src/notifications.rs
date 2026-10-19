//! Application configuration-notice state and Harbor's floating presentation.

use crate::appearance::{GlassAppearance, glass_icon_button_style};
use harbor_widget::{
    Alignment, BorderRadius, Notification, Portal,
    scene::primitive::Color,
    view::{BuildCx, Component},
};
use std::time::{Duration, Instant};

const NOTIFICATION_DURATION: Duration = Duration::from_secs(3);

/// Harbor's root escape, top-right placement and product presentation preset.
pub(crate) fn configuration_notification(
    title: &'static str,
    message: String,
    top_offset: f32,
    on_dismiss: impl Fn() + Send + Sync + 'static,
) -> impl Component {
    let on_dismiss = std::sync::Arc::new(on_dismiss);
    move |cx: &mut BuildCx| {
        let appearance = GlassAppearance {
            radius: BorderRadius::all(6.0).unwrap(),
            ..GlassAppearance::default()
        };
        let card = Notification::new(message.clone(), {
            let on_dismiss = on_dismiss.clone();
            move || on_dismiss()
        })
        .title(title)
        .max_lines(4)
        .close_alignment(Alignment::Start)
        .detail_color(Color {
            a: cx.theme().button.normal.foreground.a * 0.82,
            ..cx.theme().button.normal.foreground
        })
        .style(harbor_widget::widgets::notification::NotificationStyle {
            foreground: Some(cx.theme().button.normal.foreground),
            // The shared material owns the fill; an opaque card would hide its filter.
            background: Some(Color::TRANSPARENT),
            ..Default::default()
        })
        .close_button_style(glass_icon_button_style(cx))
        .build(cx);
        let panel = appearance.panel(cx, card);
        Portal::new()
            .child(
                harbor_widget::widgets::align::Align::new(Alignment::End)
                    .vertical_alignment(Alignment::Start)
                    .insets(top_offset + 12.0, 12.0, 12.0, 12.0)
                    .child(panel)
                    .build(cx),
            )
            .build(cx)
    }
}

/// Independent feedback sources: recovery of reload must not hide observation
/// or operation failures. This is presentation state, not configuration state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationNoticeSource {
    Startup,
    Reload,
    Observation,
    Resource,
    Operation,
    Command,
}

#[derive(Clone, Default)]
pub(crate) struct ConfigurationNotices {
    entries: Vec<(ConfigurationNoticeSource, String, bool)>,
    deadline: Option<Instant>,
    pub(crate) revision: u64,
}

impl ConfigurationNotices {
    /// Returns whether content, dismissal history or the expiry deadline changed.
    pub(crate) fn publish(
        &mut self,
        source: ConfigurationNoticeSource,
        message: Option<String>,
    ) -> bool {
        self.publish_at(source, message, Instant::now())
    }

    fn publish_at(
        &mut self,
        source: ConfigurationNoticeSource,
        message: Option<String>,
        now: Instant,
    ) -> bool {
        // Expiry is a real update even when this publication itself is a duplicate.
        let expired = self.expire(now);
        let existing = self.entries.iter().position(|entry| entry.0 == source);
        if let Some(index) = existing {
            if message.as_ref() == Some(&self.entries[index].1) {
                return expired;
            }
            self.entries.remove(index);
        } else if message.is_none() {
            return expired;
        }
        if let Some(message) = message {
            self.entries.push((source, message, false));
            self.deadline = Some(now + NOTIFICATION_DURATION);
        }
        self.revision += 1;
        if !self.entries.iter().any(|entry| !entry.2) {
            self.deadline = None;
        }
        true
    }

    pub(crate) fn title(&self) -> &'static str {
        let mut visible = self.entries.iter().filter(|entry| !entry.2);
        let Some((source, _, _)) = visible.next() else {
            return "Notification";
        };
        if visible.next().is_some() {
            return "Notifications";
        }
        match source {
            ConfigurationNoticeSource::Startup => "Startup configuration",
            ConfigurationNoticeSource::Reload => "Configuration reload",
            ConfigurationNoticeSource::Observation => "Configuration watching",
            ConfigurationNoticeSource::Resource => "Resource operation",
            ConfigurationNoticeSource::Operation => "Operation",
            ConfigurationNoticeSource::Command => "Command",
        }
    }

    pub(crate) fn message(&self) -> Option<String> {
        let messages = self
            .entries
            .iter()
            .filter(|entry| !entry.2)
            .map(|entry| entry.1.as_str())
            .collect::<Vec<_>>();
        (!messages.is_empty()).then(|| messages.join("\n\n"))
    }

    pub(crate) fn dismiss(&mut self, revision: u64) -> bool {
        if revision != self.revision
            || (self.deadline.is_none() && self.entries.iter().all(|entry| entry.2))
        {
            return false;
        }
        self.deadline = None;
        for entry in &mut self.entries {
            entry.2 = true;
        }
        true
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.dismiss(self.revision);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ConfigurationNoticeSource as Source;

    #[test]
    fn change_results_include_expiry_and_dismissal_history_but_exclude_no_ops() {
        let start = Instant::now();
        let mut notices = ConfigurationNotices::default();
        assert!(!notices.publish_at(Source::Reload, None, start));
        assert!(!notices.dismiss(0));
        assert!(notices.publish_at(Source::Reload, Some("Invalid settings".into()), start));
        let revision = notices.revision;
        let deadline = notices.deadline().unwrap();
        assert!(!notices.publish_at(
            Source::Reload,
            Some("Invalid settings".into()),
            start + std::time::Duration::from_secs(1),
        ));
        assert_eq!(notices.deadline(), Some(deadline));
        assert!(!notices.dismiss(revision + 1));
        assert!(
            notices.publish_at(Source::Reload, Some("Invalid settings".into()), deadline,),
            "an identical publication still commits overdue expiry"
        );
        assert!(notices.message().is_none());
        assert_eq!(notices.deadline(), None);
        assert!(!notices.dismiss(revision));
        assert!(!notices.publish_at(Source::Reload, Some("Invalid settings".into()), deadline,));
        assert!(
            notices.publish_at(Source::Reload, None, deadline),
            "recovery retires history"
        );
        assert!(notices.publish_at(Source::Reload, Some("Invalid settings".into()), deadline));
        assert!(notices.dismiss(notices.revision));
        assert!(!notices.dismiss(notices.revision));

        assert!(notices.publish_at(Source::Command, Some("Failed".into()), deadline));
        let next_deadline = notices.deadline().unwrap();
        assert!(
            notices.publish_at(Source::Observation, None, next_deadline),
            "even an absent-source clear must publish an overdue expiry"
        );
        assert!(notices.message().is_none());
    }

    #[test]
    fn headings_describe_only_visible_notice_sources() {
        let mut notices = ConfigurationNotices::default();
        notices.publish(Source::Reload, Some("Invalid settings".into()));
        assert_eq!(notices.title(), "Configuration reload");
        notices.publish(Source::Resource, Some("Resource failed".into()));
        assert_eq!(notices.title(), "Notifications");
        notices.publish(Source::Reload, None);
        assert_eq!(notices.title(), "Resource operation");
        notices.dismiss(notices.revision);
        notices.publish(Source::Command, Some("Command failed".into()));
        assert_eq!(
            notices.title(),
            "Command",
            "dismissed sources must not contaminate heading"
        );
    }

    #[test]
    fn long_notification_has_one_title_four_detail_lines_and_top_right_close() {
        use harbor_widget::{
            LucideIcon, renderer::Viewport, runtime::Runtime, scene::primitive::Primitive,
        };
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(640, 300, 1.0));
        runtime.set_root(configuration_notification(
            "Configuration reload",
            "Details ".repeat(1000),
            0.0,
            || {},
        ));
        runtime.update(Instant::now());
        let items = &runtime.pending_delta().unwrap().added;
        let text: Vec<_> = items
            .iter()
            .filter_map(|item| match &item.primitive {
                Primitive::Text {
                    text,
                    origin,
                    color,
                } => Some((text.as_ref(), *origin, *color)),
                _ => None,
            })
            .collect();
        assert_eq!(text.len(), 5);
        assert_eq!(text[0].0, "Configuration reload");
        assert!(text[0].2.a > text[1].2.a);
        assert!(text[4].0.ends_with('…'));
        let close = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::Icon {
                    icon: LucideIcon::X,
                    rect,
                    ..
                } => Some(rect),
                _ => None,
            })
            .unwrap();
        assert!(
            close.max.y < text[1].1.y,
            "close remains beside heading, not midway down detail"
        );
    }

    #[test]
    fn every_notice_including_errors_expires_at_three_seconds_without_resurrection() {
        let start = Instant::now();
        for source in [
            Source::Startup,
            Source::Reload,
            Source::Observation,
            Source::Resource,
            Source::Operation,
            Source::Command,
        ] {
            let mut notices = ConfigurationNotices::default();
            notices.publish_at(source, Some("Unrecovered error".into()), start);
            assert_eq!(notices.deadline(), Some(start + Duration::from_secs(3)));
            notices.publish_at(
                source,
                Some("Unrecovered error".into()),
                start + Duration::from_secs(2),
            );
            assert!(!notices.expire(start + Duration::from_millis(2999)));
            assert!(notices.message().is_some());
            assert!(notices.expire(start + Duration::from_secs(3)));
            assert_eq!(notices.message(), None);
            assert_eq!(notices.deadline(), None);
            assert!(!notices.expire(start + Duration::from_secs(4)));
            notices.publish_at(
                source,
                Some("Unrecovered error".into()),
                start + Duration::from_secs(5),
            );
            assert_eq!(
                notices.message(),
                None,
                "expiry preserves duplicate suppression"
            );
            assert_eq!(notices.deadline(), None);
            notices.publish_at(source, None, start + Duration::from_secs(6));
            notices.publish_at(
                source,
                Some("Unrecovered error".into()),
                start + Duration::from_secs(7),
            );
            assert!(
                notices.message().is_some(),
                "a new episode after recovery remains reportable"
            );
        }
    }

    #[test]
    fn changed_notice_gets_its_own_deadline_and_stale_dismissal_cannot_hide_it() {
        let start = Instant::now();
        let mut notices = ConfigurationNotices::default();
        notices.publish_at(Source::Reload, Some("First failure".into()), start);
        let stale = notices.revision;
        notices.publish_at(
            Source::Reload,
            Some("Different failure".into()),
            start + Duration::from_secs(2),
        );
        notices.dismiss(stale);
        assert_eq!(notices.deadline(), Some(start + Duration::from_secs(5)));
        assert!(!notices.expire(start + Duration::from_secs(3)));
        assert_eq!(notices.message().as_deref(), Some("Different failure"));
        assert!(notices.expire(start + Duration::from_secs(5)));
    }

    #[test]
    fn manual_dismissal_and_recovery_cancel_deadlines_without_extending_other_errors() {
        let start = Instant::now();
        let mut notices = ConfigurationNotices::default();
        notices.publish_at(Source::Reload, Some("Invalid settings".into()), start);
        notices.publish_at(
            Source::Resource,
            Some("Resource failed".into()),
            start + Duration::from_secs(1),
        );
        notices.publish_at(Source::Reload, None, start + Duration::from_secs(2));
        assert_eq!(notices.deadline(), Some(start + Duration::from_secs(4)));
        notices.dismiss(notices.revision);
        assert_eq!(notices.deadline(), None);
        assert!(!notices.expire(start + Duration::from_secs(4)));
        notices.publish_at(
            Source::Resource,
            Some("Different resource failed".into()),
            start + Duration::from_secs(5),
        );
        notices.publish_at(Source::Resource, None, start + Duration::from_secs(6));
        assert_eq!(notices.deadline(), None);
        assert_eq!(notices.message(), None);
    }

    #[test]
    fn publishing_after_a_missed_deadline_does_not_revive_old_errors() {
        let start = Instant::now();
        let mut notices = ConfigurationNotices::default();
        notices.publish_at(Source::Resource, Some("Old failure".into()), start);
        notices.publish_at(
            Source::Command,
            Some("New failure".into()),
            start + Duration::from_secs(4),
        );
        assert_eq!(notices.message().as_deref(), Some("New failure"));
        assert_eq!(notices.deadline(), Some(start + Duration::from_secs(7)));
    }

    #[test]
    fn notification_glass_and_top_right_close_share_search_feedback_at_multiple_scales() {
        use harbor_widget::{
            LucideIcon,
            input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent},
            layout::{Point, Rect},
            renderer::Viewport,
            runtime::Runtime,
            scene::primitive::Primitive,
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        fn close_icon(runtime: &Runtime) -> (Rect, Color) {
            let delta = runtime.pending_delta().unwrap();
            delta
                .added
                .iter()
                .chain(&delta.modified)
                .find_map(|item| match item.primitive {
                    Primitive::Icon {
                        icon: LucideIcon::X,
                        rect,
                        color,
                    } => Some((rect, color)),
                    _ => None,
                })
                .expect("vector close icon")
        }
        fn pointer(runtime: &mut Runtime, position: Point, phase: PointerPhase) {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                position,
                phase,
                PointerButton::Left,
                1,
            )));
            runtime.update(Instant::now());
        }
        for (width, height, scale) in [(800, 400, 1.0), (800, 400, 1.5), (320, 200, 1.0)] {
            let dismissed = Arc::new(AtomicUsize::new(0));
            let count = dismissed.clone();
            let mut runtime = Runtime::new();
            runtime.set_viewport(Viewport::new(width, height, scale));
            runtime.set_root(configuration_notification(
                "Configuration reload",
                "Invalid settings; previously accepted configuration remains effective".into(),
                0.0,
                move || {
                    count.fetch_add(1, Ordering::Relaxed);
                },
            ));
            runtime.update(Instant::now());
            let items = &runtime.pending_delta().unwrap().added;
            let appearance = GlassAppearance::default();
            let (index, panel) = items
                .iter()
                .enumerate()
                .find_map(|(i, item)| match item.primitive {
                    Primitive::BackdropRefract {
                        rect,
                        sigma,
                        corner_radii,
                        refraction,
                    } if sigma == appearance.blur_sigma
                        && refraction == appearance.refraction
                        && corner_radii == [6.0; 4] =>
                    {
                        Some((i, rect))
                    }
                    _ => None,
                })
                .expect("real shared backdrop filter, not opacity-only imitation");
            assert!(
                items
                    .iter()
                    .skip(index + 1)
                    .any(|item| matches!(item.primitive, Primitive::Text { .. })),
                "foreground is painted after filtering"
            );
            assert!(items.iter().any(|item| matches!(item.primitive, Primitive::PaintedQuad { paint, .. } if paint == appearance.panel_fill)));
            assert!(
                !items.iter().any(
                    |item| matches!(item.primitive, Primitive::Quad { color, .. } if color.a > 0.0)
                ),
                "no opaque inner card or independent close tile"
            );
            assert!(panel.size().width <= 520.0);
            assert!((panel.max.x - (width as f32 / scale - 12.0)).abs() < 0.01);
            let (close, normal) = close_icon(&runtime);
            assert_eq!(close.size().width, 18.0);
            assert_eq!(close.size().height, 18.0);
            assert!((close.min.y - (panel.min.y + 7.0)).abs() < 0.01);
            assert_eq!(normal.a, 0.78);
            let center = Point::new(
                (close.min.x + close.max.x) / 2.0,
                (close.min.y + close.max.y) / 2.0,
            );
            pointer(&mut runtime, center, PointerPhase::Move);
            assert_eq!(close_icon(&runtime).1.a, 1.0);
            pointer(&mut runtime, Point::ZERO, PointerPhase::Move);
            assert_eq!(
                close_icon(&runtime).1,
                normal,
                "leaving restores rest appearance"
            );
            pointer(&mut runtime, center, PointerPhase::Down);
            assert_eq!(close_icon(&runtime).1.a, 0.5);
            pointer(&mut runtime, center, PointerPhase::Cancel);
            assert_eq!(close_icon(&runtime).1, normal);
            assert_eq!(dismissed.load(Ordering::Relaxed), 0);
            pointer(&mut runtime, center, PointerPhase::Down);
            pointer(
                &mut runtime,
                Point::new(panel.min.x + 8.0, center.y),
                PointerPhase::Move,
            );
            pointer(
                &mut runtime,
                Point::new(panel.min.x + 8.0, center.y),
                PointerPhase::Up,
            );
            assert_eq!(
                dismissed.load(Ordering::Relaxed),
                0,
                "release outside close must cancel"
            );
            pointer(&mut runtime, center, PointerPhase::Down);
            pointer(&mut runtime, center, PointerPhase::Up);
            assert_eq!(dismissed.load(Ordering::Relaxed), 1);
            assert!(
                runtime.input().focused().is_none(),
                "close never steals focus"
            );
        }
    }
}
