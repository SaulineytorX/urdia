//! Fixed diagnostic vocabulary. Provider reports never grant execution authority.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFailureKind {
    Authentication,
    AccessDenied,
    RateOrQuota,
    ModelUnavailable,
    Connection,
    Timeout,
    ContextLimit,
    InvalidResponse,
    Unknown,
}

impl ProviderFailureKind {
    /// Only static host text may be exposed or persisted, never native diagnostics.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Authentication => {
                "The provider reported an authentication problem. Check this agent's sign-in or API settings before sending another task."
            }
            Self::AccessDenied => {
                "The provider reported that access was denied. Check the account's access to the selected service or model."
            }
            Self::RateOrQuota => {
                "The provider reported a usage or rate limit. Check its limits before choosing whether to send another task."
            }
            Self::ModelUnavailable => {
                "The provider reported that the selected model is unavailable. Review the agent's model settings and account access."
            }
            Self::Connection => {
                "The provider connection failed. Check the connection and service availability before sending another task."
            }
            Self::Timeout => {
                "The agent operation timed out. Review its status before choosing whether to send another task."
            }
            Self::ContextLimit => {
                "The provider reported a context limit. Reduce the task or supplied context before sending another task."
            }
            Self::InvalidResponse => {
                "The agent returned an invalid protocol response. Check the supported agent version before sending another task."
            }
            Self::Unknown => {
                "The provider turn failed. Its cause is unknown; private diagnostics remain hidden."
            }
        }
    }
}

impl std::fmt::Display for ProviderFailureKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}
