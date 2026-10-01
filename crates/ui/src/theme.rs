use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};
#[derive(Clone, Copy)]
pub struct Theme {
    pub canvas: Color32,
    pub surface: Color32,
    pub control: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub danger: Color32,
    pub changed: Color32,
    pub cookie: Color32,
    pub cookie_edge: Color32,
    pub chip: Color32,
}
impl Theme {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                canvas: Color32::from_rgb(27, 29, 32),
                surface: Color32::from_rgb(35, 38, 42),
                control: Color32::from_rgb(49, 52, 57),
                text: Color32::from_rgb(240, 235, 225),
                muted: Color32::from_rgb(182, 179, 173),
                accent: Color32::from_rgb(239, 184, 105),
                danger: Color32::from_rgb(255, 146, 143),
                changed: Color32::from_rgb(88, 69, 34),
                cookie: Color32::from_rgb(225, 166, 87),
                cookie_edge: Color32::from_rgb(163, 100, 44),
                chip: Color32::from_rgb(81, 48, 29),
            }
        } else {
            Self {
                canvas: Color32::from_rgb(250, 247, 240),
                surface: Color32::from_rgb(255, 253, 248),
                control: Color32::from_rgb(233, 227, 216),
                text: Color32::from_rgb(51, 43, 35),
                muted: Color32::from_rgb(105, 94, 81),
                accent: Color32::from_rgb(133, 74, 31),
                danger: Color32::from_rgb(160, 43, 37),
                changed: Color32::from_rgb(246, 217, 164),
                cookie: Color32::from_rgb(226, 172, 97),
                cookie_edge: Color32::from_rgb(160, 102, 48),
                chip: Color32::from_rgb(80, 47, 29),
            }
        }
    }
    pub fn apply(self, ctx: &egui::Context, dark: bool) {
        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.panel_fill = self.canvas;
        v.window_fill = self.surface;
        v.extreme_bg_color = self.surface;
        v.faint_bg_color = self.control;
        v.override_text_color = Some(self.text);
        // Selected/pressed controls share the readable text role; keep their fill tonal.
        v.selection.bg_fill = self.changed;
        v.selection.stroke = Stroke::new(1.0, self.text);
        v.widgets.inactive.bg_fill = self.control;
        v.widgets.inactive.weak_bg_fill = self.control;
        v.widgets.hovered.bg_fill = self.changed;
        v.widgets.hovered.weak_bg_fill = self.changed;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.muted);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, self.text);
        // egui also uses active text for strong labels and headings.
        v.widgets.active.fg_stroke = Stroke::new(1.0, self.text);
        v.weak_text_color = Some(self.muted);
        v.widgets.active.bg_fill = self.changed;
        v.widgets.active.weak_bg_fill = self.changed;
        v.interact_cursor = Some(egui::CursorIcon::PointingHand);
        v.hyperlink_color = self.accent;
        v.error_fg_color = self.danger;
        let mode = if dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_theme(mode);
        ctx.set_visuals_of(mode, v);
        ctx.global_style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 8.0);
            s.spacing.button_padding = egui::vec2(12.0, 8.0);
            s.spacing.interact_size.y = 32.0;
            s.animation_time = 0.0;
            s.text_styles
                .insert(TextStyle::Body, FontId::proportional(16.0));
            s.text_styles
                .insert(TextStyle::Button, FontId::proportional(14.0));
            s.text_styles
                .insert(TextStyle::Small, FontId::proportional(12.0));
            s.text_styles
                .insert(TextStyle::Heading, FontId::proportional(28.0));
            s.text_styles
                .insert(TextStyle::Monospace, FontId::monospace(14.0));
        });
    }
}
pub fn number(n: u64) -> String {
    let raw = n.to_string();
    let mut out = String::new();
    for (i, c) in raw.chars().enumerate() {
        if i > 0 && (raw.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub fn compact(n: u64) -> String {
    for (base, suffix) in [
        (1_000_000_000_000_000_000, "Qi"),
        (1_000_000_000_000_000, "Qa"),
        (1_000_000_000_000, "T"),
        (1_000_000_000, "B"),
        (1_000_000, "M"),
    ] {
        if n >= base {
            return format!("{}.{:02}{suffix}", n / base, (n % base) / (base / 100));
        }
    }
    number(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(c: Color32) -> f64 {
        let channel = |v: u8| {
            let n = v as f64 / 255.0;
            if n <= 0.04045 {
                n / 12.92
            } else {
                ((n + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }
    pub(super) fn contrast(a: Color32, b: Color32) -> f64 {
        let a = luminance(a);
        let b = luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn every_control_state_has_readable_contrast() {
        for dark in [false, true] {
            let ctx = egui::Context::default();
            let t = Theme::new(dark);
            t.apply(&ctx, dark);
            let s = ctx.global_style();
            let v = &s.visuals;
            for bg in [
                t.canvas,
                t.surface,
                t.control,
                v.widgets.hovered.bg_fill,
                v.widgets.active.bg_fill,
                v.selection.bg_fill,
            ] {
                assert!(
                    contrast(v.text_color(), bg) >= 4.5,
                    "dark={dark}, text {:?}, background {:?}",
                    v.text_color(),
                    bg
                );
                assert!(contrast(v.strong_text_color(), bg) >= 4.5);
            }
            assert!(contrast(t.muted, t.canvas) >= 4.5);
        }
    }
}
