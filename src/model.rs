use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    Preflight,
    License,
    Register,
    Dependencies,
    Network,
    SystemSetup,
    Tls,
    Security,
    Configuration,
    Launch,
    Complete,
}

impl Step {
    // Steps shown in the flow (Preflight is silent, Network is a sub-step)
    pub const FLOW: &[Step] = &[
        Step::License,
        Step::Register,
        Step::Dependencies,
        Step::SystemSetup,
        Step::Tls,
        Step::Security,
        Step::Configuration,
        Step::Launch,
        Step::Complete,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Step::Preflight => "PREFLIGHT",
            Step::License => "LICENSE",
            Step::Register => "REGISTER",
            Step::Dependencies => "DEPENDENCIES",
            Step::Network => "NETWORK",
            Step::SystemSetup => "SYSTEM",
            Step::Tls => "TLS",
            Step::Security => "SECURITY",
            Step::Configuration => "CONFIG",
            Step::Launch => "LAUNCH",
            Step::Complete => "COMPLETE",
        }
    }

    pub fn next(&self) -> Option<Step> {
        match self {
            Step::Preflight => Some(Step::License),
            Step::License => Some(Step::Register),
            Step::Register => Some(Step::Dependencies),
            Step::Dependencies | Step::Network => Some(Step::SystemSetup),
            Step::SystemSetup => Some(Step::Tls),
            Step::Tls => Some(Step::Security),
            Step::Security => Some(Step::Configuration),
            Step::Configuration => Some(Step::Launch),
            Step::Launch => Some(Step::Complete),
            Step::Complete => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum StepState {
    Pending,
    Running,
    Success,
    Failed(String),
    Skipped(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SecurityChoice {
    NotAskedYet,
    InstallFirewall,
    Skip,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Command,
    Error,
    Dim,
}

#[derive(Clone, Debug, Default)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    pub hostname: String,
    pub public_ip: String,
    pub docker: bool,
    pub connectivity: bool,
}

#[derive(Clone, Debug)]
pub struct InstallContext {
    pub dry_run: bool,
    pub orda_dir: PathBuf,
    pub semerkant_url: String,
    pub image: String,
    pub license_key: String,
    pub arch: String,
    pub use_sudo: bool,
    pub domain: Option<String>,
    pub orda_uid: u32,
    pub orda_gid: u32,
    pub health_token: String,
    pub lk_api_key: String,
    pub lk_api_secret: String,
    pub ssh_port: u16,
    pub security_choice: SecurityChoice,
    pub server_address: String,
}

impl InstallContext {
    pub fn new(dry_run: bool, orda_dir: PathBuf, semerkant_url: String, image: String) -> Self {
        Self {
            dry_run,
            orda_dir,
            semerkant_url,
            image,
            license_key: String::new(),
            arch: String::new(),
            use_sudo: false,
            domain: None,
            orda_uid: 0,
            orda_gid: 0,
            health_token: String::new(),
            lk_api_key: String::new(),
            lk_api_secret: String::new(),
            ssh_port: 22,
            security_choice: SecurityChoice::NotAskedYet,
            server_address: String::new(),
        }
    }
}
