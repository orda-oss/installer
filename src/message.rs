use crossterm::event::KeyEvent;

use crate::model::{HostInfo, SecurityChoice, Step};

pub enum Message {
    Key(KeyEvent),
    Tick,

    // Step lifecycle
    StepStarted(Step),
    StepLog(Step, String),
    StepCompleted(Step),
    StepFailed(Step, String),
    WaitingForInput(Step),

    // Context updates
    HostDetected(HostInfo, bool), // host info, use_sudo
    LicenseKeySet(String),
    DomainResolved(String),
    UidResolved(u32, u32),
    SecurityChosen(SecurityChoice),
    KeysGenerated {
        health_token: String,
        lk_api_key: String,
        lk_api_secret: String,
    },
    SshPortDetected(u16),

    // Control
    AdvanceStep,
    Abort,
    CleanupComplete,
}
