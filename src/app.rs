use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;

use crate::{
    message::Message,
    model::{HostInfo, InstallContext, LogEntry, LogLevel, SecurityChoice, Step, StepState},
};

const SPINNER: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

pub enum Effect {
    None,
    SpawnStep(Step),
    SpawnParallel(Vec<Step>),
    RunCleanup,
    Quit,
}

pub struct App {
    pub host: HostInfo,
    pub step_states: HashMap<Step, StepState>,
    pub step_logs: HashMap<Step, Vec<LogEntry>>,
    pub current_step: Option<Step>,
    pub context: InstallContext,
    pub input_buffer: String,
    pub should_quit: bool,
    pub abort_requested: bool,
    pub elapsed: Duration,
    pub start_time: Instant,
    pub security_selection: usize,
    pub security_prompt_at: Option<Instant>,
    pub spinner_tick: usize,
    pub done: bool,
    pub unsupported_os: bool,
    pub verbose: bool,
    pub no_cleanup: bool,
    pub fullscreen: bool,
    pub scroll_offset: Option<usize>, // None = auto-scroll to bottom
    pub max_scroll: usize,            // set by view each frame
    pub show_help: bool,

    deps_done: bool,
    network_done: bool,
    tx: mpsc::Sender<Message>,
}

impl App {
    pub fn new(context: InstallContext, tx: mpsc::Sender<Message>) -> Self {
        let mut step_states = HashMap::new();
        for &step in Step::FLOW {
            step_states.insert(step, StepState::Pending);
        }
        step_states.insert(Step::Preflight, StepState::Pending);
        step_states.insert(Step::Network, StepState::Pending);

        Self {
            host: HostInfo::default(),
            step_states,
            step_logs: HashMap::new(),
            current_step: None,
            context,
            input_buffer: String::new(),
            should_quit: false,
            abort_requested: false,
            elapsed: Duration::ZERO,
            start_time: Instant::now(),
            security_selection: 0,
            security_prompt_at: None,
            spinner_tick: 0,
            done: false,
            unsupported_os: false,
            verbose: false,
            no_cleanup: false,
            fullscreen: false,
            scroll_offset: None,
            max_scroll: 0,
            show_help: false,
            deps_done: false,
            network_done: false,
            tx,
        }
    }

    pub fn step_state(&self, step: Step) -> &StepState {
        self.step_states.get(&step).unwrap_or(&StepState::Pending)
    }

    pub fn step_logs(&self, step: Step) -> &[LogEntry] {
        self.step_logs.get(&step).map_or(&[], |v| v.as_slice())
    }

    pub fn security_countdown(&self) -> u64 {
        self.security_prompt_at
            .map(|at| 60u64.saturating_sub(at.elapsed().as_secs()))
            .unwrap_or(60)
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let current = self.scroll_offset.unwrap_or(self.max_scroll) as i32;
        let new = (current + delta).clamp(0, self.max_scroll as i32) as usize;
        if new >= self.max_scroll {
            self.scroll_offset = None; // snap back to auto-follow
        } else {
            self.scroll_offset = Some(new);
        }
    }

    pub fn spinner_char(&self) -> char {
        SPINNER[self.spinner_tick % SPINNER.len()]
    }

    pub fn log_step(&mut self, step: Step, level: LogLevel, text: impl Into<String>) {
        let logs = self.step_logs.entry(step).or_default();
        logs.push(LogEntry {
            level,
            text: text.into(),
        });
        // Keep bounded, only the tail is rendered
        const MAX_LOGS_PER_STEP: usize = 50;
        if logs.len() > MAX_LOGS_PER_STEP {
            logs.drain(..logs.len() - MAX_LOGS_PER_STEP);
        }
    }

    // Is the license input phase active?
    pub fn license_input_active(&self) -> bool {
        !self.unsupported_os
            && self.context.license_key.is_empty()
            && matches!(
                self.step_state(Step::License),
                StepState::Running | StepState::Pending
            )
    }

    // Is the security choice active?
    pub fn security_input_active(&self) -> bool {
        *self.step_state(Step::Security) == StepState::Running
            && self.context.security_choice == SecurityChoice::NotAskedYet
    }

    pub fn update(&mut self, msg: Message) -> Effect {
        match msg {
            Message::Tick => {
                self.elapsed = self.start_time.elapsed();
                self.spinner_tick = self.spinner_tick.wrapping_add(1);

                // Auto-submit security choice after 60s
                if self.security_input_active()
                    && let Some(at) = self.security_prompt_at
                    && at.elapsed().as_secs() >= 60
                {
                    return self.submit_security_choice();
                }

                Effect::None
            }

            Message::Key(key) => self.handle_key(key.code, key.modifiers),
            Message::AdvanceStep => self.advance(),

            Message::StepStarted(step) => {
                self.step_states.insert(step, StepState::Running);
                if step != Step::Network {
                    self.current_step = Some(step);
                }
                Effect::None
            }

            Message::StepLog(step, line) => {
                let (level, text) = parse_log_line(&line);
                self.log_step(step, level, text);
                Effect::None
            }

            Message::WaitingForInput(step) => {
                if step == Step::Security {
                    self.security_prompt_at = Some(Instant::now());
                }
                Effect::None
            }

            Message::StepCompleted(step) => self.handle_step_completed(step),

            Message::StepFailed(step, err) => {
                self.step_states
                    .insert(step, StepState::Failed(err.clone()));
                self.log_step(step, LogLevel::Error, format!("FAILED: {err}"));

                if step == Step::Network {
                    self.step_states
                        .insert(Step::Dependencies, StepState::Failed(err));
                }

                if self.no_cleanup {
                    self.should_quit = true;
                    Effect::None
                } else {
                    Effect::RunCleanup
                }
            }

            Message::HostDetected(info, use_sudo) => {
                self.context.arch = info.arch.clone();
                self.context.use_sudo = use_sudo;
                self.context.server_address = info.public_ip.clone();
                if !self.context.dry_run && info.os != "linux" {
                    self.unsupported_os = true;
                }
                self.host = info;
                Effect::None
            }

            Message::LicenseKeySet(key) => {
                self.context.license_key = key;
                Effect::None
            }

            Message::DomainResolved(domain) => {
                self.context.domain = Some(domain);
                Effect::None
            }

            Message::KeysGenerated {
                health_token,
                lk_api_key,
                lk_api_secret,
            } => {
                self.context.health_token = health_token;
                self.context.lk_api_key = lk_api_key;
                self.context.lk_api_secret = lk_api_secret;
                Effect::None
            }

            Message::UidResolved(uid, gid) => {
                self.context.orda_uid = uid;
                self.context.orda_gid = gid;
                Effect::None
            }

            Message::SshPortDetected(port) => {
                self.context.ssh_port = port;
                Effect::None
            }

            Message::Abort => {
                if self.done {
                    return Effect::Quit;
                }
                if self.abort_requested || self.no_cleanup {
                    return Effect::Quit;
                }
                self.abort_requested = true;
                Effect::RunCleanup
            }

            Message::CleanupComplete => {
                self.should_quit = true;
                Effect::None
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Effect {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return self.update(Message::Abort);
        }

        if self.should_quit && (code == KeyCode::Char('q') || code == KeyCode::Esc) {
            return Effect::Quit;
        }

        if (self.done || self.unsupported_os)
            && (code == KeyCode::Char('q') || code == KeyCode::Esc)
        {
            return Effect::Quit;
        }

        // Global toggles (Ctrl combos work in all states)
        if code == KeyCode::Char('f') && modifiers.contains(KeyModifiers::CONTROL) {
            self.fullscreen = !self.fullscreen;
            return Effect::None;
        }
        if code == KeyCode::Char('h') && modifiers.contains(KeyModifiers::CONTROL) {
            self.show_help = !self.show_help;
            return Effect::None;
        }
        if self.show_help {
            // Any key closes help
            self.show_help = false;
            return Effect::None;
        }

        if self.license_input_active() {
            return self.handle_license_key(code, modifiers);
        }

        if self.security_input_active() {
            return self.handle_security_key(code);
        }

        // Scroll (keyboard)
        let has_shift = modifiers.contains(KeyModifiers::SHIFT);
        match code {
            KeyCode::Up | KeyCode::Char('k') if has_shift => self.scroll_by(-10),
            KeyCode::Down | KeyCode::Char('j') if has_shift => self.scroll_by(10),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_by(1),
            KeyCode::PageUp => self.scroll_by(-10),
            KeyCode::PageDown => self.scroll_by(10),
            KeyCode::Char('G') => {
                self.scroll_offset = None;
            }
            KeyCode::Char('g') => {
                self.scroll_offset = Some(0);
            }
            _ => {}
        }

        Effect::None
    }

    fn handle_license_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Effect {
        let has_ctrl = modifiers.contains(KeyModifiers::CONTROL);
        let has_super = modifiers.contains(KeyModifiers::SUPER);
        let has_alt = modifiers.contains(KeyModifiers::ALT);

        match code {
            KeyCode::Backspace if has_super || has_ctrl => {
                self.input_buffer.clear();
                Effect::None
            }
            // Ctrl+U also maps here (macOS terminals send it for Cmd+Backspace)
            KeyCode::Char('u') if has_ctrl => {
                self.input_buffer.clear();
                Effect::None
            }
            KeyCode::Char('w') if has_ctrl => {
                delete_last_word(&mut self.input_buffer);
                Effect::None
            }
            KeyCode::Backspace if has_alt => {
                delete_last_word(&mut self.input_buffer);
                Effect::None
            }
            KeyCode::Char(_) if has_ctrl || has_super => Effect::None,
            KeyCode::Char(c) if !c.is_control() => {
                self.input_buffer.push(c);
                Effect::None
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                Effect::None
            }
            KeyCode::Enter => {
                let key = self.input_buffer.trim().to_string();
                if crate::system::validate_license_key(&key) {
                    self.input_buffer.clear();
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(Message::LicenseKeySet(key)).await;
                        let _ = tx.send(Message::StepCompleted(Step::License)).await;
                    });
                } else if !self.input_buffer.is_empty() {
                    self.log_step(Step::License, LogLevel::Error, "Invalid license key");
                }
                Effect::None
            }
            KeyCode::Esc => {
                self.input_buffer.clear();
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn handle_security_key(&mut self, code: KeyCode) -> Effect {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.security_selection = 0;
                self.security_prompt_at = Some(Instant::now());
                Effect::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.security_selection = 1;
                self.security_prompt_at = Some(Instant::now());
                Effect::None
            }
            KeyCode::Char('1') => {
                self.security_selection = 0;
                self.submit_security_choice()
            }
            KeyCode::Char('2') => {
                self.security_selection = 1;
                self.submit_security_choice()
            }
            KeyCode::Enter => self.submit_security_choice(),
            _ => Effect::None,
        }
    }

    fn submit_security_choice(&mut self) -> Effect {
        self.context.security_choice = if self.security_selection == 0 {
            SecurityChoice::InstallFirewall
        } else {
            SecurityChoice::Skip
        };
        // Re-spawn the security step so it can apply the choice
        Effect::SpawnStep(Step::Security)
    }

    fn advance(&mut self) -> Effect {
        let next = match self.current_step {
            None => Step::Preflight,
            Some(current) => match current.next() {
                Some(s) => s,
                None => return Effect::None,
            },
        };

        if next == Step::Dependencies {
            self.deps_done = false;
            self.network_done = false;
            return Effect::SpawnParallel(vec![Step::Dependencies, Step::Network]);
        }

        Effect::SpawnStep(next)
    }

    fn handle_step_completed(&mut self, step: Step) -> Effect {
        match step {
            Step::Dependencies => {
                self.step_states
                    .insert(Step::Dependencies, StepState::Success);
                self.deps_done = true;
                self.try_finish_deps_phase()
            }
            Step::Network => {
                self.step_states.insert(Step::Network, StepState::Success);
                self.network_done = true;
                self.try_finish_deps_phase()
            }
            Step::Complete => {
                self.step_states.insert(step, StepState::Success);
                self.done = true;
                self.should_quit = true;
                self.scroll_offset = None; // snap to bottom so complete section is visible
                Effect::None
            }
            _ => {
                self.step_states.insert(step, StepState::Success);
                self.advance()
            }
        }
    }

    fn try_finish_deps_phase(&mut self) -> Effect {
        if self.deps_done && self.network_done {
            self.step_states
                .insert(Step::Dependencies, StepState::Success);
            self.advance()
        } else {
            Effect::None
        }
    }
}

fn delete_last_word(buf: &mut String) {
    let trimmed_len = buf.trim_end().len();
    buf.truncate(trimmed_len);
    if let Some(pos) = buf.rfind(|c: char| c.is_whitespace()) {
        buf.truncate(pos + 1);
    } else {
        buf.clear();
    }
}

fn parse_log_line(line: &str) -> (LogLevel, String) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return (LogLevel::Dim, String::new());
    }
    if trimmed.starts_with("  $ ") || trimmed.starts_with("  [dry-run]") {
        (LogLevel::Command, trimmed.to_string())
    } else if trimmed.starts_with("FAILED:") {
        (LogLevel::Error, trimmed.to_string())
    } else if trimmed.contains("complete")
        || trimmed.contains("installed")
        || trimmed.contains("ready")
        || trimmed.contains("passed")
        || trimmed.contains("registered")
    {
        (LogLevel::Success, trimmed.to_string())
    } else if trimmed.starts_with("  ") {
        (LogLevel::Dim, trimmed.to_string())
    } else {
        (LogLevel::Info, trimmed.to_string())
    }
}
