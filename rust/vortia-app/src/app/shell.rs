//! The window around a case, built from the engineering suite's shared parts
//! (`nodalia_theme::engineering`, specified in nodalia-design's `docs/suites/engineering.md`):
//! the title bar, a ribbon of workflow steps, the Browser with the case's tree, Properties,
//! the status bar with the run and its residuals, Home, and the command search.

use eframe::egui::{
    self, Color32, Context, Frame, Id, Key, Margin, Pos2, RichText, Sense, Vec2, ViewportCommand,
};
use nodalia_theme::engineering::{
    self as eng, Document, Look, TitleAction, TitleBar, TitleLabels, TreeHit, TreeRow, layout,
};
use nodalia_theme::office::{self, Entry, Nav};
use nodalia_theme::{Mode, Palette, for_egui, space, window_controls};

use super::conditions_tab;
use super::{
    CaseView, Dialog, Screen, Tab, Tone, VortiaApp, convergence_card, file_tab, grouped, mesh_tab,
    muted, own_controls, palette, results_tab, scientific, settings_tab, status_label, tone_colors,
};
use crate::numerics::File;
use crate::summary::Summary;
use crate::viewer::Source;

/// Something the user asked for, from the File menu, the ribbon or the command search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cmd {
    Open,
    Run,
    Stop,
    CloseCase,
    Quit,
    Go(Tab),
    Show(Dialog),
}

/// What the command search offers, in the order it lists them.
const COMMANDS: &[(&str, Cmd)] = &[
    ("Run the case", Cmd::Run),
    ("Stop the run", Cmd::Stop),
    ("Open a case", Cmd::Open),
    ("Close the case", Cmd::CloseCase),
    ("Go to the run", Cmd::Go(Tab::Run)),
    ("Go to the mesh", Cmd::Go(Tab::Mesh)),
    ("Go to the results", Cmd::Go(Tab::Results)),
    ("Go to the boundary conditions", Cmd::Go(Tab::Conditions)),
    ("Go to the run settings", Cmd::Go(Tab::Settings)),
    ("Go to the solver numbers", Cmd::Go(Tab::Numerics)),
    ("Go to the schemes", Cmd::Go(Tab::Schemes)),
    ("Preferences", Cmd::Show(Dialog::Preferences)),
    ("Keyboard shortcuts", Cmd::Show(Dialog::Shortcuts)),
    ("About Vortia", Cmd::Show(Dialog::About)),
];

/// A workflow step of the ribbon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Case,
    Mesh,
    Setup,
    Results,
}

impl Step {
    const ALL: [Step; 4] = [Step::Case, Step::Mesh, Step::Setup, Step::Results];

    fn name(self) -> &'static str {
        match self {
            Step::Case => "Case",
            Step::Mesh => "Mesh",
            Step::Setup => "Setup",
            Step::Results => "Results",
        }
    }

    /// The view a step opens on.
    fn first(self) -> Tab {
        match self {
            Step::Case => Tab::Run,
            Step::Mesh => Tab::Mesh,
            Step::Setup => Tab::Conditions,
            Step::Results => Tab::Results,
        }
    }

    fn of(tab: Tab) -> Step {
        match tab {
            Tab::Run => Step::Case,
            Tab::Mesh => Step::Mesh,
            Tab::Results => Step::Results,
            Tab::Conditions | Tab::Numerics | Tab::Schemes | Tab::Settings => Step::Setup,
        }
    }
}

/// What is picked in the Browser besides a view.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(super) enum Selection {
    #[default]
    Nothing,
    Patch(String),
}

/// The name a view carries over the main island.
fn view_title(tab: Tab) -> &'static str {
    match tab {
        Tab::Run => "Run",
        Tab::Mesh => "Mesh",
        Tab::Results => "Results",
        Tab::Conditions => "Boundary conditions",
        Tab::Numerics => "Solver numbers",
        Tab::Schemes => "Schemes",
        Tab::Settings => "Run settings",
    }
}

fn gap() -> i8 {
    layout::GAP as i8
}

fn look_of(ctx: &Context) -> Look {
    Look::new(palette(ctx))
}

impl VortiaApp {
    /// The whole window under the title bar: Home, or the case.
    pub(super) fn shell(&mut self, ui: &mut egui::Ui, ctx: &Context, maximized: bool) {
        let look = look_of(ctx);
        let p = look.palette;
        egui::Panel::top("vortia_title")
            .exact_size(layout::TITLE_BAR)
            .show_separator_line(false)
            .frame(Frame::NONE)
            .show(ui, |ui| self.title_bar(ui, ctx, &look, maximized));
        if self.home || matches!(self.screen, Screen::Welcome) {
            self.home_page(ui, ctx, &look);
        } else {
            let command = self.case_page(ui, ctx, &look, &p);
            if let Some(command) = command {
                self.perform(ctx, command);
            }
        }
        self.search_dialog(ctx, &look);
    }

    /// Does what was asked for.
    pub(super) fn perform(&mut self, ctx: &Context, command: Cmd) {
        match command {
            Cmd::Open => {
                self.home = true;
                self.focus_open = true;
            }
            Cmd::Run => {
                self.start_run(ctx);
                if let Screen::Case(case) = &mut self.screen {
                    case.tab = Tab::Run;
                    self.home = false;
                }
            }
            Cmd::Stop => self.stop_run(),
            Cmd::CloseCase => {
                self.close_case();
                self.home = matches!(self.screen, Screen::Welcome) || self.home;
            }
            Cmd::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            Cmd::Go(tab) => {
                if let Screen::Case(case) = &mut self.screen {
                    case.go(tab);
                    self.home = false;
                }
            }
            Cmd::Show(dialog) => self.dialog = Some(dialog),
        }
    }

    // --- Title bar ---------------------------------------------------------------------------

    fn title_bar(&mut self, ui: &mut egui::Ui, ctx: &Context, look: &Look, maximized: bool) {
        let name = match &self.screen {
            Screen::Case(case) => Some(case.name()),
            Screen::Welcome => None,
        };
        let documents: Vec<Document> = name
            .iter()
            .map(|name| Document {
                name,
                modified: false,
            })
            .collect();
        let controls_width = if own_controls() {
            window_controls::WIDTH + 2.0 * space::SM
        } else {
            0.0
        };
        let icon = self.icon(ctx).id();
        let bar = TitleBar {
            app: "Vortia",
            icon: Some(icon),
            documents: &documents,
            active: (!self.home && !documents.is_empty()).then_some(0),
            labels: TitleLabels {
                home: "Home",
                file: "File",
                save: "Save",
                undo: "Undo",
                redo: "Redo",
                new_document: "Open a case",
                close_document: "Close the case",
                search: "Search commands",
                menu: "Menu",
            },
            quick_access: &[],
            controls_width,
            leading_width: 0.0,
        };
        let out = eng::title_bar(ui, look, &bar);
        if own_controls() {
            if out.drag.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if out.drag.double_clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            }
            let focused = ctx.input(|i| i.focused);
            let mode = if ctx.global_style().visuals.dark_mode {
                Mode::Dark
            } else {
                Mode::Light
            };
            let labels = window_controls::Labels {
                minimize: "Minimize",
                maximize: "Maximize",
                restore: "Restore",
                close: "Close",
            };
            let action = window_controls::show(
                ui,
                out.controls,
                &look.palette,
                mode,
                focused,
                maximized,
                labels,
            );
            if let Some(action) = action {
                ctx.send_viewport_cmd(match action {
                    window_controls::Action::Minimize => ViewportCommand::Minimized(true),
                    window_controls::Action::Maximize => ViewportCommand::Maximized(true),
                    window_controls::Action::Restore => ViewportCommand::Maximized(false),
                    window_controls::Action::Close => ViewportCommand::Close,
                });
            }
        }
        let bottom = ui.min_rect().bottom();
        let at = |x: f32| Pos2::new(x, bottom);
        let pointer_x = ctx.pointer_interact_pos().map(|p| p.x);
        match out.action {
            Some(TitleAction::Home) => self.home = true,
            Some(TitleAction::File) => {
                let x = pointer_x.map_or(0.0, |x| x - 20.0);
                ctx.data_mut(|d| d.insert_temp(file_menu().with("at"), at(x)));
                egui::Popup::toggle_id(ctx, file_menu());
            }
            Some(TitleAction::Menu) => {
                let x = pointer_x.unwrap_or(0.0);
                ctx.data_mut(|d| d.insert_temp(app_menu().with("at"), at(x)));
                egui::Popup::toggle_id(ctx, app_menu());
            }
            // Vortia's editors save themselves (each has its own Save), and there is no
            // document-wide history.
            Some(TitleAction::Save | TitleAction::Undo | TitleAction::Redo) => {}
            Some(TitleAction::Document(_)) => self.home = false,
            Some(TitleAction::CloseDocument(_)) => self.perform(ctx, Cmd::CloseCase),
            Some(TitleAction::NewDocument) => self.perform(ctx, Cmd::Open),
            Some(TitleAction::Search) => self.search = Some(String::new()),
            None => {}
        }
        self.title_menus(ui, ctx);
    }

    /// The File menu and the app's menu, opened from the title bar.
    fn title_menus(&mut self, ui: &egui::Ui, ctx: &Context) {
        let mut chosen = None;
        let at = |id: Id| {
            ctx.data(|d| d.get_temp::<Pos2>(id.with("at")))
                .unwrap_or_default()
        };
        let case_open = matches!(self.screen, Screen::Case(_));
        let running = matches!(&self.screen, Screen::Case(case) if case.running());
        egui::Popup::new(file_menu(), ctx.clone(), at(file_menu()), ui.layer_id())
            .kind(egui::PopupKind::Menu)
            .open_memory(None)
            .show(|ui| {
                ui.set_min_width(190.0);
                let mut item = |ui: &mut egui::Ui, label: &str, enabled: bool, command: Cmd| {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        chosen = Some(command);
                        ui.close();
                    }
                };
                item(ui, "Open a case", true, Cmd::Open);
                item(ui, "Run", case_open && !running, Cmd::Run);
                item(ui, "Stop", running, Cmd::Stop);
                item(ui, "Close the case", case_open && !running, Cmd::CloseCase);
                ui.separator();
                item(ui, "Quit", true, Cmd::Quit);
            });
        egui::Popup::new(app_menu(), ctx.clone(), at(app_menu()), ui.layer_id())
            .kind(egui::PopupKind::Menu)
            .open_memory(None)
            .show(|ui| {
                ui.set_min_width(190.0);
                for (label, dialog) in [
                    ("Preferences", Dialog::Preferences),
                    ("Keyboard shortcuts", Dialog::Shortcuts),
                    ("About Vortia", Dialog::About),
                ] {
                    if ui.button(label).clicked() {
                        chosen = Some(Cmd::Show(dialog));
                        ui.close();
                    }
                }
            });
        if let Some(command) = chosen {
            self.perform(ctx, command);
        }
    }

    // --- Command search ----------------------------------------------------------------------

    fn search_dialog(&mut self, ctx: &Context, look: &Look) {
        let Some(query) = self.search.as_mut() else {
            return;
        };
        let p = &look.palette;
        let mut chosen = None;
        let mut close = false;
        egui::Modal::new(Id::NULL.with("command-search")).show(ctx, |ui| {
            ui.set_width(nodalia_theme::size::DIALOG);
            let field = ui.add(
                egui::TextEdit::singleline(query)
                    .hint_text("Search commands")
                    .desired_width(f32::INFINITY),
            );
            field.request_focus();
            ui.add_space(space::SM);
            let needle = query.trim().to_lowercase();
            let hits: Vec<&(&str, Cmd)> = COMMANDS
                .iter()
                .filter(|(label, _)| needle.is_empty() || label.to_lowercase().contains(&needle))
                .collect();
            for (i, (label, command)) in hits.iter().enumerate() {
                let row = ui.add(
                    egui::Button::new(RichText::new(*label).color(for_egui::color(p.text_primary)))
                        .frame(false)
                        .min_size(Vec2::new(ui.available_width(), layout::TREE_ROW)),
                );
                if row.clicked() || (i == 0 && ctx.input(|i| i.key_pressed(Key::Enter))) {
                    chosen = Some(*command);
                }
            }
            if hits.is_empty() {
                ui.label(RichText::new("Nothing matches.").color(muted(p)));
            }
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
        });
        if chosen.is_some() || close {
            self.search = None;
        }
        if let Some(command) = chosen {
            self.perform(ctx, command);
        }
    }

    // --- Home --------------------------------------------------------------------------------

    fn home_page(&mut self, ui: &mut egui::Ui, ctx: &Context, look: &Look) {
        let home = eng::home_look(look);
        let p = &look.palette;
        let has_case = matches!(self.screen, Screen::Case(_));
        let nav = Nav {
            back: if has_case { "Back to the case" } else { "" },
            top: vec![
                Entry::item("home", "Home", "home"),
                Entry::item("open", "Open", "folder"),
            ],
            bottom: vec![Entry::item("preferences", "Preferences", "settings")],
            selected: Some("home"),
        };
        let icon = self.icon(ctx).id();
        let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
        let focus = std::mem::take(&mut self.focus_open);
        let recent: Vec<(String, String)> = self
            .prefs
            .recent
            .iter()
            .map(|dir| {
                let name = dir.file_name().map_or_else(
                    || dir.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                let folder = dir
                    .parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default();
                (name, folder)
            })
            .collect();
        let mut open = false;
        let mut picked = None;
        let nav_pick = office::file_view(ui, &home, &nav, |ui| {
            eng::home_header(ui, look, Some(icon), "Vortia", &version);
            office::section(ui, &home, "Open a case", |ui| {
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 120.0).clamp(160.0, 420.0);
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.path_input)
                            .hint_text("Folder of a case")
                            .desired_width(width),
                    );
                    if focus {
                        field.request_focus();
                    }
                    open |= field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    open |= super::accent_button(ui, p, "Open case").clicked();
                });
                if let Some(notice) = &self.notice {
                    ui.add_space(space::SM);
                    ui.colored_label(for_egui::color(p.status_danger_fg), notice);
                }
            });
            office::section(ui, &home, "Recent", |ui| {
                let docs: Vec<eng::RecentDocument> = recent
                    .iter()
                    .map(|(name, folder)| eng::RecentDocument {
                        name,
                        folder,
                        when: "",
                        icon: "nozzle",
                        thumbnail: None,
                    })
                    .collect();
                picked = eng::recent_documents(
                    ui,
                    look,
                    &docs,
                    "Nothing here yet. Open a case folder to get started.",
                );
            });
        });
        match nav_pick.as_deref() {
            Some(office::BACK) => self.home = false,
            Some("open") => self.focus_open = true,
            Some("preferences") => self.dialog = Some(Dialog::Preferences),
            _ => {}
        }
        if open {
            let dir = std::path::PathBuf::from(self.path_input.trim());
            match self.open_case(&dir) {
                Ok(()) => self.path_input.clear(),
                Err(message) => self.notice = Some(message),
            }
        }
        if let Some(dir) = picked.and_then(|i| self.prefs.recent.get(i).cloned())
            && let Err(message) = self.open_case(&dir)
        {
            self.notice = Some(message);
        }
    }

    // --- The case ----------------------------------------------------------------------------

    fn case_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        look: &Look,
        p: &Palette,
    ) -> Option<Cmd> {
        let g = gap();
        let island = || eng::island(look);
        let mut command = None;
        let Screen::Case(case) = &mut self.screen else {
            return None;
        };

        egui::Panel::bottom("vortia_status")
            .exact_size(layout::STATUS_BAR)
            .show_separator_line(false)
            .frame(Frame::NONE)
            .show(ui, |ui| status_bar(ui, look, case));

        egui::Panel::top("vortia_ribbon")
            .exact_size(layout::RIBBON + 12.0 + layout::GAP)
            .show_separator_line(false)
            .frame(island().outer_margin(Margin {
                left: g,
                right: g,
                top: 0,
                bottom: g,
            }))
            .show(ui, |ui| command = ribbon(ui, look, case));

        egui::Panel::left("vortia_browser")
            .exact_size(layout::BROWSER_WIDTH + 16.0 + layout::GAP)
            .resizable(false)
            .show_separator_line(false)
            .frame(island().outer_margin(Margin {
                left: g,
                right: 0,
                top: 0,
                bottom: g,
            }))
            .show(ui, |ui| browser(ui, look, case));

        egui::Panel::right("vortia_properties")
            .exact_size(layout::PROPERTIES_WIDTH + 16.0 + layout::GAP)
            .resizable(false)
            .show_separator_line(false)
            .frame(island().outer_margin(Margin {
                left: g,
                right: g,
                top: 0,
                bottom: g,
            }))
            .show(ui, |ui| properties(ui, look, case));

        egui::CentralPanel::default()
            .frame(
                island()
                    .inner_margin(Margin::same(12))
                    .outer_margin(Margin {
                        left: g,
                        right: 0,
                        top: 0,
                        bottom: g,
                    }),
            )
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                case_view(ui, ctx, look, p, case);
            });
        command
    }
}

fn file_menu() -> Id {
    Id::NULL.with("vortia_file_menu")
}

fn app_menu() -> Id {
    Id::NULL.with("vortia_app_menu")
}

// --- Ribbon ----------------------------------------------------------------------------------

/// The workflow steps and their tools. Returns what a tool asked the app to do.
fn ribbon(ui: &mut egui::Ui, look: &Look, case: &mut CaseView) -> Option<Cmd> {
    let mut command = None;
    let step = Step::of(case.tab);
    ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
    ui.horizontal(|ui| {
        for candidate in Step::ALL {
            if eng::ribbon_tab(ui, look, candidate.name(), candidate == step).clicked()
                && candidate != step
            {
                case.go(candidate.first());
            }
        }
    });
    let running = case.running();
    let stopping = case.stopping;
    let tab = case.tab;
    let mut go = None;
    ui.horizontal(|ui| {
        let tool = |ui: &mut egui::Ui, icon: &str, text: &str, active: bool, enabled: bool| {
            ui.add_enabled_ui(enabled, |ui| eng::ribbon_tool(ui, look, icon, text, active))
                .inner
                .clicked()
        };
        match step {
            Step::Case => {
                eng::ribbon_group(ui, look, "Run", |ui| {
                    if tool(ui, "run", "Run", running, !running) {
                        command = Some(Cmd::Run);
                    }
                    if tool(ui, "stop", "Stop", false, running && !stopping) {
                        command = Some(Cmd::Stop);
                    }
                });
                eng::ribbon_group(ui, look, "Case", |ui| {
                    if tool(ui, "folder", "Open", false, !running) {
                        command = Some(Cmd::Open);
                    }
                    if tool(ui, "close", "Close", false, !running) {
                        command = Some(Cmd::CloseCase);
                    }
                });
            }
            Step::Mesh => {
                eng::ribbon_group(ui, look, "Mesh", |ui| {
                    if tool(ui, "mesh", "Surface", tab == Tab::Mesh, true) {
                        go = Some(Tab::Mesh);
                    }
                });
            }
            Step::Setup => {
                eng::ribbon_group(ui, look, "Physics", |ui| {
                    if tool(ui, "boundary", "Conditions", tab == Tab::Conditions, true) {
                        go = Some(Tab::Conditions);
                    }
                });
                eng::ribbon_group(ui, look, "Solver", |ui| {
                    for (icon, text, target) in [
                        ("settings", "Settings", Tab::Settings),
                        ("solver", "Numerics", Tab::Numerics),
                        ("parameters", "Schemes", Tab::Schemes),
                    ] {
                        if tool(ui, icon, text, tab == target, true) {
                            go = Some(target);
                        }
                    }
                });
            }
            Step::Results => {
                eng::ribbon_group(ui, look, "Fields", |ui| {
                    if tool(ui, "probe", "Probes", tab == Tab::Results, true) {
                        go = Some(Tab::Results);
                    }
                });
            }
        }
    });
    if let Some(tab) = go {
        case.go(tab);
    }
    command
}

// --- Browser ---------------------------------------------------------------------------------

/// What a Browser row opens.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pick {
    View(Tab),
    Patch(String),
    Field(String),
}

struct Row {
    /// The group this row opens and closes, for a row with children.
    group: Option<&'static str>,
    depth: usize,
    icon: &'static str,
    label: String,
    selected: bool,
    muted: bool,
    pick: Pick,
}

/// The rows of the case's tree: the case, its mesh and patches, the physics and its fields, the
/// solver's files, the run and the results.
fn tree(case: &CaseView) -> Vec<Row> {
    let open = |group: &str| !case.collapsed.contains(group);
    let nothing = case.selection == Selection::Nothing;
    let view = |tab: Tab| case.tab == tab && nothing;
    let mut rows = vec![Row {
        group: Some("case"),
        depth: 0,
        icon: "folder",
        label: case.name(),
        selected: false,
        muted: false,
        pick: Pick::View(Tab::Run),
    }];
    if !open("case") {
        return rows;
    }
    let summary = case.summary.as_ref();
    rows.push(Row {
        group: Some("mesh"),
        depth: 1,
        icon: "mesh",
        label: "Mesh".into(),
        selected: view(Tab::Mesh),
        muted: false,
        pick: Pick::View(Tab::Mesh),
    });
    if open("mesh")
        && let Some(Ok(mesh)) = summary.and_then(|s| s.mesh.as_ref())
    {
        for patch in &mesh.patches {
            rows.push(Row {
                group: None,
                depth: 2,
                icon: "boundary",
                label: patch.name.clone(),
                selected: case.selection == Selection::Patch(patch.name.clone()),
                muted: matches!(patch.kind.as_str(), "empty" | "wedge"),
                pick: Pick::Patch(patch.name.clone()),
            });
        }
    }
    rows.push(Row {
        group: Some("physics"),
        depth: 1,
        icon: "boundary",
        label: "Boundary conditions".into(),
        selected: view(Tab::Conditions) && case.conditions_field.is_none(),
        muted: false,
        pick: Pick::View(Tab::Conditions),
    });
    if open("physics") {
        for field in summary.map(|s| s.fields.as_slice()).unwrap_or_default() {
            rows.push(Row {
                group: None,
                depth: 2,
                icon: "contour",
                label: field.clone(),
                selected: view(Tab::Conditions) && case.conditions_field.as_ref() == Some(field),
                muted: false,
                pick: Pick::Field(field.clone()),
            });
        }
    }
    rows.push(Row {
        group: Some("solver"),
        depth: 1,
        icon: "solver",
        label: "Solver".into(),
        selected: false,
        muted: false,
        pick: Pick::View(Tab::Settings),
    });
    if open("solver") {
        for (icon, label, tab) in [
            ("settings", "Run settings", Tab::Settings),
            ("parameters", "Numerics", Tab::Numerics),
            ("parameters", "Schemes", Tab::Schemes),
        ] {
            rows.push(Row {
                group: None,
                depth: 2,
                icon,
                label: label.into(),
                selected: view(tab),
                muted: false,
                pick: Pick::View(tab),
            });
        }
    }
    rows.push(Row {
        group: None,
        depth: 1,
        icon: "run",
        label: "Run".into(),
        selected: view(Tab::Run),
        muted: false,
        pick: Pick::View(Tab::Run),
    });
    rows.push(Row {
        group: None,
        depth: 1,
        icon: "chart",
        label: "Results".into(),
        selected: view(Tab::Results),
        muted: false,
        pick: Pick::View(Tab::Results),
    });
    rows
}

fn browser(ui: &mut egui::Ui, look: &Look, case: &mut CaseView) {
    eng::panel_header(ui, look, "Browser", |ui| {
        if let Some(Ok(mesh)) = case.summary.as_ref().and_then(|s| s.mesh.as_ref()) {
            ui.label(
                RichText::new(format!("{} cells", grouped(mesh.cells)))
                    .font(eng::caption())
                    .color(for_egui::color(look.palette.text_secondary)),
            );
        }
    });
    let rows = tree(case);
    let mut pick = None;
    let mut toggle = None;
    egui::ScrollArea::vertical()
        .id_salt("vortia-browser")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for row in &rows {
                let expanded = row.group.map(|g| !case.collapsed.contains(g));
                let shown = TreeRow {
                    depth: row.depth,
                    expanded,
                    icon: row.icon,
                    label: &row.label,
                    selected: row.selected,
                    muted: row.muted,
                };
                let (_, hit) = eng::tree_row(ui, look, &shown);
                match hit {
                    TreeHit::Row => pick = Some(row.pick.clone()),
                    TreeHit::Disclosure => toggle = row.group,
                    TreeHit::None => {}
                }
            }
        });
    if let Some(group) = toggle
        && !case.collapsed.remove(group)
    {
        case.collapsed.insert(group);
    }
    match pick {
        Some(Pick::View(tab)) => case.go(tab),
        Some(Pick::Patch(name)) => {
            case.go(Tab::Mesh);
            case.selection = Selection::Patch(name);
        }
        Some(Pick::Field(field)) => {
            case.go(Tab::Conditions);
            if case.conditions_field.as_ref() != Some(&field) {
                case.conditions_field = Some(field);
                case.conditions = None;
                case.conditions_message = None;
            }
        }
        None => {}
    }
}

// --- Properties ------------------------------------------------------------------------------

fn properties(ui: &mut egui::Ui, look: &Look, case: &CaseView) {
    let title = match &case.selection {
        Selection::Patch(_) => "Patch",
        Selection::Nothing => "Case",
    };
    eng::panel_header(ui, look, title, |_| {});
    egui::ScrollArea::vertical()
        .id_salt("vortia-properties")
        .auto_shrink([false, false])
        .show(ui, |ui| match (&case.selection, case.summary.as_ref()) {
            (_, None) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Reading the case…").color(muted(&look.palette)));
                });
            }
            (Selection::Patch(name), Some(summary)) => patch_properties(ui, look, summary, name),
            (Selection::Nothing, Some(summary)) => case_properties(ui, look, summary),
        });
}

fn case_properties(ui: &mut egui::Ui, look: &Look, s: &Summary) {
    let p = &look.palette;
    eng::properties(ui, "case", |ui| {
        let row = |ui: &mut egui::Ui, name: &str, text: String| {
            eng::property(ui, look, name, |ui| eng::value(ui, look, &text));
        };
        if let Some(app) = &s.application {
            row(ui, "Solver", app.clone());
        }
        if let (Some(start), Some(end)) = (s.start_time, s.end_time) {
            row(ui, "Time", format!("{start} to {end} s"));
        }
        if let Some(dt) = s.delta_t {
            row(ui, "Time step", format!("{dt} s"));
        }
        if let Some(every) = s.write_interval {
            row(ui, "Writes every", every.to_string());
        }
        if let Some(Ok(mesh)) = &s.mesh {
            row(ui, "Cells", grouped(mesh.cells));
            row(ui, "Faces", grouped(mesh.faces));
            row(ui, "Points", grouped(mesh.points));
            row(ui, "Patches", mesh.patches.len().to_string());
        }
        if let Some(last) = s.times.last() {
            row(ui, "Written up to", format!("{last} s"));
        }
    });
    match &s.mesh {
        None => {
            ui.add_space(space::SM);
            ui.label(RichText::new("The case has no mesh yet.").color(muted(p)));
        }
        Some(Err(message)) => {
            ui.add_space(space::SM);
            ui.colored_label(for_egui::color(p.status_danger_fg), message);
        }
        Some(Ok(_)) => {}
    }
}

fn patch_properties(ui: &mut egui::Ui, look: &Look, s: &Summary, name: &str) {
    let patch = s
        .mesh
        .as_ref()
        .and_then(|m| m.as_ref().ok())
        .and_then(|m| m.patches.iter().find(|p| p.name == name));
    let Some(patch) = patch else {
        ui.label(RichText::new("This patch is not in the mesh.").color(muted(&look.palette)));
        return;
    };
    eng::properties(ui, "patch", |ui| {
        eng::property(ui, look, "Name", |ui| eng::value(ui, look, &patch.name));
        eng::property(ui, look, "Type", |ui| eng::value(ui, look, &patch.kind));
        eng::property(ui, look, "Faces", |ui| {
            eng::value(ui, look, &grouped(patch.faces))
        });
    });
    if patch.kind == "wall" {
        ui.add_space(space::SM);
        ui.label(
            RichText::new("Shear, y+, forces and heat flux of this wall are under Results.")
                .font(eng::caption())
                .color(muted(&look.palette)),
        );
    }
}

// --- The main island -------------------------------------------------------------------------

/// The view the Browser and the ribbon picked, under its title.
fn case_view(ui: &mut egui::Ui, ctx: &Context, look: &Look, p: &Palette, case: &mut CaseView) {
    let running = case.running();
    eng::panel_header(ui, look, view_title(case.tab), |ui| {
        if running {
            ui.label(RichText::new("Running…").color(muted(p)));
            ui.spinner();
        }
    });
    if let Some(outcome) = &case.outcome {
        use crate::session::Outcome;
        let (tone, text) = match outcome {
            Outcome::Failed(message) => (p.status_danger_fg, message.as_str()),
            Outcome::Stopped => (
                p.status_warning_fg,
                "The run was stopped; what was written before stays in the case.",
            ),
            Outcome::Done => (p.status_success_fg, "The run finished."),
        };
        ui.colored_label(for_egui::color(tone), text);
        ui.add_space(space::SM);
    }
    case.ensure_viewer(ctx);
    case.ensure_conditions(ctx);
    match case.tab {
        Tab::Results => results_tab(ui, p, &case.dir.clone(), &mut case.results),
        Tab::Conditions => conditions_tab(ui, p, case),
        Tab::Numerics => file_tab(ui, p, case, File::Solution),
        Tab::Schemes => file_tab(ui, p, case, File::Schemes),
        Tab::Settings => settings_tab(ui, p, case),
        Tab::Mesh => {
            let summary = case.summary.as_ref();
            let source = Source {
                dir: case.dir.clone(),
                times: summary.map(|s| s.times.clone()).unwrap_or_default(),
                fields: summary.map(|s| s.fields.clone()).unwrap_or_default(),
            };
            mesh_tab(ui, p, case.viewer.as_mut(), &source);
        }
        Tab::Run => run_view(ui, p, case),
    }
}

fn run_view(ui: &mut egui::Ui, p: &Palette, case: &CaseView) {
    convergence_card(ui, p, case.report.as_ref());
    ui.add_space(space::MD);
    ui.label(RichText::new("Log").color(muted(p)));
    Frame::NONE
        .fill(for_egui::color(p.surface_sunken))
        .corner_radius(super::corner(nodalia_theme::component::CONTROL_RADIUS))
        .inner_margin(Margin::same(space::MD as i8))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if case.log.is_empty() {
                        ui.label(
                            RichText::new("The solver's output appears here.").color(muted(p)),
                        );
                    }
                    for line in &case.log {
                        ui.label(RichText::new(line).font(eng::value_font()));
                    }
                });
        });
}

// --- Status bar ------------------------------------------------------------------------------

/// What the run is doing, in a word and a tone.
fn run_state(case: &CaseView) -> (String, Tone) {
    use crate::session::Outcome;
    if case.running() {
        let text = match (&case.report, case.stopping) {
            (_, true) => "Stopping".to_owned(),
            (Some(report), false) => format!("Running · {}", status_label(&report.status).0),
            (None, false) => "Running".to_owned(),
        };
        return (text, Tone::Info);
    }
    match &case.outcome {
        Some(Outcome::Done) => ("Finished".into(), Tone::Success),
        Some(Outcome::Stopped) => ("Stopped".into(), Tone::Warning),
        Some(Outcome::Failed(_)) => ("Failed".into(), Tone::Danger),
        None => ("Ready".into(), Tone::Info),
    }
}

fn status_bar(ui: &mut egui::Ui, look: &Look, case: &CaseView) {
    let p = &look.palette;
    let (state, tone) = run_state(case);
    let dot: Color32 = tone_colors(p, tone).0;
    eng::status_bar(
        ui,
        |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, dot);
            ui.label(RichText::new(state).color(for_egui::color(p.text_primary)));
            if let Some(app) = &case.application {
                ui.label(RichText::new(format!("·  {app}")).color(muted(p)));
            }
        },
        |ui| {
            let Some(report) = &case.report else {
                return;
            };
            // Drawn from the right edge, so the last value is the first one drawn.
            eng::status_value(
                ui,
                look,
                &format!("change {}", scientific(report.field_change)),
            );
            eng::status_value(
                ui,
                look,
                &format!("equations {}", scientific(report.equation_error)),
            );
            eng::status_value(
                ui,
                look,
                &format!("mass {}", scientific(report.global_mass)),
            );
            if let Some(time) = report.time {
                eng::status_value(ui, look, &format!("t {time}"));
            }
        },
    );
}

impl CaseView {
    /// Opens a view, and forgets a patch picked in the Browser unless the view is the mesh.
    pub(super) fn go(&mut self, tab: Tab) {
        if tab != Tab::Mesh {
            self.selection = Selection::Nothing;
        }
        self.tab = tab;
    }
}

/// The app's icon, as the title bar and Home draw it.
const ICON: &[u8] = include_bytes!("../../../../resources/icons/vortia.png");

/// The side the icon is kept at: it is drawn at 24 and 52 points, so 128 serves both at 2x.
const ICON_SIDE: u32 = 128;

fn load_icon(ctx: &Context) -> egui::TextureHandle {
    let image = image::load_from_memory_with_format(ICON, image::ImageFormat::Png)
        .map(|img| {
            let small = image::imageops::resize(
                &img.to_rgba8(),
                ICON_SIDE,
                ICON_SIDE,
                image::imageops::FilterType::Lanczos3,
            );
            egui::ColorImage::from_rgba_unmultiplied(
                [ICON_SIDE as usize, ICON_SIDE as usize],
                small.as_raw(),
            )
        })
        // A broken icon leaves the space empty rather than stopping the window.
        .unwrap_or_else(|_| egui::ColorImage::filled([1, 1], Color32::TRANSPARENT));
    ctx.load_texture("vortia-icon", image, egui::TextureOptions::LINEAR)
}

impl VortiaApp {
    fn icon(&mut self, ctx: &Context) -> &egui::TextureHandle {
        self.icon.get_or_insert_with(|| load_icon(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convergence::Convergence;
    use crate::prefs::Prefs;

    fn case_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("system")).unwrap();
        std::fs::write(
            dir.path().join("system/controlDict"),
            "FoamFile { version 2.0; format ascii; class dictionary; object controlDict; }\n\
             application vortiaCentralFoam;\n",
        )
        .unwrap();
        dir
    }

    fn app_with(case: &std::path::Path) -> VortiaApp {
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case).unwrap();
        app
    }

    fn frame(app: &mut VortiaApp, ctx: &Context) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui));
        output.textures_delta.clear();
    }

    fn view(app: &mut VortiaApp) -> &mut CaseView {
        match &mut app.screen {
            Screen::Case(case) => case,
            Screen::Welcome => panic!("the test opened a case"),
        }
    }

    #[test]
    fn every_view_belongs_to_one_step_that_opens_on_a_view_of_its_own() {
        for tab in [
            Tab::Run,
            Tab::Mesh,
            Tab::Results,
            Tab::Conditions,
            Tab::Numerics,
            Tab::Schemes,
            Tab::Settings,
        ] {
            assert_eq!(Step::of(Step::of(tab).first()), Step::of(tab));
        }
        for step in Step::ALL {
            assert_eq!(Step::of(step.first()), step);
        }
    }

    #[test]
    fn the_tree_lists_the_mesh_the_fields_and_the_solver_files_and_folds() {
        let dir = case_dir();
        let mut app = app_with(dir.path());
        let case = view(&mut app);
        let labels = |case: &CaseView| tree(case).into_iter().map(|r| r.label).collect::<Vec<_>>();
        assert_eq!(
            labels(case),
            [
                case.name().as_str(),
                "Mesh",
                "Boundary conditions",
                "Solver",
                "Run settings",
                "Numerics",
                "Schemes",
                "Run",
                "Results"
            ]
        );
        case.collapsed.insert("solver");
        assert!(!labels(case).contains(&"Schemes".to_owned()));
        case.collapsed.insert("case");
        assert_eq!(labels(case).len(), 1);
    }

    #[test]
    fn a_view_is_marked_in_the_tree_and_a_picked_patch_is_forgotten_when_leaving_the_mesh() {
        let dir = case_dir();
        let mut app = app_with(dir.path());
        let case = view(&mut app);
        case.go(Tab::Numerics);
        let selected: Vec<_> = tree(case)
            .into_iter()
            .filter(|r| r.selected)
            .map(|r| r.label)
            .collect();
        assert_eq!(selected, ["Numerics"]);
        case.go(Tab::Mesh);
        case.selection = Selection::Patch("inlet".into());
        case.go(Tab::Mesh);
        assert_eq!(case.selection, Selection::Patch("inlet".into()));
        case.go(Tab::Schemes);
        assert_eq!(case.selection, Selection::Nothing);
    }

    #[test]
    fn commands_move_between_home_the_views_and_the_dialogs() {
        let dir = case_dir();
        let ctx = Context::default();
        let mut app = app_with(dir.path());
        assert!(!app.home);
        app.perform(&ctx, Cmd::Open);
        assert!(app.home && app.focus_open);
        app.perform(&ctx, Cmd::Go(Tab::Schemes));
        assert!(!app.home);
        assert_eq!(view(&mut app).tab, Tab::Schemes);
        app.perform(&ctx, Cmd::Show(Dialog::About));
        assert_eq!(app.dialog, Some(Dialog::About));
        app.perform(&ctx, Cmd::CloseCase);
        assert!(matches!(app.screen, Screen::Welcome));
        // With no case open, a view cannot be gone to.
        app.perform(&ctx, Cmd::Go(Tab::Mesh));
        assert!(matches!(app.screen, Screen::Welcome));
    }

    #[test]
    fn the_command_search_offers_what_the_menus_do() {
        assert!(COMMANDS.iter().any(|(_, c)| *c == Cmd::Run));
        assert!(COMMANDS.iter().any(|(_, c)| *c == Cmd::Go(Tab::Conditions)));
        let labels: Vec<_> = COMMANDS.iter().map(|(l, _)| *l).collect();
        let mut unique = labels.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(labels.len(), unique.len());
    }

    #[test]
    fn the_status_bar_tells_the_state_of_the_run() {
        let dir = case_dir();
        let mut app = app_with(dir.path());
        let case = view(&mut app);
        assert_eq!(run_state(case), ("Ready".to_owned(), Tone::Info));
        case.outcome = Some(crate::session::Outcome::Failed("boom".into()));
        assert_eq!(run_state(case).1, Tone::Danger);
        case.outcome = Some(crate::session::Outcome::Stopped);
        assert_eq!(run_state(case).1, Tone::Warning);
        case.outcome = Some(crate::session::Outcome::Done);
        assert_eq!(run_state(case), ("Finished".to_owned(), Tone::Success));
    }

    #[test]
    fn home_the_case_and_the_search_draw_in_both_themes_with_everything_open() {
        let dir = case_dir();
        for theme in [crate::prefs::Theme::Light, crate::prefs::Theme::Dark] {
            let ctx = Context::default();
            let mut app = VortiaApp::with_prefs(
                Prefs {
                    theme,
                    recent: vec![dir.path().to_path_buf()],
                },
                None,
            );
            frame(&mut app, &ctx);
            app.open_case(dir.path()).unwrap();
            frame(&mut app, &ctx);
            view(&mut app).report = Convergence::parse(
                r#"{"status":"stabilizing","automatic":true,"stableSamples":1,"requiredSamples":5,"globalMass":1e-4,"iteration":12}"#,
            );
            for tab in [
                Tab::Run,
                Tab::Mesh,
                Tab::Results,
                Tab::Conditions,
                Tab::Numerics,
                Tab::Schemes,
                Tab::Settings,
            ] {
                view(&mut app).go(tab);
                frame(&mut app, &ctx);
            }
            view(&mut app).selection = Selection::Patch("nowhere".into());
            frame(&mut app, &ctx);
            app.home = true;
            frame(&mut app, &ctx);
            app.search = Some("run".into());
            frame(&mut app, &ctx);
            assert!(app.search.is_some());
        }
    }
}
