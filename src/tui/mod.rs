//! Optional interactive terminal rendering.
//!
//! This module owns the terminal session, event reducer, and renderer so the
//! install pipeline can remain a normal, terminal-agnostic command pipeline.

use crate::ui::{ProgressSink, ProgressStage};
use crossterm::cursor::Show;
use crossterm::event::{self, Event as InputEvent, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::collections::VecDeque;
use std::io::{self, IsTerminal};
use std::panic::{self, PanicHookInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const EVENT_HISTORY: usize = 8;

type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>;

fn stage_label(stage: ProgressStage) -> &'static str {
    match stage {
        ProgressStage::Resolving => "Resolving release",
        ProgressStage::Downloading => "Downloading",
        ProgressStage::Verifying => "Verifying checksum",
        ProgressStage::Extracting => "Extracting archive",
        ProgressStage::Trusting => "Checking trust",
        ProgressStage::Installing => "Installing",
    }
}

/// A package's lifecycle state in the queue pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageStatus {
    /// Work has not started.
    Pending,
    /// Work is in progress.
    Active,
    /// Work completed successfully.
    Succeeded,
    /// Work ended with an error.
    Failed,
}

impl PackageStatus {
    fn symbol(self) -> &'static str {
        match self {
            PackageStatus::Pending => "·",
            PackageStatus::Active => "⟳",
            PackageStatus::Succeeded => "✓",
            PackageStatus::Failed => "!",
        }
    }
}

/// A typed update emitted by command work; rendering never reaches into core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A pipeline stage began for one package.
    Stage {
        package: String,
        stage: ProgressStage,
    },
    /// A download reported its total byte length, when the source provided it.
    DownloadStarted { package: String, total: Option<u64> },
    /// A download advanced by this many bytes.
    DownloadAdvanced { package: String, delta: u64 },
    /// A download finished and moved on to verification.
    DownloadFinished { package: String },
    /// A package succeeded or failed.
    Completed { package: String, success: bool },
    /// A line of useful activity for the scrollback pane.
    Message(String),
    /// Toggle the compact keyboard-help view.
    ToggleHelp,
    /// Leave the TUI and continue in the normal line-oriented mode.
    Leave,
}

#[derive(Debug, Clone)]
struct Package {
    name: String,
    status: PackageStatus,
    stage: Option<ProgressStage>,
    downloaded: u64,
    total: Option<u64>,
}

/// Reducer state. It has no terminal handle, so transitions are unit-testable.
#[derive(Debug, Clone)]
pub struct State {
    command: String,
    packages: Vec<Package>,
    activity: VecDeque<String>,
    started: Instant,
    show_help: bool,
    leave_requested: bool,
}

impl State {
    /// Build state for one command and its initially known work queue.
    pub fn new(command: impl Into<String>, packages: impl IntoIterator<Item = String>) -> Self {
        State {
            command: command.into(),
            packages: packages
                .into_iter()
                .map(|name| Package {
                    name,
                    status: PackageStatus::Pending,
                    stage: None,
                    downloaded: 0,
                    total: None,
                })
                .collect(),
            activity: VecDeque::new(),
            started: Instant::now(),
            show_help: false,
            leave_requested: false,
        }
    }

    /// Apply one event. This is the sole mutation point for TUI state.
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Stage { package, stage } => {
                let name = {
                    let item = self.package_mut(&package);
                    item.status = PackageStatus::Active;
                    item.stage = Some(stage);
                    item.name.clone()
                };
                self.push_activity(format!("{name}: {}", stage_label(stage)));
            }
            Event::DownloadStarted { package, total } => {
                let item = self.package_mut(&package);
                item.status = PackageStatus::Active;
                item.stage = Some(ProgressStage::Downloading);
                item.downloaded = 0;
                item.total = total;
            }
            Event::DownloadAdvanced { package, delta } => {
                let item = self.package_mut(&package);
                item.downloaded = item.downloaded.saturating_add(delta);
            }
            Event::DownloadFinished { package } => {
                let item = self.package_mut(&package);
                item.stage = Some(ProgressStage::Verifying);
            }
            Event::Completed { package, success } => {
                let name = {
                    let item = self.package_mut(&package);
                    item.status = if success {
                        PackageStatus::Succeeded
                    } else {
                        PackageStatus::Failed
                    };
                    item.stage = None;
                    item.name.clone()
                };
                self.push_activity(format!(
                    "{} {}",
                    if success { "Installed" } else { "Failed" },
                    name
                ));
            }
            Event::Message(line) => self.push_activity(line),
            Event::ToggleHelp => self.show_help = !self.show_help,
            Event::Leave => self.leave_requested = true,
        }
    }

    fn package_mut(&mut self, name: &str) -> &mut Package {
        if let Some(index) = self.packages.iter().position(|item| item.name == name) {
            return &mut self.packages[index];
        }
        self.packages.push(Package {
            name: name.to_string(),
            status: PackageStatus::Pending,
            stage: None,
            downloaded: 0,
            total: None,
        });
        // A value was just appended, so this index is always valid.
        let last = self.packages.len() - 1;
        &mut self.packages[last]
    }

    fn push_activity(&mut self, line: String) {
        if self.activity.len() == EVENT_HISTORY {
            self.activity.pop_front();
        }
        self.activity.push_back(line);
    }

    fn counts(&self) -> (usize, usize, usize) {
        let succeeded = self
            .packages
            .iter()
            .filter(|item| item.status == PackageStatus::Succeeded)
            .count();
        let failed = self
            .packages
            .iter()
            .filter(|item| item.status == PackageStatus::Failed)
            .count();
        (succeeded, failed, self.packages.len())
    }
}

/// A live terminal controller shared by progress callbacks from worker threads.
pub struct Controller {
    state: Mutex<State>,
    terminal: Mutex<Option<Terminal<CrosstermBackend<io::Stderr>>>>,
    active: AtomicBool,
}

impl Controller {
    fn new(state: State, terminal: Terminal<CrosstermBackend<io::Stderr>>) -> Self {
        Controller {
            state: Mutex::new(state),
            terminal: Mutex::new(Some(terminal)),
            active: AtomicBool::new(true),
        }
    }

    /// Apply an event and redraw. Rendering failures never break an install.
    pub fn send(&self, event: Event) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let (leave, interrupted) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.apply(event);
            let interrupted = self.handle_input(&mut state);
            (state.leave_requested, interrupted)
        };
        if leave {
            self.shutdown();
        }
        if interrupted {
            // Raw mode turns Ctrl-C into a key event rather than a signal.
            // Restore first, then preserve the conventional interrupt status.
            std::process::exit(130);
        }
        if !leave {
            self.draw();
        }
    }

    fn handle_input(&self, state: &mut State) -> bool {
        let mut interrupted = false;
        // Poll instead of blocking so a download worker never waits on input.
        while event::poll(Duration::ZERO).unwrap_or(false) {
            let Ok(InputEvent::Key(key)) = event::read() else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    state.apply(Event::Leave);
                    interrupted = true;
                }
                KeyCode::Char('?') => state.apply(Event::ToggleHelp),
                KeyCode::Char('q') | KeyCode::Esc => state.apply(Event::Leave),
                _ => {}
            }
        }
        interrupted
    }

    fn draw(&self) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut terminal = self.terminal.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(terminal) = terminal.as_mut() {
            let _ = terminal.draw(|frame| render(frame, &state));
        }
    }

    /// Restore the terminal now. It is idempotent so every exit path can call it.
    pub fn shutdown(&self) {
        if !self.active.swap(false, Ordering::AcqRel) {
            return;
        }
        // `q` and `Esc` leave the session alive so the command can keep
        // running. Disconnect first, otherwise later status lines disappear
        // into an inactive renderer instead of returning to the line UI.
        crate::ui::disable_tui();
        let terminal = self
            .terminal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(mut terminal) = terminal {
            let _ = terminal.show_cursor();
            let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
    }

    /// Restore using a fresh stderr handle, which is safe from a panic hook
    /// even if the renderer was interrupted while holding its mutex.
    fn emergency_restore(&self) {
        self.active.store(false, Ordering::Release);
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), Show, LeaveAlternateScreen);
    }
}

/// A progress sink that turns byte callbacks into reducer events.
pub struct TuiProgress {
    controller: Arc<Controller>,
    package: String,
}

impl TuiProgress {
    /// Build a sink for one package label.
    pub fn new(controller: Arc<Controller>, package: impl Into<String>) -> Self {
        TuiProgress {
            controller,
            package: package.into(),
        }
    }
}

impl ProgressSink for TuiProgress {
    fn start(&self, total: Option<u64>, _label: &str) {
        self.controller.send(Event::DownloadStarted {
            package: self.package.clone(),
            total,
        });
    }

    fn advance(&self, delta: u64) {
        self.controller.send(Event::DownloadAdvanced {
            package: self.package.clone(),
            delta,
        });
    }

    fn finish(&self, _message: &str) {
        self.controller.send(Event::DownloadFinished {
            package: self.package.clone(),
        });
    }
}

/// Owns an active terminal session and unregisters it before normal output resumes.
pub struct Session {
    controller: Arc<Controller>,
    previous_panic_hook: Arc<Mutex<Option<PanicHook>>>,
}

impl Session {
    /// Enter raw mode and the alternate screen for an explicitly requested TUI.
    pub fn start(command: &str, packages: Vec<String>) -> io::Result<Session> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stderr(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        let backend = CrosstermBackend::new(io::stderr());
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = execute!(io::stderr(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(error);
            }
        };
        let controller = Arc::new(Controller::new(State::new(command, packages), terminal));
        controller.draw();
        let hook_controller = Arc::clone(&controller);
        let previous_panic_hook = Arc::new(Mutex::new(Some(panic::take_hook())));
        let hook_previous = Arc::clone(&previous_panic_hook);
        panic::set_hook(Box::new(move |info| {
            hook_controller.emergency_restore();
            if let Some(previous) = hook_previous
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                previous(info);
            }
        }));
        Ok(Session {
            controller,
            previous_panic_hook,
        })
    }

    /// Expose the controller to the line UI's progress/reporting seam.
    pub fn controller(&self) -> Arc<Controller> {
        Arc::clone(&self.controller)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        crate::ui::disable_tui();
        self.controller.shutdown();
        // Panic hooks are process-global, so remove ours and put back the
        // handler that was installed before this short-lived session began.
        let _ = panic::take_hook();
        if let Some(previous) = self
            .previous_panic_hook
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            panic::set_hook(previous);
        }
    }
}

/// Whether this process can safely activate a full-screen TUI.
pub fn can_start(quiet: bool) -> bool {
    !quiet
        && io::stderr().is_terminal()
        && std::env::var_os("CI").is_none()
        && std::env::var("TERM")
            .map(|term| term != "dumb")
            .unwrap_or(true)
}

fn render(frame: &mut Frame<'_>, state: &State) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("ketch TUI needs at least 30×8 terminal cells"),
            area,
        );
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);
    render_header(frame, rows[0], state);
    render_body(frame, rows[1], state);
    render_footer(frame, rows[2], state);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let (done, failed, total) = state.counts();
    let title = format!(
        " ketch · {} · {}/{} packages ",
        state.command,
        done + failed,
        total
    );
    frame.render_widget(Block::default().borders(Borders::ALL).title(title), area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, state: &State) {
    if state.show_help {
        frame.render_widget(
            Paragraph::new("q / Esc  return to line output\n?        hide this help")
                .block(Block::default().borders(Borders::ALL).title(" Help "))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    let columns = if area.width >= 62 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
            .split(area)
    };
    let queue: Vec<ListItem<'_>> = state
        .packages
        .iter()
        .map(|item| {
            let stage = item.stage.map(stage_label).unwrap_or("");
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ", item.status.symbol()),
                    status_style(item.status),
                ),
                Span::raw(format!("{} {}", item.name, stage)),
            ]))
        })
        .collect();
    frame.render_widget(
        List::new(queue).block(Block::default().borders(Borders::ALL).title(" Queue ")),
        columns[0],
    );

    let active = state
        .packages
        .iter()
        .find(|item| item.status == PackageStatus::Active);
    let mut lines = Vec::new();
    if let Some(item) = active {
        if let Some(total) = item.total.filter(|total| *total > 0) {
            let ratio = item.downloaded as f64 / total as f64;
            lines.push(Line::from(format!(
                "{}  {:.0}%  {} / {}",
                item.stage.map(stage_label).unwrap_or("Working"),
                ratio * 100.0,
                crate::ui::bytes(item.downloaded),
                crate::ui::bytes(total)
            )));
        } else {
            lines.push(Line::from(format!(
                "{} {}",
                item.stage.map(stage_label).unwrap_or("Working"),
                item.name
            )));
        }
    }
    for line in &state.activity {
        lines.push(Line::from(line.clone()));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Activity "))
            .wrap(Wrap { trim: true }),
        columns[1],
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let (succeeded, failed, _) = state.counts();
    let elapsed = state.started.elapsed().as_secs();
    let text = format!(
        " {succeeded} succeeded · {failed} failed · {}m {:02}s                         q line UI  ? help ",
        elapsed / 60,
        elapsed % 60
    );
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn status_style(status: PackageStatus) -> Style {
    if !crate::ui::color_enabled() {
        return Style::default();
    }
    match status {
        PackageStatus::Pending => Style::default().fg(Color::DarkGray),
        PackageStatus::Active => Style::default().fg(Color::Cyan),
        PackageStatus::Succeeded => Style::default().fg(Color::Green),
        PackageStatus::Failed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn reducer_tracks_downloads_and_completion() {
        let mut state = State::new("install", ["ripgrep".to_string()]);
        state.apply(Event::Stage {
            package: "ripgrep".to_string(),
            stage: ProgressStage::Resolving,
        });
        state.apply(Event::DownloadStarted {
            package: "ripgrep".to_string(),
            total: Some(100),
        });
        state.apply(Event::DownloadAdvanced {
            package: "ripgrep".to_string(),
            delta: 80,
        });
        state.apply(Event::Completed {
            package: "ripgrep".to_string(),
            success: true,
        });

        let item = &state.packages[0];
        assert_eq!(item.status, PackageStatus::Succeeded);
        assert_eq!(item.downloaded, 80);
        assert_eq!(state.counts(), (1, 0, 1));

        state.apply(Event::Leave);
        assert!(state.leave_requested);
    }

    #[test]
    fn renderer_projects_state_without_a_real_terminal() {
        let mut state = State::new("install", ["ripgrep".to_string()]);
        state.apply(Event::Stage {
            package: "ripgrep".to_string(),
            stage: ProgressStage::Downloading,
        });
        let backend = TestBackend::new(80, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(text.contains("ketch · install"), "{text}");
    }
}
