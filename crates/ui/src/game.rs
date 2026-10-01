use crate::{
    CookieApp,
    theme::{compact, number},
};
use cookie_game::{
    ACHIEVEMENTS, GENERATORS, GameView, OrderKind, RECIPES, RushState, TICKS_PER_SECOND, UPGRADES,
};
use eframe::egui::{self, Align, Button, RichText, ScrollArea, Stroke, Vec2};
use vm_core::ExecutionBackend;
impl CookieApp {
    pub(crate) fn game_ui(&mut self, ui: &mut egui::Ui) {
        let view = match self.machine.view() {
            Ok(v) => v,
            Err(e) => {
                self.machine.set_paused(true);
                ui.heading("The bakery has a memory fault");
                ui.colored_label(self.theme().danger, e.to_string());
                ui.label("The simulation is paused. Repair the guest bytes or start a fresh game from File.");
                return;
            }
        };
        ScrollArea::vertical()
            .id_salt("game_scroll")
            .show(ui, |ui| {
                egui::Frame::NONE.inner_margin(16.0).show(ui, |ui| {
                    if ui.available_width() >= 760.0 {
                        let width = ui.available_width();
                        let left = (width - 24.0) * 0.43;
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(left, 0.0),
                                egui::Layout::top_down(Align::Min),
                                |ui| self.bakery_ui(ui, &view),
                            );
                            ui.add_space(16.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(width - left - 24.0, 0.0),
                                egui::Layout::top_down(Align::Min),
                                |ui| self.shop_ui(ui, &view),
                            );
                        });
                    } else {
                        self.bakery_ui(ui, &view);
                        ui.add_space(24.0);
                        self.shop_ui(ui, &view);
                    }
                });
            });
    }
    fn bakery_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        let theme = self.theme();
        ui.vertical_centered(|ui| {
            let balance = number(v.cookies);
            let font_size = (ui.available_width() / (balance.len() as f32 * 0.6)).clamp(24.0, 80.0);
            ui.label(RichText::new(balance).size(font_size).strong());
            ui.label(RichText::new("cookies available").color(theme.muted));
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("{} total cookies baked", number(v.lifetime)))
                    .size(14.0)
                    .color(theme.muted),
            );
            ui.add_space(8.0);
            ui.label(format!(
                "{} per click · {} per second",
                compact(v.click_power),
                compact(v.cps)
            ));
            ui.add_space(16.0);
            let size = ui.available_width().min(240.0);
            let (_, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
            let response = response.on_hover_text("Bake a cookie · Enter or Space when focused");
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Bake a cookie")
            });
            let center = response.rect.center();
            let radius = size * 0.43;
            let p = ui.painter();
            let fill = if response.is_pointer_button_down_on() {
                theme.cookie_edge
            } else {
                theme.cookie
            };
            p.circle_filled(center, radius, fill);
            p.circle_stroke(
                center,
                radius,
                Stroke::new(
                    if response.has_focus() { 3.0 } else { 2.0 },
                    if response.has_focus() {
                        theme.text
                    } else {
                        theme.cookie_edge
                    },
                ),
            );
            for (x, y, r) in [
                (-0.36, -0.44, 0.07),
                (0.17, -0.56, 0.06),
                (0.48, -0.14, 0.09),
                (-0.55, 0.13, 0.08),
                (-0.13, 0.00, 0.08),
                (0.09, 0.53, 0.09),
                (-0.32, 0.53, 0.05),
                (0.44, 0.37, 0.06),
            ] {
                p.circle_filled(center + egui::vec2(x, y) * radius, radius * r, theme.chip);
            }
            if response.clicked() {
                let r = self.machine.click();
                self.report(r);
            }
            if v.gift_available {
                ui.label(RichText::new("Golden batch").strong());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("7× baking boost").clicked() {
                        let r = self.machine.claim_gift();
                        self.report(r);
                    }
                    if ui
                        .button(format!(
                            "Cookie shower · +{}",
                            compact(v.activities.shower_reward)
                        ))
                        .clicked()
                    {
                        let r = self.machine.claim_cookie_shower();
                        self.report(r);
                    }
                });
            } else if v.boost_remaining > 0 {
                ui.label(
                    RichText::new(format!(
                        "Golden batch · {}s of 7× production",
                        v.boost_remaining.div_ceil(TICKS_PER_SECOND)
                    ))
                    .color(theme.accent),
                );
            }
            ui.add_space(16.0);
        });
        self.rush_ui(ui, v);
        self.enemy_ui(ui, v);
        ui.add_space(16.0);
        ui.collapsing(&v.name, |ui| {
            ui.label("Bakery name");
            ui.text_edit_singleline(&mut self.name_draft);
            if ui.button("Rename bakery").clicked() {
                let bytes = self.name_draft.as_bytes().to_vec();
                let r = self.machine.rename(&bytes);
                self.report(r);
            }
        });
        ui.collapsing("Statistics", |ui| {
            egui::Grid::new("statistics").num_columns(2).show(ui, |ui| {
                for (label, value) in [
                    ("Manual clicks", number(v.clicks)),
                    ("Cookies spent", number(v.spent)),
                    (
                        "Session time",
                        format!(
                            "{}m {}s",
                            v.play_ticks / TICKS_PER_SECOND / 60,
                            v.play_ticks / TICKS_PER_SECOND % 60
                        ),
                    ),
                    ("Prestige", number(v.prestige)),
                    ("Orders completed", number(v.activities.orders_done)),
                    ("Baking rushes won", number(v.activities.rush_wins)),
                    ("Visitors shooed", number(v.encounter.defeated)),
                ] {
                    ui.label(label);
                    ui.label(value);
                    ui.end_row();
                }
            });
        });
        ui.collapsing(
            format!(
                "Achievements · {}/{}",
                v.achievements.iter().filter(|x| **x).count(),
                ACHIEVEMENTS.len()
            ),
            |ui| {
                for (i, (name, threshold)) in ACHIEVEMENTS.iter().enumerate() {
                    ui.label(format!(
                        "{}  {name} · {} lifetime cookies",
                        if v.achievements[i] { "✓" } else { "○" },
                        compact(*threshold)
                    ));
                }
            },
        );
        ui.collapsing("Prestige",|ui|{
            ui.label(format!("{} prestige points available. Each point adds 100% to base production.",v.prestige_gain));
            ui.small("Earn a point at one million lifetime cookies. Future points require increasingly larger totals.");
            if ui.add_enabled(v.prestige_gain>0,Button::new("Reset bakery for prestige…")).clicked(){self.confirm_prestige=true;}
        });
    }
    fn shop_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        ui.scope(|ui| {
            ui.spacing_mut().button_padding.x = 8.0;
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.shop_tab, 0, "Generators");
                ui.selectable_value(&mut self.shop_tab, 1, "Upgrades");
                let ready = v
                    .activities
                    .orders
                    .iter()
                    .filter(|o| o.progress >= o.target)
                    .count();
                ui.selectable_value(
                    &mut self.shop_tab,
                    2,
                    if ready > 0 {
                        format!("Orders ({ready})")
                    } else {
                        "Orders".into()
                    },
                );
                ui.selectable_value(&mut self.shop_tab, 3, "Recipes");
            });
        });
        ui.add_space(16.0);
        if self.shop_tab == 0 {
            for (i, spec) in GENERATORS.iter().enumerate() {
                let g = &v.generators[i];
                let muted = self.theme().muted;
                activity_row(
                    ui,
                    |ui| {
                        ui.label(RichText::new(spec.name).strong());
                        ui.label(
                            RichText::new(format!(
                                "{} owned · {} /s each",
                                compact(g.owned),
                                compact(g.per_unit)
                            ))
                            .small()
                            .color(muted),
                        )
                        .on_hover_text(format!(
                            "{} cookies per second from all {}",
                            number(g.production),
                            spec.name
                        ));
                    },
                    |ui| {
                        if ui
                            .add_enabled(
                                v.cookies >= g.price,
                                Button::new(format!("Buy · {}", compact(g.price))),
                            )
                            .clicked()
                        {
                            let r = self.machine.buy_generator(i);
                            self.report(r);
                        }
                    },
                );
                ui.add_space(16.0);
            }
        } else if self.shop_tab == 1 {
            let muted = self.theme().muted;
            for (i, spec) in UPGRADES.iter().enumerate() {
                let unlocked = v.lifetime >= spec.unlock;
                activity_row(
                    ui,
                    |ui| {
                        ui.label(RichText::new(spec.name).strong());
                        ui.small(spec.description);
                        if !unlocked {
                            ui.label(
                                RichText::new(format!(
                                    "Unlocks at {} total cookies",
                                    compact(spec.unlock)
                                ))
                                .small()
                                .color(muted),
                            );
                        }
                    },
                    |ui| {
                        if v.upgrades[i] {
                            ui.label("Owned");
                        } else if ui
                            .add_enabled(
                                unlocked && v.cookies >= spec.price,
                                Button::new(format!("Buy · {}", compact(spec.price))),
                            )
                            .on_hover_text(format!("Costs {} cookies", number(spec.price)))
                            .clicked()
                        {
                            let r = self.machine.buy_upgrade(i);
                            self.report(r);
                        }
                    },
                );
                ui.add_space(16.0);
            }
        } else if self.shop_tab == 2 {
            self.orders_ui(ui, v);
        } else {
            self.recipes_ui(ui, v);
        }
    }
    fn rush_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        let a = &v.activities;
        if matches!(a.rush_state, RushState::Idle | RushState::Missed) {
            return;
        }
        activity_row(
            ui,
            |ui| {
                ui.label(RichText::new("Baking rush").strong());
                if a.rush_state == RushState::Offered {
                    ui.small(format!(
                        "{} clicks in 20s · +{} cookies",
                        number(a.rush_target),
                        compact(a.rush_reward)
                    ));
                } else if a.rush_state == RushState::Running {
                    ui.small(format!(
                        "{}s left",
                        a.rush_remaining.div_ceil(TICKS_PER_SECOND)
                    ));
                }
            },
            |ui| match a.rush_state {
                RushState::Offered => {
                    if ui
                        .button("Start rush")
                        .on_hover_text(format!(
                            "Offer closes in {}s",
                            a.rush_remaining.div_ceil(TICKS_PER_SECOND)
                        ))
                        .clicked()
                    {
                        let r = self.machine.start_rush();
                        self.report(r);
                    }
                }
                RushState::Ready => {
                    if ui
                        .button(format!("Collect · +{}", compact(a.rush_reward)))
                        .clicked()
                    {
                        let r = self.machine.claim_rush();
                        self.report(r);
                        if !self.error {
                            self.status =
                                format!("Rush complete! +{} cookies", number(a.rush_reward));
                        }
                    }
                }
                _ => {}
            },
        );
        if a.rush_state == RushState::Running {
            ui.add(
                egui::ProgressBar::new(progress(a.rush_progress, a.rush_target)).text(format!(
                    "{} / {} clicks",
                    compact(a.rush_progress),
                    compact(a.rush_target)
                )),
            );
        }
        ui.add_space(16.0);
    }
    fn enemy_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        let Some(name) = v.encounter.name else {
            return;
        };
        let theme = self.theme();
        activity_row(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
                    let c = rect.center();
                    let p = ui.painter();
                    p.circle_filled(c + egui::vec2(-12.0, -12.0), 7.0, theme.cookie_edge);
                    p.circle_filled(c + egui::vec2(12.0, -12.0), 7.0, theme.cookie_edge);
                    p.circle_filled(c, 17.0, theme.cookie_edge);
                    for x in [-6.0, 6.0] {
                        p.circle_filled(c + egui::vec2(x, -4.0), 5.0, theme.surface);
                        p.circle_filled(c + egui::vec2(x, -3.0), 2.5, theme.text);
                    }
                    p.circle_filled(c + egui::vec2(0.0, 12.0), 7.0, theme.cookie);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(name).strong());
                        ui.small(format!(
                            "{} /s · {} eaten",
                            compact(v.encounter.bite),
                            compact(v.encounter.stolen)
                        ));
                    });
                });
            },
            |ui| {
                if ui
                    .button(format!("Shoo · {}", number(v.encounter.health)))
                    .on_hover_text("Shoo it away to recover the cookies it ate")
                    .clicked()
                {
                    match self.machine.shoo_enemy() {
                        Ok(recovered) => {
                            self.error = false;
                            if v.encounter.health <= 1 {
                                self.status = format!(
                                    "Visitor shooed away · {} cookies recovered",
                                    number(recovered)
                                );
                            }
                        }
                        Err(e) => self.report::<(), _>(Err(e)),
                    }
                }
            },
        );
        ui.add_space(16.0);
    }
    fn orders_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        for (i, o) in v.activities.orders.iter().enumerate() {
            ui.push_id(("order", i), |ui| {
                activity_row(
                    ui,
                    |ui| {
                        ui.label(RichText::new(o.customer).strong());
                        ui.label(match o.kind {
                            OrderKind::Bake => format!("Bake {} new cookies", compact(o.target)),
                            OrderKind::Click => {
                                format!("Click the cookie {} times", compact(o.target))
                            }
                            OrderKind::Generator => format!(
                                "Own {} × {}",
                                compact(o.target),
                                GENERATORS[o.generator].name,
                            ),
                        });
                    },
                    |ui| {
                        if ui
                            .add_enabled(
                                o.progress >= o.target,
                                Button::new(format!("Collect · +{}", compact(o.reward))),
                            )
                            .clicked()
                        {
                            let r = self.machine.claim_order(i);
                            self.report(r);
                            if !self.error {
                                self.status = format!(
                                    "{} order complete! +{} cookies",
                                    o.customer,
                                    number(o.reward)
                                );
                            }
                        }
                    },
                );
                ui.add(
                    egui::ProgressBar::new(progress(o.progress, o.target)).text(format!(
                        "{} / {}",
                        compact(o.progress.min(o.target)),
                        compact(o.target)
                    )),
                );
                ui.add_space(24.0);
            });
        }
    }
    fn recipes_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        for (i, recipe) in RECIPES.iter().enumerate() {
            ui.push_id(("recipe", i), |ui| {
                activity_row(
                    ui,
                    |ui| {
                        ui.label(RichText::new(recipe.name).strong());
                        ui.small(recipe.description);
                    },
                    |ui| {
                        if v.activities.recipe == i {
                            ui.label("In use");
                        } else {
                            let unlocked = v.activities.recipes_unlocked[i];
                            if ui
                                .add_enabled(
                                    unlocked || v.cookies >= recipe.price,
                                    Button::new(if unlocked {
                                        "Use recipe".into()
                                    } else {
                                        format!("Unlock · {}", compact(recipe.price))
                                    }),
                                )
                                .clicked()
                            {
                                let r = self.machine.select_recipe(i);
                                self.report(r);
                            }
                        }
                    },
                );
                ui.add_space(24.0);
            });
        }
    }
}
fn activity_row(
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut egui::Ui),
    action: impl FnOnce(&mut egui::Ui),
) {
    let width = ui.available_width();
    let content_width = (width - (width * 0.44).min(164.0) - ui.spacing().item_spacing.x).max(0.0);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(content_width, 0.0),
            egui::Layout::top_down(Align::Min),
            |ui| {
                ui.set_max_width(content_width);
                content(ui);
            },
        );
        ui.with_layout(egui::Layout::right_to_left(Align::Center), action);
    });
}
fn progress(current: u64, target: u64) -> f32 {
    if target == 0 {
        1.0
    } else {
        (current as f64 / target as f64).clamp(0.0, 1.0) as f32
    }
}
