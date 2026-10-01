use crate::{CookieApp, theme::number};
use debugger::{
    ScanFilter, Scanner, ValueType, Watch, diff, parse_hex, parse_u64, pointer_label, unsigned,
};
use eframe::egui::{self, Button, RichText, ScrollArea};
use egui_dock::{DockArea, DockState, TabViewer};
use vm_core::{Address, Architecture, ExecutionBackend, Memory, RegionKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Memory,
    Scanner,
    Inspector,
    Watch,
    History,
    Diff,
}
impl Tab {
    fn title(self) -> &'static str {
        match self {
            Self::Memory => "Memory",
            Self::Scanner => "Scanner",
            Self::Inspector => "Inspector",
            Self::Watch => "Watch",
            Self::History => "Write History",
            Self::Diff => "Memory Diff",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanFormat {
    Number,
    Text,
    HexBytes,
}
impl ScanFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::Text => "Text (UTF-8)",
            Self::HexBytes => "Hex bytes",
        }
    }
    fn bytes(self, value: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Text => Ok(value.as_bytes().to_vec()),
            Self::HexBytes => parse_hex(value),
            Self::Number => Err("select text or bytes".into()),
        }
    }
    fn display(self, bytes: &[u8]) -> String {
        match self {
            Self::Number => number(unsigned(bytes)),
            Self::Text => String::from_utf8_lossy(bytes).escape_debug().to_string(),
            Self::HexBytes => hex(bytes),
        }
    }
}
pub(crate) struct DebugWorkspace {
    dock: DockState<Tab>,
    pending_tools: Vec<Tab>,
    selected: Address,
    anchor: Address,
    selection_end: Address,
    jump: String,
    scroll_to: Option<f32>,
    back: Vec<Address>,
    bookmarks: Vec<Address>,
    symbols: bool,
    scanner: Scanner,
    scan_value: String,
    scan_width: usize,
    scan_unknown: bool,
    scan_aligned: bool,
    scan_filter: ScanFilter,
    watches: Vec<Watch>,
    watch_name: String,
    snapshot: Option<Memory>,
    diffs: Vec<debugger::Diff>,
    inline_edit: Option<(Address, String)>,
    scan_format: ScanFormat,
    result_format: ScanFormat,
    result_page: usize,
}
fn layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![]);
    *dock.main_surface_mut() = egui_dock::Tree::default();
    dock
}
impl DebugWorkspace {
    pub(crate) fn new() -> Self {
        Self {
            dock: layout(),
            pending_tools: vec![],
            selected: Architecture::HEAP,
            anchor: Architecture::HEAP,
            selection_end: Architecture::HEAP,
            jump: format!("0x{:016X}", Architecture::HEAP),
            scroll_to: None,
            back: vec![],
            bookmarks: vec![],
            symbols: false,
            scanner: Scanner::default(),
            scan_value: "0".into(),
            scan_width: 8,
            scan_unknown: false,
            scan_aligned: true,
            scan_filter: ScanFilter::Exact,
            watches: vec![],
            watch_name: String::new(),
            snapshot: None,
            diffs: vec![],
            inline_edit: None,
            scan_format: ScanFormat::Number,
            result_format: ScanFormat::Number,
            result_page: 0,
        }
    }
    pub(crate) fn menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Memory tools", |ui| {
            ui.small("Open a tool when you need it. Drag its title to move it.");
            if ui.button("Find a value…").clicked() {
                self.open_tool(Tab::Scanner, ui.ctx());
                ui.close();
            }
            if ui.button("Inspect memory…").clicked() {
                self.open_tool(Tab::Memory, ui.ctx());
                ui.close();
            }
            ui.menu_button("More tools", |ui| {
                for tab in [Tab::Watch, Tab::History, Tab::Diff] {
                    if ui.button(tab.title()).clicked() {
                        self.open_tool(tab, ui.ctx());
                        ui.close();
                    }
                }
            });
            if ui.button("Close all tools").clicked() {
                self.dock = layout();
                ui.close();
            }
        });
    }
    fn open_tool(&mut self, tab: Tab, ctx: &egui::Context) {
        if let Some(path) = self.dock.find_tab(&tab) {
            let _ = self.dock.set_active_tab(path);
            return;
        }
        let bounds = ctx.content_rect();
        let count = self.dock.iter_all_tabs().count();
        let width: f32 = if tab == Tab::Memory { 760.0 } else { 420.0 };
        let size = egui::vec2(
            width.min(bounds.width() - 32.0),
            (bounds.height() - 120.0).clamp(240.0, 560.0),
        );
        let surface = self.dock.add_window(vec![tab]);
        if let Some(window) = self.dock.get_window_state_mut(surface) {
            window.set_position(egui::pos2(
                (bounds.right() - size.x - 24.0).max(16.0),
                64.0 + (count % 3) as f32 * 24.0,
            ));
            window.set_size(size);
        }
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, app: &mut CookieApp) {
        let mut dock = std::mem::replace(&mut self.dock, layout());
        // Only floating surfaces are populated; the game keeps the entire main surface.
        let mut host = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
            ui.max_rect().min,
            egui::Vec2::ZERO,
        )));
        DockArea::new(&mut dock)
            .style(egui_dock::Style::from_egui(ui.style()))
            .show_inside(
                &mut host,
                &mut Viewer {
                    workspace: self,
                    app,
                },
            );
        self.dock = dock;
        for tab in std::mem::take(&mut self.pending_tools) {
            self.open_tool(tab, ui.ctx());
        }
    }
    fn navigate(&mut self, a: Address, app: &mut CookieApp) {
        if let Some(r) = app
            .machine
            .memory_regions()
            .iter()
            .find(|r| r.kind() == RegionKind::Heap && r.contains(a))
        {
            if a != self.selected {
                self.back.push(self.selected);
                if self.back.len() > 128 {
                    self.back.remove(0);
                }
            }
            self.pending_tools.push(Tab::Memory);
            self.selected = a;
            self.anchor = a;
            self.selection_end = a;
            self.jump = format!("0x{a:016X}");
            self.inline_edit = None;
            self.scroll_to = Some(((a - r.base()) / 16) as f32 * 32.0);
        } else {
            app.report::<(), _>(Err(format!(
                "Memory shows heap addresses only: 0x{a:016X} is outside the heap"
            )));
        }
    }
    fn selection(&self) -> (Address, u64) {
        let a = self.anchor.min(self.selection_end);
        let end = self.anchor.max(self.selection_end);
        (a, (end - a + 1).min(4096))
    }
    fn follow(&mut self, app: &mut CookieApp) {
        match app.machine.read_memory(self.selected, 8) {
            Ok(b) => self.navigate(unsigned(&b), app),
            Err(e) => app.report::<(), _>(Err(e)),
        }
    }
    fn add_watch(&mut self, kind: ValueType) {
        if !self
            .watches
            .iter()
            .any(|w| w.address == self.selected && w.kind == kind)
        {
            self.watches.push(Watch {
                address: self.selected,
                kind,
                name: if self.watch_name.is_empty() {
                    kind.label().into()
                } else {
                    self.watch_name.clone()
                },
            });
        }
    }
    fn memory_ui(&mut self, ui: &mut egui::Ui, app: &mut CookieApp) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Address");
            ui.add(
                egui::TextEdit::singleline(&mut self.jump)
                    .desired_width(190.0)
                    .font(egui::TextStyle::Monospace),
            );
            if ui.button("Go").clicked() {
                match parse_u64(&self.jump) {
                    Ok(a) => self.navigate(a, app),
                    Err(e) => app.report::<(), _>(Err(e)),
                }
            }
            if ui
                .add_enabled(!self.back.is_empty(), Button::new("Back"))
                .clicked()
                && let Some(a) = self.back.pop()
            {
                let history = self.back.clone();
                self.navigate(a, app);
                self.back = history;
            }
            if ui.button("Follow pointer").clicked() {
                self.follow(app);
            }
            if ui.button("Inspector").clicked() {
                self.pending_tools.push(Tab::Inspector);
            }
        });
        let (start, n) = self.selection();
        let Some(r) = app
            .machine
            .memory_regions()
            .iter()
            .find(|r| r.kind() == RegionKind::Heap)
        else {
            ui.label("No heap memory mapped");
            return;
        };
        let base = r.base();
        let len = r.len();
        let rows = len.div_ceil(16) as usize;
        let changed = self
            .snapshot
            .as_ref()
            .map(|s| {
                diff(s, &app.machine)
                    .into_iter()
                    .map(|d| d.address)
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();
        let mut scroll = ScrollArea::both()
            .id_salt("hex_scroll")
            .auto_shrink([false, false]);
        if let Some(offset) = self.scroll_to.take() {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let mut byte_commit = None;
        let mut cancel_edit = false;
        scroll.show_rows(ui, 24.0, rows, |ui, range| {
            ui.set_min_width(740.0);
            ui.spacing_mut().button_padding = egui::Vec2::ZERO;
            ui.spacing_mut().item_spacing.x = 4.0;
            for row in range {
                let a = base + row as u64 * 16;
                let count = (len - row as u64 * 16).min(16);
                let bytes = match app.machine.read_memory(a, count) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{a:016X}"))
                            .monospace()
                            .color(app.theme().muted),
                    );
                    for (i, b) in bytes.iter().enumerate() {
                        let addr = a + i as u64;
                        let selected = addr >= start && addr < start + n;
                        if i == 8 {
                            ui.add_space(8.0);
                        }
                        let mut text = RichText::new(format!("{b:02X}")).monospace();
                        if changed.contains(&addr) {
                            text = text.background_color(app.theme().changed);
                        }
                        if let Some((editing, text)) = &mut self.inline_edit
                            && *editing == addr
                        {
                            let id = egui::Id::new(("byte_edit", addr));
                            let response = ui.add(
                                egui::TextEdit::singleline(text)
                                    .id(id)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(24.0)
                                    .hint_text(format!("{b:02X}"))
                                    .char_limit(512),
                            );
                            if response.has_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                cancel_edit = true;
                            } else if response.changed()
                                && response.has_focus()
                                && parse_hex(text).is_ok_and(|b| !b.is_empty())
                            {
                                byte_commit = Some((addr, text.clone(), true));
                            } else if (response.has_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                || response.lost_focus()
                            {
                                byte_commit = Some((addr, text.clone(), false));
                            }
                            continue;
                        }
                        let response = ui.add(
                            Button::new(text)
                                .selected(selected)
                                .frame(selected)
                                .min_size(egui::vec2(24.0, 24.0)),
                        );
                        if response.clicked() {
                            if ui.input(|i| i.modifiers.shift) {
                                self.selection_end = addr;
                            } else {
                                self.anchor = addr;
                                self.selection_end = addr;
                            }
                            self.selected = addr;
                            self.jump = format!("0x{addr:016X}");
                        }
                        if response.double_clicked() {
                            self.inline_edit = Some((addr, String::new()));
                            ui.memory_mut(|m| m.request_focus(egui::Id::new(("byte_edit", addr))));
                        }
                        response.context_menu(|ui| {
                            if ui.button("Copy address").clicked() {
                                ui.ctx().copy_text(format!("0x{addr:016X}"));
                                ui.close();
                            }
                            if ui.button("Copy selected bytes").clicked() {
                                let (a, n) = self.selection();
                                match app.machine.read_memory(a, n) {
                                    Ok(bytes) => ui.ctx().copy_text(hex(&bytes)),
                                    Err(e) => app.report::<(), _>(Err(e)),
                                }
                                ui.close();
                            }
                            if ui.button("Copy pointer value").clicked() {
                                match app.machine.read_memory(addr, 8) {
                                    Ok(bytes) => {
                                        ui.ctx().copy_text(format!("0x{:016X}", unsigned(&bytes)))
                                    }
                                    Err(e) => app.report::<(), _>(Err(e)),
                                }
                                ui.close();
                            }
                            if ui.button("Bookmark address").clicked() {
                                if !self.bookmarks.contains(&addr) {
                                    self.bookmarks.push(addr);
                                }
                                ui.close();
                            }
                            if !self.bookmarks.is_empty() {
                                ui.menu_button("Bookmarks", |ui| {
                                    for a in self.bookmarks.clone() {
                                        if ui.button(format!("{a:016X}")).clicked() {
                                            self.navigate(a, app);
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            if ui.button("Inspect byte").clicked() {
                                self.selected = addr;
                                self.anchor = addr;
                                self.selection_end = addr;
                                self.pending_tools.push(Tab::Inspector);
                                ui.close();
                            }
                            if ui.button("Follow as 8-byte pointer").clicked() {
                                self.selected = addr;
                                self.follow(app);
                                ui.close();
                            }
                            if ui.button("Watch as u64").clicked() {
                                self.selected = addr;
                                self.add_watch(ValueType::U64);
                                ui.close();
                            }
                        });
                    }
                    ui.label(
                        RichText::new(
                            bytes
                                .iter()
                                .map(|b| {
                                    if b.is_ascii_graphic() || *b == b' ' {
                                        *b as char
                                    } else {
                                        '.'
                                    }
                                })
                                .collect::<String>(),
                        )
                        .monospace(),
                    );
                });
            }
        });
        if cancel_edit {
            self.inline_edit = None;
        } else if let Some((address, text, advance)) = byte_commit {
            if text.trim().is_empty() {
                self.inline_edit = None;
                return;
            }
            let r = parse_hex(&text).and_then(|bytes| {
                if bytes.is_empty() {
                    Err("enter pairs of hex digits".into())
                } else {
                    app.machine.write_memory(address, &bytes)
                }
            });
            if r.is_ok() {
                let count = parse_hex(&text).expect("already validated").len() as u64;
                self.anchor = address;
                self.selection_end = address + count - 1;
                self.inline_edit = None;
                if advance
                    && let Some(next) = address.checked_add(count).filter(|next| *next < base + len)
                {
                    self.selected = next;
                    self.anchor = next;
                    self.selection_end = next;
                    self.jump = format!("0x{next:016X}");
                    self.inline_edit = Some((next, String::new()));
                    if (next - base) / 16 != (address - base) / 16 {
                        self.scroll_to = Some(((next - base) / 16) as f32 * 32.0);
                    }
                    ui.memory_mut(|m| m.request_focus(egui::Id::new(("byte_edit", next))));
                }
            } else {
                ui.memory_mut(|m| m.request_focus(egui::Id::new(("byte_edit", address))));
            }
            app.report(r);
        }
    }
    fn first_scan(&mut self, app: &mut CookieApp) -> Result<(), String> {
        let result = match self.scan_format {
            ScanFormat::Number => {
                let exact = if self.scan_unknown {
                    None
                } else {
                    Some(parse_u64(&self.scan_value)?)
                };
                self.scanner
                    .first(&app.machine, self.scan_width, exact, self.scan_aligned)
            }
            kind => self
                .scanner
                .first_bytes(&app.machine, &kind.bytes(&self.scan_value)?),
        };
        if result.is_ok() {
            self.result_format = self.scan_format;
            self.result_page = 0;
        }
        result
    }
    fn narrow_scan(&mut self, app: &mut CookieApp) -> Result<(), String> {
        if self.scan_format != self.result_format {
            return Err("Input format changed. Start a new first scan.".into());
        }
        if self.scanner.byte_pattern {
            let target = if self.scan_filter.needs_value() {
                Some(self.scan_format.bytes(&self.scan_value)?)
            } else {
                None
            };
            self.scanner
                .filter_bytes(&app.machine, self.scan_filter, target.as_deref())
        } else {
            let target = if self.scan_filter.needs_value() {
                Some(parse_u64(&self.scan_value)?)
            } else {
                None
            };
            self.scanner.filter(&app.machine, self.scan_filter, target)
        }
    }
    fn scanner_ui(&mut self, ui: &mut egui::Ui, app: &mut CookieApp) {
        ui.horizontal(|ui| {
            ui.label("Find");
            egui::ComboBox::from_id_salt("scan_format")
                .selected_text(self.scan_format.label())
                .show_ui(ui, |ui| {
                    for kind in [ScanFormat::Number, ScanFormat::Text, ScanFormat::HexBytes] {
                        ui.selectable_value(&mut self.scan_format, kind, kind.label());
                    }
                });
        });
        let hint = match self.scan_format {
            ScanFormat::Number => "127, 0x7F, or 0b1111111",
            ScanFormat::Text => "Crumb & Co.",
            ScanFormat::HexBytes => "20 31 01 00",
        };
        ui.add(
            egui::TextEdit::singleline(&mut self.scan_value)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        );
        ui.small(match self.scan_format {
            ScanFormat::Number => "Decimal, 0x hex, and 0b binary describe the same number.",
            ScanFormat::Text => "Search the exact UTF-8 bytes of a name or label.",
            ScanFormat::HexBytes => "Search bytes in the order written, separated by spaces.",
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("First scan").clicked() {
                let r = self.first_scan(app);
                app.report(r);
            }
            if ui
                .add_enabled(self.scanner.initialized, Button::new("Narrow results"))
                .clicked()
            {
                let r = self.narrow_scan(app);
                app.report(r);
            }
            if ui.button("Reset").clicked() {
                self.scanner = Scanner::default();
            }
        });
        ui.collapsing("Scan options", |ui| {
            if self.scan_format == ScanFormat::Number {
                egui::ComboBox::from_id_salt("scan_width")
                    .selected_text(format!("{}-bit unsigned", self.scan_width * 8))
                    .show_ui(ui, |ui| {
                        for width in [1, 2, 4, 8] {
                            ui.selectable_value(
                                &mut self.scan_width,
                                width,
                                format!("{}-bit unsigned", width * 8),
                            );
                        }
                    });
                ui.checkbox(&mut self.scan_aligned, "Aligned addresses");
                ui.checkbox(&mut self.scan_unknown, "Unknown initial value");
            }
            egui::ComboBox::from_id_salt("scan_filter")
                .selected_text(self.scan_filter.label())
                .show_ui(ui, |ui| {
                    for f in ScanFilter::ALL {
                        if self.scan_format == ScanFormat::Number
                            || matches!(
                                f,
                                ScanFilter::Exact
                                    | ScanFilter::Equal
                                    | ScanFilter::NotEqual
                                    | ScanFilter::Changed
                                    | ScanFilter::Unchanged
                            )
                        {
                            ui.selectable_value(&mut self.scan_filter, f, f.label());
                        }
                    }
                });
        });
        if !self.scanner.initialized {
            ui.label(
                RichText::new("Enter a value you can see in the game, then run First scan.")
                    .color(app.theme().muted),
            );
            return;
        }
        ui.small("Change the game value, enter its new value above, then narrow the results.");
        ui.label(
            RichText::new(format!(
                "{} matches",
                number(self.scanner.results.len() as u64)
            ))
            .strong(),
        );
        if self.scanner.results.is_empty() {
            ui.label("No matches. Check the input and start a new scan.");
            return;
        }
        ScrollArea::both()
            .id_salt("scan_table")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("scan_table_rows")
                    .num_columns(3)
                    .striped(true)
                    .spacing(egui::vec2(16.0, 8.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Guest address").strong());
                        ui.label(RichText::new("Current value").strong());
                        ui.label("");
                        ui.end_row();
                        // Keep the table bounded while scans retain every match. Paging is explicit.
                        let pages = self.scanner.results.len().div_ceil(100);
                        self.result_page = self.result_page.min(pages.saturating_sub(1));
                        let start = self.result_page * 100;
                        let end = (start + 100).min(self.scanner.results.len());
                        for i in start..end {
                            let a = self.scanner.results[i].address;
                            let current = app
                                .machine
                                .read_memory(a, self.scanner.width as u64)
                                .map(|b| self.result_format.display(&b))
                                .unwrap_or_else(|_| "Unreadable".into());
                            if ui
                                .selectable_label(
                                    self.selected == a,
                                    RichText::new(format!("0x{a:016X}")).monospace(),
                                )
                                .clicked()
                            {
                                self.navigate(a, app);
                            }
                            ui.add(egui::Label::new(current.clone()).truncate())
                                .on_hover_text(current);
                            if ui.button("Inspect").clicked() {
                                self.navigate(a, app);
                            }
                            ui.end_row();
                        }
                    });
            });
        let pages = self.scanner.results.len().div_ceil(100);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.result_page > 0, Button::new("Previous"))
                .clicked()
            {
                self.result_page -= 1;
            }
            ui.label(format!("Page {} of {}", self.result_page + 1, pages));
            if ui
                .add_enabled(self.result_page + 1 < pages, Button::new("Next"))
                .clicked()
            {
                self.result_page += 1;
            }
        });
    }
    fn inspector_ui(&mut self, ui: &mut egui::Ui, app: &mut CookieApp) {
        let allocation = app
            .machine
            .allocations()
            .iter()
            .rev()
            .find(|a| a.live && a.contains(self.selected));
        egui::Grid::new("inspector_location")
            .num_columns(2)
            .spacing(egui::vec2(16.0, 6.0))
            .show(ui, |ui| {
                ui.label("Address");
                ui.monospace(format!("0x{:016X}", self.selected));
                ui.end_row();
                ui.label("Allocation");
                if let Some(a) = allocation {
                    ui.label(format!(
                        "#{} · offset +0x{:X}",
                        a.id,
                        self.selected - a.base
                    ));
                } else {
                    ui.label(pointer_label(self.selected, app.machine.allocations()));
                }
                ui.end_row();
                if let Some(a) = allocation {
                    ui.label("Size");
                    ui.label(format!(
                        "{} bytes · generation {}",
                        number(a.size),
                        a.generation
                    ));
                    ui.end_row();
                    ui.label("Range")
                        .on_hover_text("The ending address is exclusive");
                    ui.vertical(|ui| {
                        ui.monospace(format!("0x{:016X}", a.base));
                        ui.monospace(format!("to 0x{:016X}", a.end()));
                    });
                    ui.end_row();
                    if self.symbols {
                        ui.label("Object");
                        ui.label(&a.semantic_type);
                        ui.end_row();
                    }
                }
            });
        ui.checkbox(&mut self.symbols, "Show object names").on_hover_text("Reveal simulator object names such as PlayerState or Generator[] for the selected allocation.");
        if ui.button("Follow 8-byte pointer").clicked() {
            self.follow(app);
        }
        ui.add_space(8.0);
        ui.separator();
        egui::Grid::new("typed_interpretations")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for kind in ValueType::ALL {
                    ui.label(kind.label());
                    match app.machine.read_memory(self.selected, kind.width() as u64) {
                        Ok(b) => {
                            ui.label(kind.interpret(&b).unwrap_or_else(|e| e));
                        }
                        Err(_) => {
                            ui.label("Unreadable");
                        }
                    }
                    ui.end_row();
                }
            });
        ui.add_space(16.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.watch_name).hint_text("Watch label (optional)"),
        );
        if ui.button("Add u64 watch").clicked() {
            self.add_watch(ValueType::U64);
        }
    }
    fn watch_ui(&mut self, ui: &mut egui::Ui, app: &mut CookieApp) {
        if self.watches.is_empty() {
            ui.label("No watches. Select a byte in Memory, open Inspector, then add a watch.");
        }
        let mut remove = None;
        ScrollArea::both().id_salt("watches").show(ui, |ui| {
            for (i, w) in self.watches.clone().iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        remove = Some(i);
                    }
                    if ui
                        .selectable_label(
                            self.selected == w.address,
                            RichText::new(format!("{:016X}", w.address)).monospace(),
                        )
                        .clicked()
                    {
                        self.navigate(w.address, app);
                    }
                });
                ui.label(format!(
                    "{} · {} · {}",
                    w.name,
                    w.kind.label(),
                    w.value(&app.machine).unwrap_or_else(|e| e)
                ));
                ui.add_space(8.0);
            }
        });
        if let Some(i) = remove {
            self.watches.remove(i);
        }
    }
    fn history_ui(&mut self, ui: &mut egui::Ui, app: &mut CookieApp) {
        ui.label("Recent writes · last 512 operations · newest first");
        if app.machine.memory.events().is_empty() {
            ui.label("No writes yet. Play the game or edit guest memory.");
        }
        let events = app
            .machine
            .memory
            .events()
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>();
        ScrollArea::both().id_salt("write_history").show(ui,|ui|{
            for (i,e) in events.iter().enumerate(){ui.push_id(i,|ui|{
                let title=format!("tick {} · {:?} · {} · {} bytes{}",e.tick,e.source,e.operation,e.new.len(),if e.overflow>0{format!(" · ALLOCATION OVERFLOW {} bytes",e.overflow)}else if e.semantic_overflow>0{format!(" · FIELD OVERFLOW {} bytes",e.semantic_overflow)}else{String::new()});
                egui::CollapsingHeader::new(RichText::new(title).color(if e.overflow>0||e.semantic_overflow>0{app.theme().danger}else{app.theme().text})).show(ui,|ui|{
                    if ui.button(format!("Jump to 0x{:016X}",e.address)).clicked(){self.navigate(e.address,app);}
                    if let Some(boundary)=e.semantic_boundary{ui.monospace(format!("Intended field ends at {boundary:016X} · {} bytes beyond it",e.semantic_overflow));}
                    ui.label(format!("Origin allocation: {:?} · affected: {:?}",e.origin,e.affected));
                    if let Some(id)=e.origin && let Some(a)=app.machine.allocations().iter().find(|a|a.id==id){ui.monospace(format!("Intended range {:016X} .. {:016X}",a.base,a.end()));}
                    ui.monospace(format!("Old: {}",hex(&e.old[..e.old.len().min(128)])));ui.monospace(format!("New: {}",hex(&e.new[..e.new.len().min(128)])));
                    if e.new.len()>128{ui.small("Byte preview shows the first 128 bytes. Inspect the full range in Memory.");}
                });
            });}
        });
    }
    fn diff_ui(&mut self, ui: &mut egui::Ui, app: &mut CookieApp) {
        ui.horizontal(|ui| {
            if ui.button("Take snapshot").clicked() {
                self.snapshot = Some(app.machine.memory.snapshot());
                self.diffs.clear();
            }
            if ui
                .add_enabled(self.snapshot.is_some(), Button::new("Compare"))
                .clicked()
            {
                self.diffs = diff(self.snapshot.as_ref().unwrap(), &app.machine);
            }
        });
        if self.snapshot.is_none() {
            ui.label("Take a snapshot, play the game, then compare. Memory highlights changed bytes while a snapshot exists.");
            return;
        }
        ui.label(format!(
            "{} changed bytes at last comparison",
            number(self.diffs.len() as u64)
        ));
        ScrollArea::both().id_salt("diff_rows").show_rows(
            ui,
            32.0,
            self.diffs.len(),
            |ui, range| {
                for i in range {
                    let d = &self.diffs[i];
                    let a = d.address;
                    let label = format!("{a:016X}    {:02X} → {:02X}", d.before, d.after);
                    if ui
                        .selectable_label(self.selected == a, RichText::new(label).monospace())
                        .clicked()
                    {
                        self.navigate(a, app);
                    }
                }
            },
        );
    }
}
struct Viewer<'a> {
    workspace: &'a mut DebugWorkspace,
    app: &'a mut CookieApp,
}
impl TabViewer for Viewer<'_> {
    type Tab = Tab;
    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(tab.title())
    }
    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.title().into()
    }
    fn is_closeable(&self, _: &Tab) -> bool {
        true
    }
    fn scroll_bars(&self, _: &Tab) -> [bool; 2] {
        [false, false]
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        egui::Frame::NONE
            .inner_margin(8.0)
            .show(ui, |ui| match tab {
                Tab::Memory => self.workspace.memory_ui(ui, self.app),
                Tab::Scanner => self.workspace.scanner_ui(ui, self.app),
                Tab::Inspector => {
                    ScrollArea::vertical().id_salt("inspector").show(ui, |ui| {
                        self.workspace.inspector_ui(ui, self.app);
                    });
                }
                Tab::Watch => self.workspace.watch_ui(ui, self.app),
                Tab::History => self.workspace.history_ui(ui, self.app),
                Tab::Diff => self.workspace.diff_ui(ui, self.app),
            });
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, app: &mut CookieApp, events: Vec<egui::Event>) {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 850.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show(ui),
        )
        .drop_without_applying_deltas();
    }
    #[test]
    fn starts_without_tools_and_opens_only_floating_tools() {
        let ctx = egui::Context::default();
        let mut app = CookieApp::headless(cookie_game::Machine::new(1).unwrap(), true);
        frame(&ctx, &mut app, vec![]);
        let workspace = app.debugger.as_mut().unwrap();
        assert_eq!(workspace.dock.iter_all_tabs().count(), 0);
        workspace.open_tool(Tab::Scanner, &ctx);
        assert!(workspace.dock.main_surface().is_empty());
        assert_eq!(workspace.dock.iter_all_tabs().count(), 1);
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        assert_eq!(
            app.debugger.as_ref().unwrap().dock.iter_all_tabs().count(),
            1
        );
    }
    #[test]
    fn inline_hex_edit_writes_guest_bytes() {
        let ctx = egui::Context::default();
        let mut app = CookieApp::headless(cookie_game::Machine::new(1).unwrap(), true);
        app.theme().apply(&ctx, false);
        frame(&ctx, &mut app, vec![]);
        let address = app.machine.address(cookie_game::field::COOKIES).unwrap();
        let mut workspace = app.debugger.take().unwrap();
        workspace.navigate(address, &mut app);
        workspace.open_tool(Tab::Memory, &ctx);
        workspace.inline_edit = Some((address, String::new()));
        app.debugger = Some(workspace);
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new(("byte_edit", address))));
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::Text("E8 03 00 00 00 00 00 00".into())],
        );
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            app.machine.value(cookie_game::field::COOKIES).unwrap(),
            1000
        );
        assert_eq!(
            app.machine.memory.events().back().unwrap().source,
            vm_core::WriteSource::Debugger
        );
    }
    fn begin_edit(ctx: &egui::Context, app: &mut CookieApp, address: Address) {
        let mut workspace = app.debugger.take().unwrap();
        workspace.navigate(address, app);
        workspace.open_tool(Tab::Memory, ctx);
        workspace.inline_edit = Some((address, String::new()));
        app.debugger = Some(workspace);
        frame(ctx, app, vec![]);
        frame(ctx, app, vec![]);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new(("byte_edit", address))));
    }
    #[test]
    fn hex_pairs_advance_across_rows_and_stop_at_heap_end() {
        let ctx = egui::Context::default();
        let mut app = CookieApp::headless(cookie_game::Machine::new(1).unwrap(), true);
        app.theme().apply(&ctx, false);
        frame(&ctx, &mut app, vec![]);
        let address = Architecture::HEAP + 0x100F;
        begin_edit(&ctx, &mut app, address);
        frame(&ctx, &mut app, vec![egui::Event::Text("E".into())]);
        assert_eq!(app.machine.read_memory(address, 1).unwrap(), vec![0]);
        frame(&ctx, &mut app, vec![egui::Event::Text("8".into())]);
        assert_eq!(app.debugger.as_ref().unwrap().selected, address + 1);
        frame(&ctx, &mut app, vec![egui::Event::Text("0".into())]);
        frame(&ctx, &mut app, vec![egui::Event::Text("3".into())]);
        assert_eq!(app.machine.read_memory(address, 2).unwrap(), vec![0xE8, 3]);
        assert_eq!(app.debugger.as_ref().unwrap().selected, address + 2);
        let end = Architecture::HEAP + Architecture::HEAP_SIZE - 1;
        begin_edit(&ctx, &mut app, end);
        frame(&ctx, &mut app, vec![egui::Event::Text("AA BB".into())]);
        assert!(app.error);
        assert_eq!(app.machine.read_memory(end, 1).unwrap(), vec![0]);
        begin_edit(&ctx, &mut app, end);
        frame(&ctx, &mut app, vec![egui::Event::Text("FF".into())]);
        assert_eq!(app.machine.read_memory(end, 1).unwrap(), vec![255]);
        assert!(app.debugger.as_ref().unwrap().inline_edit.is_none());
        assert!(!app.error);
    }
    #[test]
    fn memory_stays_in_heap_and_opens_a_read_only_inspector() {
        let ctx = egui::Context::default();
        let mut app = CookieApp::headless(cookie_game::Machine::new(1).unwrap(), true);
        app.theme().apply(&ctx, false);
        frame(&ctx, &mut app, vec![]);
        let mut workspace = app.debugger.take().unwrap();
        workspace.open_tool(Tab::Memory, &ctx);
        workspace.navigate(Architecture::GLOBALS, &mut app);
        assert_eq!(workspace.selected, Architecture::HEAP);
        assert!(app.error);
        app.debugger = Some(workspace);
        crate::tests::click_label(&ctx, &mut app, "Inspector");
        assert!(
            app.debugger
                .as_ref()
                .unwrap()
                .dock
                .find_tab(&Tab::Inspector)
                .is_some()
        );
        assert_eq!(
            app.debugger.as_ref().unwrap().dock.iter_all_tabs().count(),
            2
        );
        let output = crate::tests::render(&ctx, &mut app, vec![]);
        let mut painted = vec![];
        for s in &output.shapes {
            crate::tests::texts(&s.shape, &mut painted);
        }
        for removed in ["Write typed value", "More memory actions", "Edit raw bytes"] {
            assert!(!painted.iter().any(|(text, _, _, _)| text == removed));
        }
        output.drop_without_applying_deltas();
    }
    #[test]
    fn scanner_ui_accepts_numbers_text_and_bytes() {
        let mut app = CookieApp::headless(cookie_game::Machine::new(1).unwrap(), true);
        app.machine.click().unwrap();
        let mut w = app.debugger.take().unwrap();
        for input in ["1", "0x1", "0b1"] {
            w.scan_value = input.into();
            w.first_scan(&mut app).unwrap();
            assert!(w.scanner.results.iter().any(|r|r.address==app.machine.address(cookie_game::field::COOKIES).unwrap()));
        }
        w.scan_format = ScanFormat::Text;
        w.scan_value = "Crumb & Co.".into();
        w.first_scan(&mut app).unwrap();
        assert_eq!(w.scanner.results.len(), 1);
        w.scan_format = ScanFormat::HexBytes;
        w.scan_value = "43 72 75 6D 62".into();
        w.first_scan(&mut app).unwrap();
        assert_eq!(w.scanner.results.len(), 1);
    }
}
