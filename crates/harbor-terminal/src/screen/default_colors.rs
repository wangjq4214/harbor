use harbor_config::{Palette, Rgba};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DefaultColorSlot {
    Foreground,
    Background,
    Cursor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DefaultColors {
    startup: Palette,
    active: Palette,
}

impl DefaultColors {
    pub(super) const fn new(startup: Palette) -> Self {
        Self {
            startup,
            active: startup,
        }
    }

    pub(super) const fn active_palette(self) -> Palette {
        self.active
    }

    pub(super) const fn get(self, slot: DefaultColorSlot) -> Rgba {
        match slot {
            DefaultColorSlot::Foreground => self.active.foreground,
            DefaultColorSlot::Background => self.active.background,
            DefaultColorSlot::Cursor => self.active.cursor,
        }
    }

    pub(super) fn set_rgb(&mut self, slot: DefaultColorSlot, rgb: [u8; 3]) -> bool {
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
        let color = match slot {
            DefaultColorSlot::Foreground => self.startup.foreground,
            DefaultColorSlot::Background => self.startup.background,
            DefaultColorSlot::Cursor => self.startup.cursor,
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
}
