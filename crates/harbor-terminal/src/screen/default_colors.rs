use harbor_config::{Palette, Rgba};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DefaultColorSlot {
    Foreground,
    Background,
    Cursor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DefaultColors {
    baseline: Palette,
    active: Palette,
    defaults: [Option<[u8; 3]>; 3],
    indexed_overrides: [Option<[u8; 3]>; 256],
}

impl DefaultColors {
    pub(super) const fn new(baseline: Palette) -> Self {
        Self {
            baseline,
            active: baseline,
            defaults: [None; 3],
            indexed_overrides: [None; 256],
        }
    }

    pub(super) fn advance_baseline(&mut self, baseline: Palette) {
        self.baseline = baseline;
        self.active = baseline;
        for (slot, rgb) in [
            DefaultColorSlot::Foreground,
            DefaultColorSlot::Background,
            DefaultColorSlot::Cursor,
        ]
        .into_iter()
        .zip(self.defaults)
        {
            if let Some(rgb) = rgb {
                self.set_rgb(slot, rgb);
            }
        }
        for index in 0..=255u8 {
            if let Some(rgb) = self.indexed_overrides[usize::from(index)] {
                self.set_indexed_rgb(index, rgb);
            }
        }
    }
    pub(super) fn indexed(&self, index: u8) -> Rgba {
        self.active.indexed(index)
    }

    pub(super) fn set_indexed_rgb(&mut self, index: u8, rgb: [u8; 3]) -> bool {
        self.indexed_overrides[usize::from(index)] = Some(rgb);
        let alpha = self.active.indexed(index).components()[3];
        self.replace_indexed(
            index,
            Rgba::new(
                rgb[0] as f32 / 255.0,
                rgb[1] as f32 / 255.0,
                rgb[2] as f32 / 255.0,
                alpha,
            ),
        )
    }

    pub(super) fn reset_indexed(&mut self, index: u8) -> bool {
        self.indexed_overrides[usize::from(index)] = None;
        self.replace_indexed(index, self.baseline.indexed(index))
    }

    fn replace_indexed(&mut self, index: u8, color: Rgba) -> bool {
        let target = self.active.indexed_mut(index);
        if *target == color {
            false
        } else {
            *target = color;
            true
        }
    }

    pub(super) const fn active_palette(&self) -> Palette {
        self.active
    }

    pub(super) const fn get(&self, slot: DefaultColorSlot) -> Rgba {
        match slot {
            DefaultColorSlot::Foreground => self.active.foreground,
            DefaultColorSlot::Background => self.active.background,
            DefaultColorSlot::Cursor => self.active.cursor,
        }
    }

    pub(super) fn set_rgb(&mut self, slot: DefaultColorSlot, rgb: [u8; 3]) -> bool {
        self.defaults[slot as usize] = Some(rgb);
        let current = self.get(slot);
        let [_, _, _, alpha] = current.components();
        self.replace(
            slot,
            Rgba::new(
                rgb[0] as f32 / 255.0,
                rgb[1] as f32 / 255.0,
                rgb[2] as f32 / 255.0,
                alpha,
            ),
        )
    }

    pub(super) fn reset(&mut self, slot: DefaultColorSlot) -> bool {
        self.defaults[slot as usize] = None;
        let color = match slot {
            DefaultColorSlot::Foreground => self.baseline.foreground,
            DefaultColorSlot::Background => self.baseline.background,
            DefaultColorSlot::Cursor => self.baseline.cursor,
        };
        self.replace(slot, color)
    }

    fn replace(&mut self, slot: DefaultColorSlot, color: Rgba) -> bool {
        let target = match slot {
            DefaultColorSlot::Foreground => &mut self.active.foreground,
            DefaultColorSlot::Background => &mut self.active.background,
            DefaultColorSlot::Cursor => &mut self.active.cursor,
        };
        if *target == color {
            false
        } else {
            *target = color;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_preserves_alpha_and_reset_restores_startup_rgba() {
        let startup = Palette {
            background: Rgba::from_rgba8(1, 2, 3, 64),
            ..Palette::default()
        };
        let mut colors = DefaultColors::new(startup);

        assert!(colors.set_rgb(DefaultColorSlot::Background, [10, 20, 30]));
        assert_eq!(
            colors.get(DefaultColorSlot::Background),
            Rgba::from_rgba8(10, 20, 30, 64)
        );
        assert!(colors.reset(DefaultColorSlot::Background));
        assert_eq!(colors.get(DefaultColorSlot::Background), startup.background);
    }
    #[test]
    fn reload_preserves_override_identity_and_uses_new_alpha_and_reset_baseline() {
        fn rgb8(color: Rgba) -> [u8; 3] {
            let c = color.components();
            [c[0], c[1], c[2]].map(|channel| (channel * 255.0).round() as u8)
        }
        let old = Palette::default();
        let mut colors = DefaultColors::new(old);
        colors.set_rgb(DefaultColorSlot::Foreground, rgb8(old.foreground));
        colors.set_indexed_rgb(200, rgb8(old.indexed(200)));
        let mut next = old;
        next.foreground = Rgba::from_rgba8(1, 2, 3, 77);
        next.background = Rgba::from_rgba8(4, 5, 6, 88);
        next.selection = Rgba::from_rgba8(7, 8, 9, 99);
        *next.indexed_mut(200) = Rgba::from_rgba8(10, 11, 12, 66);
        colors.advance_baseline(next);
        assert_eq!(
            rgb8(colors.get(DefaultColorSlot::Foreground)),
            rgb8(old.foreground)
        );
        assert_eq!(
            colors.get(DefaultColorSlot::Foreground).components()[3],
            next.foreground.components()[3]
        );
        assert_eq!(rgb8(colors.indexed(200)), rgb8(old.indexed(200)));
        assert_eq!(
            colors.indexed(200).components()[3],
            next.indexed(200).components()[3]
        );
        assert_eq!(colors.get(DefaultColorSlot::Background), next.background);
        assert_eq!(colors.active_palette().selection, next.selection);
        colors.reset(DefaultColorSlot::Foreground);
        colors.reset_indexed(200);
        assert_eq!(colors.get(DefaultColorSlot::Foreground), next.foreground);
        assert_eq!(colors.indexed(200), next.indexed(200));
        let mut third = next;
        third.foreground = Rgba::from_rgba8(20, 21, 22, 23);
        *third.indexed_mut(200) = Rgba::from_rgba8(24, 25, 26, 27);
        colors.advance_baseline(third);
        assert_eq!(colors.active_palette(), third);
    }
}
