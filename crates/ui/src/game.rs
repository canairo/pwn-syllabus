use crate::{
    CookieApp,
    theme::{compact, number},
};
use cookie_game::{
    ACHIEVEMENTS, GENERATORS, GameView, OrderKind, RECIPES, RushState, TICKS_PER_SECOND, UPGRADES,
};
use eframe::egui::{self, Align, Button, FontId, RichText, ScrollArea, Stroke, Vec2};
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
                    if ui.available_width() >= 600.0 {
                        ui.columns(2, |cols| {
                            self.bakery_ui(&mut cols[0], &view);
                            self.shop_ui(&mut cols[1], &view);
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
            ui.label(RichText::new(&v.name).size(20.0));
            ui.add_space(16.0);
            let total = number(v.lifetime);
            let font_size = (ui.available_width() / (total.len() as f32 * 0.6)).clamp(24.0, 80.0);
            ui.label(RichText::new(total).size(font_size).strong());
            ui.label(RichText::new("total cookies").color(theme.muted));
            ui.add_space(8.0);
            ui.label(RichText::new(format!("{} available to spend", number(v.cookies))).size(20.0));
            ui.add_space(8.0);
            ui.label(format!(
                "{} per click · {} per second",
                compact(v.click_power),
                compact(v.cps)
            ));
            if ui
                .link(format!("Recipe: {}", RECIPES[v.activities.recipe].name))
                .clicked()
            {
                self.shop_tab = 3;
            }
            ui.add_space(16.0);
            let size = ui.available_width().min(240.0);
            let (_, response) =
                ui.allocate_exact_size(Vec2::new(size, size + 36.0), egui::Sense::click());
            let response = response.on_hover_text("Bake a cookie · Enter or Space when focused");
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Bake a cookie")
            });
            let center = response.rect.center() - egui::vec2(0.0, 18.0);
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
            p.text(
                egui::pos2(center.x, response.rect.bottom() - 16.0),
                egui::Align2::CENTER_CENTER,
                "Bake a cookie",
                FontId::proportional(16.0),
                theme.accent,
            );
            if response.clicked() {
                let r = self.machine.click();
                self.report(r);
            }
            if v.gift_available {
                ui.label(RichText::new("A golden batch! Choose your reward.").strong());
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
        ui.add_space(16.0);
        ui.collapsing("Bakery & recipe", |ui| {
            ui.label("Bakery name");
            ui.text_edit_singleline(&mut self.name_draft);
            if ui.button("Rename bakery").clicked() {
                let bytes = self.name_draft.as_bytes().to_vec();
                let r = self.machine.rename(&bytes);
                self.report(r);
            }
            ui.add_space(8.0);
            ui.label("Recipe label");
            ui.text_edit_singleline(&mut self.recipe_draft);
            if ui.button("Label recipe").clicked() {
                let bytes = self.recipe_draft.as_bytes().to_vec();
                let r = self.machine.label_recipe(&bytes);
                self.report(r);
            }
            ui.label(format!(
                "Recipe multiplier: {}×",
                compact(v.recipe_multiplier)
            ));
            if ui
                .add_enabled(
                    !v.charm_traded,
                    Button::new("Trade lucky charm for a coupon"),
                )
                .clicked()
            {
                let r = self.machine.trade_charm();
                self.report(r);
            }
            if v.charm_traded {
                ui.small("Charm traded. The coupon now occupies its former space.");
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
                    format!("Orders · {ready} ready")
                } else {
                    "Orders".into()
                },
            );
            ui.selectable_value(&mut self.shop_tab, 3, "Recipes");
        });
        ui.add_space(16.0);
        if self.shop_tab == 0 {
            for (i, spec) in GENERATORS.iter().enumerate() {
                let g = &v.generators[i];
                ui.horizontal(|ui|{
                    ui.vertical(|ui|{
                        ui.horizontal_wrapped(|ui|{ui.label(RichText::new(spec.name).strong());ui.label(RichText::new(format!("{} cookies/s each",compact(g.per_unit))).small().color(self.theme().muted));});
                        ui.label(RichText::new(format!("{} owned · {} cookies/s total",compact(g.owned),compact(g.production))).small().color(self.theme().muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(Align::Center),|ui|{
                        if ui.add_enabled(v.cookies>=g.price,Button::new(format!("Buy · {}",compact(g.price)))).on_hover_text(format!("Costs {} cookies. Each produces {} cookies per second before multipliers.",number(g.price),number(spec.rate))).clicked(){let r=self.machine.buy_generator(i);self.report(r);}
                    });
                });
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
        ui.horizontal(|ui| {
            ui.label(RichText::new("Baking rush").strong());
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                match a.rush_state {
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
                    RushState::Running => {
                        ui.label(format!(
                            "{}s left",
                            a.rush_remaining.div_ceil(TICKS_PER_SECOND)
                        ));
                    }
                    RushState::Idle | RushState::Missed => {
                        if ui
                            .add_enabled(
                                a.rush_cooldown == 0,
                                Button::new(if a.rush_state == RushState::Missed {
                                    "Try again"
                                } else {
                                    "Start rush"
                                }),
                            )
                            .clicked()
                        {
                            let r = self.machine.start_rush();
                            self.report(r);
                        }
                    }
                }
            });
        });
        match a.rush_state {
            RushState::Running => {
                ui.add(
                    egui::ProgressBar::new(progress(a.rush_progress, a.rush_target)).text(format!(
                        "{} / {} clicks · reward {} cookies",
                        number(a.rush_progress),
                        number(a.rush_target),
                        compact(a.rush_reward)
                    )),
                );
            }
            RushState::Ready => {
                ui.small("You did it! Your reward is ready to collect.");
            }
            RushState::Missed => {
                ui.small("Time’s up. No cookies lost — have another go!");
            }
            RushState::Idle => {
                ui.small(if a.rush_cooldown > 0 {
                    format!(
                        "Next rush in {}s",
                        a.rush_cooldown.div_ceil(TICKS_PER_SECOND)
                    )
                } else {
                    "Click the cookie to beat a 20-second challenge and earn a bonus.".into()
                });
            }
        }
    }
    fn orders_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        ui.label("Bake for your customers and collect bonus cookies.");
        ui.label(
            RichText::new("All orders progress together. Rewards cost no cookies.")
                .small()
                .color(self.theme().muted),
        );
        ui.add_space(16.0);
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
        ui.small(format!(
            "{} orders completed",
            number(v.activities.orders_done)
        ));
    }
    fn recipes_ui(&mut self, ui: &mut egui::Ui, v: &GameView) {
        ui.label("Choose what your bakery does best.");
        ui.label(
            RichText::new("Unlock once, then switch freely. One recipe active at a time.")
                .small()
                .color(self.theme().muted),
        );
        ui.add_space(16.0);
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
