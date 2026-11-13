//! The window: Nodalia islands on the window surface, a header with the hamburger menu, and
//! two screens, the welcome and the case.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, Frame, Id, Key, KeyboardShortcut, Layout, Margin,
    Modifiers, RichText, Sense, Stroke, Vec2, ViewportBuilder, ViewportCommand,
};
use nodalia_theme::{Accent, Mode, Palette, component, font_size, for_egui, icons, radius, space};

use crate::conditions::{self, Loaded};
use crate::controldict::{self, SettingsForm};
use crate::convergence::Convergence;
#[cfg(target_os = "linux")]
use crate::globalmenu::{Entry, GlobalMenu, Menu};
use crate::numerics::{self, Group};
use crate::prefs::{self, Prefs, Theme};
use crate::session::{Event, Outcome, Session};
use crate::summary::{self, Summary};
use crate::surface;
use crate::viewer::{Source, Viewer};

/// The accent of the app.
const ACCENT: Accent = Accent::Vortia;
/// The height of the header.
const HEADER: f32 = 48.0;
/// The size of a window control.
const CONTROL: f32 = 28.0;
/// The log lines kept; the solver prints a few per step.
const MAX_LOG: usize = 5_000;
/// The edge of the window that resizes it, with the window's own decorations off.
const GRIP: f32 = 6.0;
/// The width of the case's summary beside the run.
const SIDE: f32 = 300.0;

/// The viewport the app opens in. Linux draws its own window controls (NDL), so it asks for
/// no decorations; macOS and Windows keep the system's.
pub fn viewport() -> ViewportBuilder {
    let builder = ViewportBuilder::default()
        .with_title("Vortia")
        .with_app_id("dev.danielmiguelt.Vortia")
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([760.0, 520.0]);
    if own_controls() {
        builder.with_decorations(false).with_transparent(true)
    } else {
        builder
    }
}

fn own_controls() -> bool {
    cfg!(all(unix, not(target_os = "macos")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dialog {
    Preferences,
    Shortcuts,
    About,
}

/// The run of one case, and what it has reported.
struct CaseView {
    dir: PathBuf,
    application: Option<String>,
    session: Option<Session>,
    log: VecDeque<String>,
    report: Option<Convergence>,
    outcome: Option<Outcome>,
    /// A stop was asked for and the engine has not ended yet.
    stopping: bool,
    summary: Option<Summary>,
    summary_rx: Option<std::sync::mpsc::Receiver<Summary>>,
    /// The folder changed (a run ended): read it again.
    summary_stale: bool,
    tab: Tab,
    /// The run settings as typed, and how the last save went (`true` when it worked).
    form: Option<SettingsForm>,
    form_message: Option<(bool, String)>,
    /// The boundary conditions of one field as typed, the field asked for, and how the last
    /// save went.
    conditions: Option<Loaded>,
    conditions_rx: Option<std::sync::mpsc::Receiver<Loaded>>,
    conditions_field: Option<String>,
    conditions_message: Option<(bool, String)>,
    /// The numbers of the solver controls as typed (read when the tab is first looked at), and
    /// how the last save went.
    numerics: Option<Result<Vec<Group>, String>>,
    numerics_message: Option<(bool, String)>,
    /// A field to colour the mesh with as soon as it is read, asked for on the command line.
    first_field: Option<String>,
    /// The mesh is read the first time it is looked at.
    viewer: Option<Result<Viewer, String>>,
    viewer_rx: Option<std::sync::mpsc::Receiver<Result<surface::Surface, String>>>,
}

/// A tab that can be asked for from outside, on the command line.
#[derive(Clone, Copy)]
pub enum TabName {
    Conditions,
    Numerics,
    Settings,
}

/// What the main part of the case screen shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Run,
    Mesh,
    Conditions,
    Numerics,
    Settings,
}

impl CaseView {
    fn new(dir: PathBuf, application: Option<String>) -> Self {
        Self {
            dir,
            application,
            session: None,
            log: VecDeque::new(),
            report: None,
            outcome: None,
            stopping: false,
            summary: None,
            summary_rx: None,
            summary_stale: true,
            tab: Tab::Run,
            form: None,
            form_message: None,
            conditions: None,
            conditions_rx: None,
            conditions_field: None,
            conditions_message: None,
            numerics: None,
            numerics_message: None,
            first_field: None,
            viewer: None,
            viewer_rx: None,
        }
    }

    /// The folder the conditions are edited in: where the run starts.
    fn start_time(&self) -> String {
        let start = self.summary.as_ref().and_then(|s| s.start_time);
        vortia_foamfile::write::scalar(start.unwrap_or(0.0))
    }

    /// Reads the field of the Conditions tab on a thread when it is wanted, and takes what it read.
    fn ensure_conditions(&mut self, ctx: &Context) {
        if let Some(rx) = &self.conditions_rx
            && let Ok(loaded) = rx.try_recv()
        {
            self.conditions = Some(loaded);
            self.conditions_rx = None;
        }
        if self.tab == Tab::Conditions
            && self.conditions.is_none()
            && self.conditions_rx.is_none()
            && self.summary.is_some()
        {
            let repaint = ctx.clone();
            self.conditions_rx = Some(conditions::spawn(
                self.dir.clone(),
                self.start_time(),
                self.conditions_field.clone(),
                move || repaint.request_repaint(),
            ));
        }
    }

    /// Reads the mesh on a thread the first time the viewer is wanted, and takes what it read.
    fn ensure_viewer(&mut self, ctx: &Context) {
        if let Some(rx) = &self.viewer_rx
            && let Ok(read) = rx.try_recv()
        {
            let first_field = self.first_field.take();
            self.viewer = Some(read.map(|surface| {
                let mut viewer = Viewer::new(surface);
                viewer.colour_by(first_field.as_deref(), None);
                viewer
            }));
            self.viewer_rx = None;
        }
        if self.tab == Tab::Mesh && self.viewer.is_none() && self.viewer_rx.is_none() {
            let repaint = ctx.clone();
            self.viewer_rx = Some(surface::spawn(self.dir.clone(), move || {
                repaint.request_repaint()
            }));
        }
    }

    /// Reads the folder on a thread when it has not been read, or has changed since.
    fn ensure_summary(&mut self, ctx: &Context) {
        if let Some(rx) = &self.summary_rx
            && let Ok(summary) = rx.try_recv()
        {
            self.summary = Some(summary);
            self.summary_rx = None;
        }
        if self.summary_stale && self.summary_rx.is_none() {
            let repaint = ctx.clone();
            self.summary_rx = Some(summary::spawn(self.dir.clone(), move || {
                repaint.request_repaint()
            }));
            self.summary_stale = false;
        }
    }

    fn name(&self) -> String {
        self.dir.file_name().map_or_else(
            || self.dir.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    fn running(&self) -> bool {
        self.session.is_some()
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() >= MAX_LOG {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// Takes what the engine's thread has said.
    fn poll(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let events = session.poll();
        let done = session.is_finished();
        for event in events {
            match event {
                Event::Log(line) => self.push_log(line),
                Event::Report(report) => self.report = Some(report),
                Event::Finished(result) => self.outcome = Some(result),
            }
        }
        if done {
            self.session = None;
            self.summary_stale = true;
        }
    }
}

enum Screen {
    Welcome,
    Case(Box<CaseView>),
}

pub struct VortiaApp {
    prefs: Prefs,
    prefs_file: Option<PathBuf>,
    applied: Option<Theme>,
    path_input: String,
    notice: Option<String>,
    screen: Screen,
    dialog: Option<Dialog>,
    /// The menus in the shell's top bar, where there is one.
    #[cfg(target_os = "linux")]
    global_menu: Option<GlobalMenu>,
}

/// The menus the shell shows for the app, each action named for [`VortiaApp::act`].
#[cfg(target_os = "linux")]
fn global_menus() -> Vec<Menu> {
    vec![
        Menu {
            title: "Vortia".into(),
            entries: vec![
                Entry::action("About Vortia", "about"),
                Entry::Separator,
                Entry::action("Preferences", "preferences"),
                Entry::action("Keyboard shortcuts", "shortcuts"),
                Entry::Separator,
                Entry::action("Quit", "quit"),
            ],
        },
        Menu {
            title: "Case".into(),
            entries: vec![
                Entry::action("Run", "run"),
                Entry::action("Stop", "stop"),
                Entry::Separator,
                Entry::action("Close case", "close"),
            ],
        },
    ]
}

impl VortiaApp {
    /// `case` is a folder named on the command line, opened at once, and run when `run`.
    pub fn new(ctx: &Context, case: Option<OsString>, run: bool) -> Self {
        let prefs_file = prefs::default_file();
        let prefs = prefs_file
            .as_deref()
            .map(Prefs::load_from)
            .unwrap_or_default();
        let mut app = Self::with_prefs(prefs, prefs_file);
        #[cfg(target_os = "linux")]
        {
            let wake = ctx.clone();
            app.global_menu = GlobalMenu::start(&global_menus(), move || wake.request_repaint());
        }
        app.apply_theme(ctx);
        if let Some(case) = case {
            let case = PathBuf::from(case);
            match app.open_case(&case) {
                Ok(()) if run => app.start_run(ctx),
                Ok(()) => {}
                Err(message) => app.notice = Some(message),
            }
        }
        app
    }

    pub fn with_prefs(prefs: Prefs, prefs_file: Option<PathBuf>) -> Self {
        Self {
            prefs,
            prefs_file,
            applied: None,
            path_input: String::new(),
            notice: None,
            screen: Screen::Welcome,
            dialog: None,
            #[cfg(target_os = "linux")]
            global_menu: None,
        }
    }

    /// Opens a case folder: it has to hold `system/controlDict`.
    pub fn open_case(&mut self, dir: &Path) -> Result<(), String> {
        if !dir.is_dir() {
            return Err(format!("'{}' is not a folder.", dir.display()));
        }
        if !dir.join("system").join("controlDict").is_file() {
            return Err(format!(
                "'{}' is not a case: it has no system/controlDict.",
                dir.display()
            ));
        }
        // Only the controlDict: a case that is not ready to run still opens, and the run says why.
        let application = vortia_solvers::case::read_dict(&dir.join("system").join("controlDict"))
            .ok()
            .and_then(|control| control.word("application").map(str::to_owned));
        self.prefs.remember(dir);
        self.save_prefs();
        self.notice = None;
        self.screen = Screen::Case(Box::new(CaseView::new(dir.to_path_buf(), application)));

        Ok(())
    }

    /// Switches the open case to its mesh.
    pub fn show_mesh(&mut self) {
        if let Screen::Case(case) = &mut self.screen {
            case.tab = Tab::Mesh;
        }
    }

    /// Switches the open case to one of its tabs.
    pub fn show_tab(&mut self, tab: TabName) {
        if let Screen::Case(case) = &mut self.screen {
            case.tab = match tab {
                TabName::Conditions => Tab::Conditions,
                TabName::Numerics => Tab::Numerics,
                TabName::Settings => Tab::Settings,
            };
        }
    }

    /// Switches the open case to its mesh, coloured with a field of the latest time.
    pub fn show_mesh_field(&mut self, field: &str) {
        self.show_mesh();
        if let Screen::Case(case) = &mut self.screen {
            case.first_field = Some(field.to_owned());
        }
    }

    fn save_prefs(&self) {
        if let Some(file) = &self.prefs_file {
            // Preferences are not worth interrupting the user for.
            let _ = self.prefs.save_to(file);
        }
    }

    fn start_run(&mut self, ctx: &Context) {
        let Screen::Case(case) = &mut self.screen else {
            return;
        };
        if case.running() {
            return;
        }
        let repaint = ctx.clone();
        match Session::start(&case.dir, move || repaint.request_repaint()) {
            Ok(session) => {
                case.session = Some(session);
                case.log.clear();
                case.report = None;
                case.outcome = None;
                case.stopping = false;
            }
            Err(e) => {
                case.outcome = Some(Outcome::Failed(format!("The engine could not start: {e}")))
            }
        }
    }

    fn stop_run(&mut self) {
        if let Screen::Case(case) = &mut self.screen
            && let Some(session) = &case.session
            && !case.stopping
        {
            session.stop();
            case.stopping = true;
        }
    }

    fn close_case(&mut self) {
        if matches!(&self.screen, Screen::Case(case) if !case.running()) {
            self.screen = Screen::Welcome;
        }
    }

    /// A command that came from the shell's menu.
    #[cfg(target_os = "linux")]
    fn act(&mut self, ctx: &Context, action: &str) {
        match action {
            "about" => self.dialog = Some(Dialog::About),
            "preferences" => self.dialog = Some(Dialog::Preferences),
            "shortcuts" => self.dialog = Some(Dialog::Shortcuts),
            "quit" => ctx.send_viewport_cmd(ViewportCommand::Close),
            "run" => self.start_run(ctx),
            "stop" => self.stop_run(),
            "close" => self.close_case(),
            _ => {}
        }
    }

    /// Takes the shell's clicks, and tells it which of the case's commands apply now.
    #[cfg(target_os = "linux")]
    fn sync_global_menu(&mut self, ctx: &Context) {
        let Some(menu) = &self.global_menu else {
            return;
        };
        let clicks = menu.poll();
        let (open, running, stopping) = match &self.screen {
            Screen::Case(case) => (true, case.running(), case.stopping),
            Screen::Welcome => (false, false, false),
        };
        menu.set_enabled("run", open && !running);
        menu.set_enabled("stop", running && !stopping);
        menu.set_enabled("close", open && !running);
        for action in clicks {
            self.act(ctx, &action);
        }
    }

    fn apply_theme(&mut self, ctx: &Context) {
        if self.applied == Some(self.prefs.theme) {
            return;
        }
        let base = for_egui::Appearance {
            accent: ACCENT,
            ..for_egui::Appearance::default()
        };
        match self.prefs.theme {
            Theme::System => for_egui::apply_system(ctx, &base),
            Theme::Light => for_egui::apply(
                ctx,
                &for_egui::Appearance {
                    mode: Mode::Light,
                    ..base
                },
            ),
            Theme::Dark => for_egui::apply(
                ctx,
                &for_egui::Appearance {
                    mode: Mode::Dark,
                    ..base
                },
            ),
        }
        self.applied = Some(self.prefs.theme);
    }

    fn shortcuts(&mut self, ctx: &Context) {
        let command = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        let take = |shortcut: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&shortcut));
        if take(command(Key::Comma)) {
            self.dialog = Some(Dialog::Preferences);
        }
        if take(command(Key::Questionmark)) || take(KeyboardShortcut::new(Modifiers::NONE, Key::F1))
        {
            self.dialog = Some(Dialog::Shortcuts);
        }
        if take(command(Key::R)) {
            self.start_run(ctx);
        }
        if take(command(Key::Q)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        if take(KeyboardShortcut::new(Modifiers::NONE, Key::Escape)) {
            self.dialog = None;
        }
    }

    /// One frame of the interface.
    pub fn ui(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        self.apply_theme(ctx);
        if let Screen::Case(case) = &mut self.screen {
            case.poll();
            case.ensure_summary(ctx);
            if case.running() {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        #[cfg(target_os = "linux")]
        self.sync_global_menu(ctx);
        self.shortcuts(ctx);

        let p = palette(ctx);
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let round = if own_controls() && !maximized {
            radius::XXL
        } else {
            0.0
        };
        let frame = Frame::NONE
            .fill(for_egui::color(p.surface_window))
            .corner_radius(corner(round));

        egui::CentralPanel::default().frame(frame).show(root, |ui| {
            self.header(ui, ctx, &p, maximized);
            let gap = component::WINDOW_GAP;
            Frame::NONE
                .inner_margin(Margin {
                    left: gap as i8,
                    right: gap as i8,
                    top: 0,
                    bottom: gap as i8,
                })
                .show(ui, |ui| {
                    Frame::NONE
                        .fill(for_egui::color(p.surface_panel))
                        .corner_radius(corner(component::PANEL_RADIUS))
                        .inner_margin(Margin::same(space::XL as i8))
                        .show(ui, |ui| {
                            ui.set_min_size(ui.available_size());
                            match self.screen {
                                Screen::Welcome => self.welcome(ui, ctx, &p),
                                Screen::Case(_) => self.case_screen(ui, ctx, &p),
                            }
                        });
                });
        });

        self.dialogs(ctx, &p);
        if own_controls() && !maximized {
            resize_edges(ctx);
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, ctx: &Context, p: &Palette, maximized: bool) {
        let rect =
            egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), HEADER));
        // Under the widgets: the empty part of the bar moves the window.
        let bar = ui.interact(rect, Id::unique("header-drag"), Sense::click_and_drag());
        if own_controls() {
            if bar.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if bar.double_clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            }
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(space::MD);
                let menu = icon_button(ui, p, "menu");
                egui::Popup::menu(&menu).show(|ui| {
                    ui.set_min_width(200.0);
                    if ui.button("Preferences").clicked() {
                        self.dialog = Some(Dialog::Preferences);
                    }
                    if ui.button("Keyboard shortcuts").clicked() {
                        self.dialog = Some(Dialog::Shortcuts);
                    }
                    if ui.button("About Vortia").clicked() {
                        self.dialog = Some(Dialog::About);
                    }
                });
                ui.label(RichText::new("Vortia").size(font_size::TITLE).strong());
                if let Screen::Case(case) = &self.screen {
                    ui.label(RichText::new(format!("·  {}", case.name())).color(muted(p)));
                }
                if own_controls() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(space::MD);
                        if control(ui, "close", p.status_danger_fg).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        let (icon, tint) = if maximized {
                            ("fullscreen-exit", p.accent_default)
                        } else {
                            ("fullscreen", p.accent_default)
                        };
                        if control(ui, icon, tint).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                        }
                        if control(ui, "minus", p.status_warning_fg).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                        }
                    });
                }
            });
        });
        ui.advance_cursor_after_rect(rect);
    }

    fn welcome(&mut self, ui: &mut egui::Ui, ctx: &Context, p: &Palette) {
        ui.vertical_centered(|ui| {
            ui.add_space(space::XXL);
            ui.label(RichText::new("Vortia").size(font_size::DISPLAY).strong());
            ui.label(
                RichText::new("Compressible flow, solved by the Vortia engine")
                    .size(font_size::LARGE)
                    .color(muted(p)),
            );
            ui.add_space(space::XL);
            let mut open = false;
            ui.horizontal(|ui| {
                let width = 420.0_f32.min(ui.available_width() - 160.0).max(200.0);
                ui.add_space(((ui.available_width() - width - 140.0) / 2.0).max(0.0));
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.path_input)
                        .hint_text("Folder of a case")
                        .desired_width(width),
                );
                open |= field.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
                open |= accent_button(ui, p, "Open case").clicked();
            });
            if open {
                let dir = PathBuf::from(self.path_input.trim());
                match self.open_case(&dir) {
                    Ok(()) => self.path_input.clear(),
                    Err(message) => self.notice = Some(message),
                }
            }
            if let Some(notice) = &self.notice {
                ui.add_space(space::SM);
                ui.colored_label(for_egui::color(p.status_danger_fg), notice);
            }
            ui.add_space(space::XL);
        });
        if self.prefs.recent.is_empty() {
            return;
        }
        ui.label(RichText::new("Recent cases").color(muted(p)));
        ui.add_space(space::XS);
        let mut chosen = None;
        for dir in self.prefs.recent.clone() {
            let name = dir.file_name().map_or_else(
                || dir.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let row = ui.add(
                egui::Button::new(RichText::new(name).size(font_size::LARGE))
                    .frame(false)
                    .min_size(Vec2::new(ui.available_width(), 28.0)),
            );
            ui.label(
                RichText::new(dir.display().to_string())
                    .size(font_size::SMALL)
                    .color(muted(p)),
            );
            if row.clicked() {
                chosen = Some(dir);
            }
        }
        if let Some(dir) = chosen
            && let Err(message) = self.open_case(&dir)
        {
            self.notice = Some(message);
        }
    }

    fn case_screen(&mut self, ui: &mut egui::Ui, ctx: &Context, p: &Palette) {
        let mut run = false;
        let mut close = false;
        let mut stop = false;
        let Screen::Case(case) = &mut self.screen else {
            return;
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(case.name())
                        .size(font_size::HEADLINE)
                        .strong(),
                );
                let solver = case.application.as_deref().unwrap_or("no solver named");
                ui.label(
                    RichText::new(format!("{solver}  ·  {}", case.dir.display()))
                        .size(font_size::SMALL)
                        .color(muted(p)),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if case.session.is_some() {
                    let stopping = case.stopping;
                    let label = if stopping { "Stopping…" } else { "Stop" };
                    let button = ui.add_enabled(!stopping, egui::Button::new(label));
                    if button.clicked() {
                        stop = true;
                    }
                } else {
                    run = accent_button(ui, p, "Run").clicked();
                    close = ui.button("Close case").clicked();
                }
            });
        });
        ui.add_space(space::MD);

        if let Some(outcome) = &case.outcome {
            match outcome {
                Outcome::Failed(message) => {
                    ui.colored_label(for_egui::color(p.status_danger_fg), message)
                }
                Outcome::Stopped => ui.colored_label(
                    for_egui::color(p.status_warning_fg),
                    "The run was stopped; what was written before stays in the case.",
                ),
                Outcome::Done => {
                    ui.colored_label(for_egui::color(p.status_success_fg), "The run finished.")
                }
            };
            ui.add_space(space::SM);
        } else if case.running() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Running…");
            });
            ui.add_space(space::SM);
        }

        let room = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(SIDE, room.y),
                Layout::top_down(Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("case-summary")
                        .auto_shrink([false, false])
                        .show(ui, |ui| summary_card(ui, p, case.summary.as_ref()));
                },
            );
            let width = ui.available_width();
            ui.allocate_ui_with_layout(
                Vec2::new(width, room.y),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut case.tab, Tab::Run, "Run");
                        ui.selectable_value(&mut case.tab, Tab::Mesh, "Mesh");
                        ui.selectable_value(&mut case.tab, Tab::Conditions, "Conditions");
                        ui.selectable_value(&mut case.tab, Tab::Numerics, "Numerics");
                        ui.selectable_value(&mut case.tab, Tab::Settings, "Settings");
                    });
                    ui.add_space(space::SM);
                    case.ensure_viewer(ctx);
                    case.ensure_conditions(ctx);
                    if case.tab == Tab::Conditions {
                        conditions_tab(ui, p, case);
                        return;
                    }
                    if case.tab == Tab::Numerics {
                        numerics_tab(ui, p, case);
                        return;
                    }
                    if case.tab == Tab::Settings {
                        settings_tab(ui, p, case);
                        return;
                    }
                    if case.tab == Tab::Mesh {
                        let source = Source {
                            dir: case.dir.clone(),
                            times: case
                                .summary
                                .as_ref()
                                .map(|s| s.times.clone())
                                .unwrap_or_default(),
                            fields: case
                                .summary
                                .as_ref()
                                .map(|s| s.fields.clone())
                                .unwrap_or_default(),
                        };
                        mesh_tab(ui, p, case.viewer.as_mut(), &source);
                        return;
                    }
                    convergence_card(ui, p, case.report.as_ref());
                    ui.add_space(space::MD);

                    ui.label(RichText::new("Log").color(muted(p)));
                    Frame::NONE
                        .fill(for_egui::color(p.surface_sunken))
                        .corner_radius(corner(component::CONTROL_RADIUS))
                        .inner_margin(Margin::same(space::MD as i8))
                        .show(ui, |ui| {
                            ui.set_min_size(ui.available_size());
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    if case.log.is_empty() {
                                        ui.label(
                                            RichText::new("The solver's output appears here.")
                                                .color(muted(p)),
                                        );
                                    }
                                    for line in &case.log {
                                        ui.label(
                                            RichText::new(line).monospace().size(font_size::SMALL),
                                        );
                                    }
                                });
                        });
                },
            );
        });

        if run {
            self.start_run(ctx);
        }
        if stop {
            self.stop_run();
        }
        if close {
            self.close_case();
        }
    }

    fn dialogs(&mut self, ctx: &Context, p: &Palette) {
        let Some(dialog) = self.dialog else {
            return;
        };
        let title = match dialog {
            Dialog::Preferences => "Preferences",
            Dialog::Shortcuts => "Keyboard shortcuts",
            Dialog::About => "About Vortia",
        };
        let mut close = false;
        let before = self.prefs.theme;
        egui::Modal::new(Id::unique("dialog")).show(ctx, |ui| {
            ui.set_width(component_dialog_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(font_size::TITLE).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close = control(ui, "close", p.status_danger_fg).clicked();
                });
            });
            ui.add_space(space::MD);
            match dialog {
                Dialog::Preferences => {
                    ui.label(RichText::new("Appearance").color(muted(p)));
                    for theme in Theme::ALL {
                        ui.selectable_value(&mut self.prefs.theme, theme, theme_label(theme));
                    }
                }
                Dialog::Shortcuts => {
                    egui::Grid::new("shortcuts")
                        .num_columns(2)
                        .spacing([space::XL, space::SM])
                        .show(ui, |ui| {
                            for (keys, what) in SHORTCUTS {
                                ui.label(RichText::new(*keys).monospace());
                                ui.label(*what);
                                ui.end_row();
                            }
                        });
                }
                Dialog::About => {
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(space::SM);
                    ui.label("A compressible-flow solver and its desktop app, written in Rust.");
                    ui.add_space(space::SM);
                    ui.label(RichText::new("GPL-3.0-or-later").color(muted(p)));
                }
            }
        });
        if close {
            self.dialog = None;
        }
        if self.prefs.theme != before {
            self.apply_theme(ctx);
            self.save_prefs();
        }
    }
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+R", "Run the case"),
    ("Ctrl+,", "Preferences"),
    ("F1", "Keyboard shortcuts"),
    ("Ctrl+Q", "Quit"),
    ("Esc", "Close a dialog"),
];

fn component_dialog_width() -> f32 {
    nodalia_theme::size::DIALOG
}

fn theme_label(theme: Theme) -> &'static str {
    match theme {
        Theme::System => "Follow the system",
        Theme::Light => "Light",
        Theme::Dark => "Dark",
    }
}

/// What a convergence status says to the user, and in which tone.
pub fn status_label(status: &str) -> (&'static str, Tone) {
    match status {
        "warming" => ("Warming up", Tone::Info),
        "checking" => ("Checking", Tone::Info),
        "stabilizing" => ("Settling", Tone::Warning),
        "converged" => ("Converged", Tone::Success),
        "limit" => ("Time limit reached", Tone::Warning),
        "invalid" => ("The solution left its physical range", Tone::Danger),
        _ => ("Unknown", Tone::Info),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Warning,
    Danger,
}

/// A number in scientific notation, or a dash when the engine could not give it.
pub fn scientific(value: Option<f64>) -> String {
    value.map_or_else(|| "–".to_owned(), |v| format!("{v:.2e}"))
}

/// A count with a thin space between thousands.
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

fn summary_card(ui: &mut egui::Ui, p: &Palette, summary: Option<&Summary>) {
    Frame::NONE
        .fill(for_egui::color(p.surface_sunken))
        .corner_radius(corner(component::CONTROL_RADIUS))
        .inner_margin(Margin::same(space::MD as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(s) = summary else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Reading the case…").color(muted(p)));
                });
                return;
            };
            let row = |ui: &mut egui::Ui, name: &str, value: String| {
                ui.label(RichText::new(name).color(muted(p)));
                ui.label(value);
                ui.end_row();
            };
            egui::Grid::new("case-settings")
                .num_columns(2)
                .spacing([space::LG, space::XS])
                .show(ui, |ui| {
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
                        row(ui, "Points", grouped(mesh.points));
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
                Some(Ok(mesh)) => {
                    ui.add_space(space::MD);
                    ui.label(RichText::new("Patches").color(muted(p)));
                    egui::Grid::new("case-patches")
                        .num_columns(3)
                        .spacing([space::MD, space::XS])
                        .show(ui, |ui| {
                            for patch in &mesh.patches {
                                ui.label(&patch.name);
                                ui.label(RichText::new(&patch.kind).color(muted(p)));
                                ui.label(
                                    RichText::new(grouped(patch.faces))
                                        .monospace()
                                        .color(muted(p)),
                                );
                                ui.end_row();
                            }
                        });
                }
            }
            if !s.fields.is_empty() {
                ui.add_space(space::MD);
                ui.label(RichText::new("Fields").color(muted(p)));
                ui.label(s.fields.join(", "));
            }
            if let Some(last) = s.times.last() {
                ui.add_space(space::MD);
                ui.label(RichText::new("Written up to").color(muted(p)));
                ui.label(format!("{last} s"));
            }
        });
}

fn convergence_card(ui: &mut egui::Ui, p: &Palette, report: Option<&Convergence>) {
    Frame::NONE
        .fill(for_egui::color(p.surface_sunken))
        .corner_radius(corner(component::CONTROL_RADIUS))
        .inner_margin(Margin::same(space::MD as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(r) = report else {
                ui.label(
                    RichText::new("Convergence appears once the solver has checked the run.")
                        .color(muted(p)),
                );
                return;
            };
            let (label, tone) = status_label(&r.status);
            ui.horizontal(|ui| {
                let (fg, bg) = tone_colors(p, tone);
                Frame::NONE
                    .fill(bg)
                    .corner_radius(corner(radius::FULL))
                    .inner_margin(Margin::symmetric(space::MD as i8, space::XS as i8))
                    .show(ui, |ui| ui.label(RichText::new(label).color(fg)));
                if r.automatic {
                    ui.label(
                        RichText::new(format!(
                            "{} of {} checks in a row",
                            r.stable_samples, r.required_samples
                        ))
                        .color(muted(p)),
                    );
                }
            });
            if r.automatic {
                ui.add_space(space::SM);
                ui.add(
                    egui::ProgressBar::new(r.progress()).desired_height(component::PROGRESS_HEIGHT),
                );
            }
            ui.add_space(space::SM);
            egui::Grid::new("convergence")
                .num_columns(3)
                .spacing([space::XL, space::XS])
                .show(ui, |ui| {
                    for (name, value, limit) in [
                        ("Mass balance", r.global_mass, r.mass_tolerance),
                        ("Local mass", r.local_mass, r.mass_tolerance),
                        ("Equations", r.equation_error, r.equation_tolerance),
                        ("Field change", r.field_change, r.field_tolerance),
                    ] {
                        ui.label(RichText::new(name).color(muted(p)));
                        ui.label(RichText::new(scientific(value)).monospace());
                        ui.label(
                            RichText::new(format!("limit {}", scientific(limit)))
                                .monospace()
                                .color(muted(p)),
                        );
                        ui.end_row();
                    }
                });
        });
}

/// The solver controls of the case (tolerances, relaxation, targets), to edit and save into its
/// fvSolution.
fn numerics_tab(ui: &mut egui::Ui, p: &Palette, case: &mut CaseView) {
    let running = case.running();
    let read = case
        .numerics
        .get_or_insert_with(|| numerics::read(&case.dir));
    let (mut save, mut revert) = (false, false);
    match read {
        Err(message) => {
            ui.colored_label(for_egui::color(p.status_danger_fg), message.as_str());
        }
        Ok(groups) => {
            let dirty = groups
                .iter()
                .any(|g| g.items.iter().any(|item| item.changed()));
            egui::ScrollArea::vertical()
                .id_salt("numerics")
                .auto_shrink([false, false])
                .max_height((ui.available_height() - 90.0).max(120.0))
                .show(ui, |ui| {
                    ui.add_enabled_ui(!running, |ui| {
                        for group in groups.iter_mut() {
                            ui.add_space(space::MD);
                            ui.label(RichText::new(group.title()).strong());
                            let id = group.title();
                            for item in &mut group.items {
                                condition_row(ui, p, &item.key.clone(), item, &id);
                            }
                        }
                    });
                });
            ui.add_space(space::MD);
            ui.add_enabled_ui(!running && dirty, |ui| {
                ui.horizontal(|ui| {
                    save = accent_button(ui, p, "Save").clicked();
                    revert = ui.button("Revert").clicked();
                });
            });
            if running {
                ui.label(
                    RichText::new("The numbers cannot change while the case runs.").color(muted(p)),
                );
            }
        }
    }
    if let Some((ok, message)) = &case.numerics_message {
        let tone = if *ok {
            p.status_success_fg
        } else {
            p.status_danger_fg
        };
        ui.colored_label(for_egui::color(tone), message.as_str());
    }
    if revert {
        case.numerics = None;
        case.numerics_message = None;
    } else if save && let Some(Ok(groups)) = &case.numerics {
        case.numerics_message = Some(match numerics::save(&case.dir, groups) {
            Ok(()) => (true, "Saved to system/fvSolution.".to_owned()),
            Err(message) => (false, message),
        });
        if matches!(case.numerics_message, Some((true, _))) {
            case.numerics = None;
        }
    }
}

/// The boundary conditions of one field of the case, to edit and save into the field's file.
fn conditions_tab(ui: &mut egui::Ui, p: &Palette, case: &mut CaseView) {
    let running = case.running();
    let time = case.start_time();
    let Some(loaded) = case.conditions.as_mut() else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Reading the field…");
        });
        return;
    };
    let mut pick: Option<String> = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Field").color(muted(p)));
        let shown = loaded
            .current
            .as_ref()
            .map(|c| c.field.clone())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("conditions-field")
            .selected_text(shown.as_str())
            .show_ui(ui, |ui| {
                for name in &loaded.fields {
                    if ui.selectable_label(*name == shown, name).clicked() && *name != shown {
                        pick = Some(name.clone());
                    }
                }
            });
        ui.label(
            RichText::new(format!("from the {time} folder"))
                .size(font_size::SMALL)
                .color(muted(p)),
        );
    });
    ui.add_space(space::SM);
    let (mut save, mut revert) = (false, false);
    match loaded.current.as_mut() {
        Err(message) => {
            ui.colored_label(for_egui::color(p.status_danger_fg), message.as_str());
        }
        Ok(field) => {
            let dirty = field.internal.changed()
                || field
                    .patches
                    .iter()
                    .any(|patch| patch.items.iter().any(|i| i.changed()));
            egui::ScrollArea::vertical()
                .id_salt("conditions")
                .auto_shrink([false, false])
                .max_height((ui.available_height() - 90.0).max(120.0))
                .show(ui, |ui| {
                    ui.add_enabled_ui(!running, |ui| {
                        condition_row(ui, p, "internalField", &mut field.internal, "Inside");
                        for patch in &mut field.patches {
                            ui.add_space(space::MD);
                            ui.label(RichText::new(&patch.patch).strong());
                            for item in &mut patch.items {
                                condition_row(ui, p, &item.key.clone(), item, &patch.patch);
                            }
                        }
                    });
                });
            ui.add_space(space::MD);
            ui.add_enabled_ui(!running && dirty, |ui| {
                ui.horizontal(|ui| {
                    save = accent_button(ui, p, "Save").clicked();
                    revert = ui.button("Revert").clicked();
                });
            });
            if running {
                ui.label(
                    RichText::new("The conditions cannot change while the case runs.")
                        .color(muted(p)),
                );
            }
        }
    }
    if let Some((ok, message)) = &case.conditions_message {
        let tone = if *ok {
            p.status_success_fg
        } else {
            p.status_danger_fg
        };
        ui.colored_label(for_egui::color(tone), message.as_str());
    }
    if let Some(name) = pick {
        case.conditions_field = Some(name);
        case.conditions = None;
        case.conditions_message = None;
    } else if revert {
        case.conditions = None;
        case.conditions_message = None;
    } else if save && let Some(Ok(field)) = case.conditions.as_ref().map(|l| &l.current) {
        let result = conditions::save(&case.dir, &time, field);
        case.conditions_message = Some(match result {
            Ok(()) => (true, format!("Saved to {time}/{}.", field.field)),
            Err(message) => (false, message),
        });
        if matches!(case.conditions_message, Some((true, _))) {
            // Read it again, so what shows is what the file says.
            case.conditions_field = Some(field.field.clone());
            case.conditions = None;
        }
    }
}

/// One entry: its name, and its value in a box when it can be edited.
fn condition_row(ui: &mut egui::Ui, p: &Palette, key: &str, item: &mut conditions::Item, id: &str) {
    ui.horizontal(|ui| {
        ui.add_space(space::MD);
        ui.allocate_ui_with_layout(
            Vec2::new(140.0, 24.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(140.0);
                ui.label(RichText::new(key).color(muted(p)));
            },
        );
        if item.editable {
            ui.add(
                egui::TextEdit::singleline(&mut item.text)
                    .id_salt((id, key))
                    .desired_width(200.0),
            );
        } else {
            ui.label(item.text.as_str());
        }
    });
}

/// The run settings of the case, to edit and save into its controlDict.
fn settings_tab(ui: &mut egui::Ui, p: &Palette, case: &mut CaseView) {
    if case.form.is_none()
        && let Some(summary) = &case.summary
    {
        case.form = Some(SettingsForm::from_summary(summary));
    }
    let running = case.running();
    let mut save = false;
    let mut revert = false;
    let Some(form) = case.form.as_mut() else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Reading the case…");
        });
        return;
    };
    ui.add_enabled_ui(!running, |ui| {
        egui::Grid::new("run-settings")
            .num_columns(3)
            .spacing([space::XL, space::MD])
            .show(ui, |ui| {
                for (label, unit, text) in [
                    ("Start time", "s", &mut form.start_time),
                    ("End time", "s", &mut form.end_time),
                    ("Time step", "s", &mut form.delta_t),
                    (
                        "Write interval",
                        "steps or s, as the case writes",
                        &mut form.write_interval,
                    ),
                ] {
                    ui.label(RichText::new(label).color(muted(p)));
                    ui.add(egui::TextEdit::singleline(text).desired_width(140.0));
                    ui.label(RichText::new(unit).size(font_size::SMALL).color(muted(p)));
                    ui.end_row();
                }
            });
        ui.add_space(space::LG);
        ui.horizontal(|ui| {
            save = accent_button(ui, p, "Save").clicked();
            revert = ui.button("Revert").clicked();
        });
    });
    if running {
        ui.add_space(space::SM);
        ui.label(RichText::new("The settings cannot change while the case runs.").color(muted(p)));
    }
    if let Some((ok, message)) = &case.form_message {
        ui.add_space(space::SM);
        let tone = if *ok {
            p.status_success_fg
        } else {
            p.status_danger_fg
        };
        ui.colored_label(for_egui::color(tone), message.as_str());
    }
    if revert {
        case.form = case.summary.as_ref().map(SettingsForm::from_summary);
        case.form_message = None;
    } else if save {
        let result = case
            .form
            .as_ref()
            .map(SettingsForm::parse)
            .unwrap_or_else(|| Err("Nothing to save yet.".to_owned()))
            .and_then(|settings| controldict::save(&case.dir, &settings));
        case.form_message = Some(match result {
            Ok(()) => {
                case.summary_stale = true;
                (true, "Saved to system/controlDict.".to_owned())
            }
            Err(message) => (false, message),
        });
    }
}

/// The mesh of the case, or why there is none to show.
fn mesh_tab(
    ui: &mut egui::Ui,
    p: &Palette,
    viewer: Option<&mut Result<Viewer, String>>,
    source: &Source,
) {
    match viewer {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading the mesh…");
            });
        }
        Some(Err(message)) => {
            ui.colored_label(for_egui::color(p.status_danger_fg), message.as_str());
        }
        Some(Ok(viewer)) => viewer.show(
            ui,
            for_egui::color(p.surface_sunken),
            muted(p),
            component::CONTROL_RADIUS.round().clamp(0.0, 255.0) as u8,
            source,
        ),
    }
}

fn tone_colors(p: &Palette, tone: Tone) -> (Color32, Color32) {
    let (fg, bg) = match tone {
        Tone::Info => (p.status_info_fg, p.status_info_bg),
        Tone::Success => (p.status_success_fg, p.status_success_bg),
        Tone::Warning => (p.status_warning_fg, p.status_warning_bg),
        Tone::Danger => (p.status_danger_fg, p.status_danger_bg),
    };
    (for_egui::color(fg), for_egui::color(bg))
}

fn palette(ctx: &Context) -> Palette {
    let mode = if ctx.global_style().visuals.dark_mode {
        Mode::Dark
    } else {
        Mode::Light
    };
    mode.palette_with(ACCENT)
}

fn muted(p: &Palette) -> Color32 {
    for_egui::color(p.text_secondary)
}

fn corner(value: f32) -> CornerRadius {
    CornerRadius::same(value.round().clamp(0.0, 255.0) as u8)
}

/// An icon of the NDL set painted into a square; nothing when the set lacks the name.
fn paint_icon(painter: &egui::Painter, rect: egui::Rect, name: &str, ink: Color32) {
    if let Some(icon) = icons::get(name) {
        icons::paint(painter, rect, icon, icons::Style::bold(ink));
    }
}

/// A flat square button with an icon.
fn icon_button(ui: &mut egui::Ui, p: &Palette, name: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CONTROL + 4.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            corner(radius::FULL),
            for_egui::color(p.control_bg_hover),
        );
    }
    paint_icon(
        ui.painter(),
        rect.shrink(7.0),
        name,
        for_egui::color(p.text_primary),
    );
    response
}

/// A round window control, tinted: the fill is the tint at low strength, the icon the tint.
fn control(ui: &mut egui::Ui, name: &str, tint: nodalia_theme::Rgba) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CONTROL), Sense::click());
    let ink = for_egui::color(tint);
    let strength = if response.hovered() { 0.34 } else { 0.18 };
    ui.painter()
        .circle_filled(rect.center(), CONTROL / 2.0, ink.gamma_multiply(strength));
    paint_icon(ui.painter(), rect.shrink(8.0), name, ink);
    response
}

fn accent_button(ui: &mut egui::Ui, p: &Palette, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(for_egui::color(p.text_on_accent)))
            .fill(for_egui::color(p.accent_default))
            .stroke(Stroke::NONE),
    )
}

/// With no window decorations the edges of the window are ours to resize by.
fn resize_edges(ctx: &Context) {
    use egui::ResizeDirection as D;
    let screen = ctx.content_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let left = pos.x - screen.min.x < GRIP;
    let right = screen.max.x - pos.x < GRIP;
    let top = pos.y - screen.min.y < GRIP;
    let bottom = screen.max.y - pos.y < GRIP;
    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => D::NorthWest,
        (_, true, true, _) => D::NorthEast,
        (true, _, _, true) => D::SouthWest,
        (_, true, _, true) => D::SouthEast,
        (true, ..) => D::West,
        (_, true, ..) => D::East,
        (_, _, true, _) => D::North,
        (.., true) => D::South,
        _ => return,
    };
    let icon = match direction {
        D::North | D::South => egui::CursorIcon::ResizeVertical,
        D::East | D::West => egui::CursorIcon::ResizeHorizontal,
        D::NorthEast | D::SouthWest => egui::CursorIcon::ResizeNeSw,
        D::NorthWest | D::SouthEast => egui::CursorIcon::ResizeNwSe,
    };
    ctx.set_cursor_icon(icon);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}

impl eframe::App for VortiaApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The window is transparent where its corners are rounded.
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        VortiaApp::ui(self, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut VortiaApp, ctx: &Context) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui));
        // Nobody paints these frames.
        output.textures_delta.clear();
    }

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

    #[test]
    fn only_a_folder_with_a_control_dict_opens() {
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        let empty = tempfile::tempdir().unwrap();
        assert!(app.open_case(&empty.path().join("missing")).is_err());
        assert!(
            app.open_case(empty.path())
                .unwrap_err()
                .contains("controlDict")
        );
        assert!(matches!(app.screen, Screen::Welcome));

        let case = case_dir();
        app.open_case(case.path()).unwrap();
        let Screen::Case(view) = &app.screen else {
            panic!("the case did not open");
        };
        assert_eq!(view.application.as_deref(), Some("vortiaCentralFoam"));
        assert_eq!(app.prefs.recent, [case.path().to_path_buf()]);
    }

    #[test]
    fn the_run_settings_of_a_case_are_edited_and_saved() {
        let case = case_dir();
        std::fs::write(
            case.path().join("system/controlDict"),
            "application vortiaCentralFoam;\nstartTime 0;\nendTime 10;\ndeltaT 0.5;\n\
             writeInterval 2;\n",
        )
        .unwrap();
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        if let Screen::Case(view) = &mut app.screen {
            view.tab = Tab::Settings;
        }
        let mut ready = false;
        for _ in 0..200 {
            frame(&mut app, &ctx);
            if let Screen::Case(view) = &app.screen
                && view.form.is_some()
            {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready, "the form never appeared");
        let Screen::Case(view) = &mut app.screen else {
            panic!("the case closed");
        };
        let form = view.form.as_mut().unwrap();
        assert_eq!(form.end_time, "10");
        form.end_time = "20".into();
        // What the Save button does, without a pointer.
        let settings = view.form.as_ref().unwrap().parse().unwrap();
        controldict::save(&view.dir, &settings).unwrap();
        let text = std::fs::read_to_string(case.path().join("system/controlDict")).unwrap();
        assert!(text.contains("endTime 20;"), "{text}");
        assert!(text.contains("deltaT 0.5;"), "{text}");
    }

    #[test]
    fn the_boundary_conditions_of_a_field_are_shown_and_saved() {
        let case = case_dir();
        std::fs::create_dir_all(case.path().join("0")).unwrap();
        std::fs::write(
            case.path().join("0/T"),
            "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
             dimensions [0 0 0 1 0 0 0];\ninternalField uniform 300;\n\
             boundaryField\n{\n    inlet\n    {\n        type fixedValue;\n\
             \x20       value uniform 300;\n    }\n}\n",
        )
        .unwrap();
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        app.show_tab(TabName::Conditions);
        let mut ready = false;
        for _ in 0..300 {
            frame(&mut app, &ctx);
            if let Screen::Case(view) = &app.screen
                && view.conditions.is_some()
            {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready, "the field was never read");
        let Screen::Case(view) = &mut app.screen else {
            panic!("the case closed");
        };
        let loaded = view.conditions.as_mut().unwrap();
        assert_eq!(loaded.fields, ["T"]);
        let field = loaded.current.as_mut().unwrap();
        field.patches[0].items[1].text = "uniform 350".into();
        conditions::save(&view.dir, "0", field).unwrap();
        let text = std::fs::read_to_string(case.path().join("0/T")).unwrap();
        assert!(text.contains("value uniform 350;"), "{text}");
        assert!(text.contains("internalField uniform 300;"), "{text}");
    }

    #[test]
    fn the_solver_numbers_of_a_case_are_shown_and_saved() {
        let case = case_dir();
        std::fs::write(
            case.path().join("system/fvSolution"),
            "solvers\n{\n    p\n    {\n        solver PCG;\n        relTol 0.01;\n    }\n}\n",
        )
        .unwrap();
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        app.show_tab(TabName::Numerics);
        frame(&mut app, &ctx);
        let Screen::Case(view) = &mut app.screen else {
            panic!("the case closed");
        };
        let groups = view.numerics.as_mut().unwrap().as_mut().unwrap();
        assert_eq!(groups[0].title(), "solvers / p");
        groups[0].items[1].text = "0.1".into();
        numerics::save(&view.dir, groups).unwrap();
        let text = std::fs::read_to_string(case.path().join("system/fvSolution")).unwrap();
        assert!(
            text.contains("relTol 0.1;") && text.contains("solver PCG;"),
            "{text}"
        );
    }

    #[test]
    fn the_mesh_of_a_case_is_read_and_drawn() {
        let case = case_dir();
        let from = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vortia-foamfile/tests/data/cavity-ascii/constant/polyMesh");
        let to = case.path().join("constant/polyMesh");
        std::fs::create_dir_all(&to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        app.show_mesh();
        let mut ready = false;
        for _ in 0..200 {
            frame(&mut app, &ctx);
            if let Screen::Case(view) = &app.screen
                && let Some(viewer) = &view.viewer
            {
                let viewer = viewer.as_ref().unwrap();
                assert!(viewer.surface().triangle_count() > 0);
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready, "the mesh was not read");
        // And it draws once it is there.
        frame(&mut app, &ctx);
    }

    #[test]
    fn the_mesh_is_coloured_with_a_field_of_the_case() {
        let case = case_dir();
        let data = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vortia-foamfile/tests/data/cavity-ascii");
        let to = case.path().join("constant/polyMesh");
        std::fs::create_dir_all(&to).unwrap();
        for entry in std::fs::read_dir(data.join("constant/polyMesh")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
        std::fs::create_dir_all(case.path().join("0.01")).unwrap();
        for name in ["p", "U", "phi"] {
            std::fs::copy(
                data.join("0.01").join(name),
                case.path().join("0.01").join(name),
            )
            .unwrap();
        }
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        app.show_mesh_field("p");
        let mut seen = None;
        for _ in 0..300 {
            frame(&mut app, &ctx);
            if let Screen::Case(view) = &app.screen
                && let Some(Ok(viewer)) = &view.viewer
                && let Some(values) = viewer.values()
            {
                seen = Some(values.clone());
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let values = seen.expect("the field was not read").unwrap();
        assert_eq!((values.name.as_str(), values.time.as_str()), ("p", "0.01"));
        assert_eq!(values.kind, "scalar");
        assert_eq!(values.values.len(), 12);
        frame(&mut app, &ctx);
    }

    #[test]
    fn a_case_without_a_mesh_says_so_in_its_mesh_tab() {
        let case = case_dir();
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        app.show_mesh();
        for _ in 0..200 {
            frame(&mut app, &ctx);
            if let Screen::Case(view) = &app.screen
                && let Some(viewer) = &view.viewer
            {
                assert!(viewer.is_err());
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no answer about the mesh");
    }

    #[test]
    fn every_screen_and_dialog_draws_in_both_themes() {
        let case = case_dir();
        for theme in [Theme::Light, Theme::Dark, Theme::System] {
            let ctx = Context::default();
            let mut app = VortiaApp::with_prefs(
                Prefs {
                    theme,
                    ..Prefs::default()
                },
                None,
            );
            frame(&mut app, &ctx);
            for dialog in [Dialog::Preferences, Dialog::Shortcuts, Dialog::About] {
                app.dialog = Some(dialog);
                frame(&mut app, &ctx);
            }
            app.dialog = None;
            app.open_case(case.path()).unwrap();
            frame(&mut app, &ctx);
            if let Screen::Case(view) = &mut app.screen {
                view.report = Convergence::parse(
                    r#"{"status":"stabilizing","automatic":true,"stableSamples":1,"requiredSamples":5,"globalMass":1e-4}"#,
                );
                view.log.push_back("Time = 1".into());
                view.outcome = Some(Outcome::Failed("boom".into()));
            }
            frame(&mut app, &ctx);
        }
    }

    #[test]
    fn a_run_that_cannot_start_the_solver_says_so_in_the_case() {
        let case = case_dir();
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        app.start_run(&ctx);
        let start = std::time::Instant::now();
        loop {
            frame(&mut app, &ctx);
            let Screen::Case(view) = &app.screen else {
                panic!("left the case");
            };
            if let Some(outcome) = &view.outcome {
                assert!(
                    matches!(outcome, Outcome::Failed(_)),
                    "the solver is not set up: {outcome:?}"
                );
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(60), "no outcome");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn the_case_folder_is_read_for_the_screen_and_again_after_a_run() {
        let case = case_dir();
        let ctx = Context::default();
        let mut app = VortiaApp::with_prefs(Prefs::default(), None);
        app.open_case(case.path()).unwrap();
        let start = std::time::Instant::now();
        let wait_for_summary = |app: &mut VortiaApp| loop {
            frame(app, &ctx);
            if let Screen::Case(view) = &app.screen
                && view.summary.is_some()
            {
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "no summary");
            std::thread::sleep(Duration::from_millis(10));
        };
        wait_for_summary(&mut app);
        let Screen::Case(view) = &mut app.screen else {
            panic!("left the case");
        };
        assert_eq!(
            view.summary.as_ref().and_then(|s| s.application.as_deref()),
            Some("vortiaCentralFoam")
        );
        // A run that ends marks the folder as changed; the next frame reads it again.
        view.summary = None;
        view.summary_stale = true;
        wait_for_summary(&mut app);
    }

    #[test]
    fn counts_are_grouped_by_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(7500), "7\u{202f}500");
        assert_eq!(grouped(1_234_567), "1\u{202f}234\u{202f}567");
    }

    #[test]
    fn statuses_and_numbers_read_well() {
        assert_eq!(status_label("converged"), ("Converged", Tone::Success));
        assert_eq!(status_label("invalid").1, Tone::Danger);
        assert_eq!(status_label("something new").1, Tone::Info);
        assert_eq!(scientific(None), "–");
        assert_eq!(scientific(Some(0.00123)), "1.23e-3");
    }

    #[test]
    fn the_log_keeps_only_its_last_lines() {
        let mut view = CaseView::new(PathBuf::from("/c"), None);

        for n in 0..MAX_LOG + 10 {
            view.push_log(n.to_string());
        }
        assert_eq!(view.log.len(), MAX_LOG);
        assert_eq!(view.log.front().map(String::as_str), Some("10"));
        assert_eq!(view.name(), "c");
    }
}
