/// Provider connector paths from poshanka config (`[provider]`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderSpec {
    /// Long-running feed script (`sh -c`).
    pub exec: Option<String>,
    /// One-shot CLI binary (`list`, `close`, `activate`, `input`, …).
    pub command: Option<String>,
    /// Optional socket path forwarded as `--socket` before subcommand args.
    pub socket: Option<String>,
}

impl ProviderSpec {
    pub fn has_feed(&self) -> bool {
        self.exec.is_some() || self.command.is_some()
    }
}

impl From<&crate::model::SubscriberSpec> for ProviderSpec {
    fn from(spec: &crate::model::SubscriberSpec) -> Self {
        Self {
            exec: spec.exec.clone(),
            command: spec.command.clone(),
            socket: spec.socket.clone(),
        }
    }
}
