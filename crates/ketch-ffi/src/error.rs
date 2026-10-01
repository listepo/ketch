//! The one error type a foreign caller sees, and how the core's map onto it.
//!
//! The core's `Error` has two dozen variants shaped for a terminal: exit codes,
//! hints that name CLI flags. A front end branches on far fewer questions — is
//! another ketch running, did the person cancel, does the thing exist, is the
//! network down, did a download fail its check — so those are the variants, and
//! everything else is `Other` with the core's own wording.

use ketch_core::error::Error;

/// Why an operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum KetchError {
    /// Another operation holds the install tree's lock: a ketch process named
    /// by `pid`, or this process on another thread, or (when the lock file
    /// could not be read) nobody known. Retry once it finishes.
    #[error("another ketch process holds the lock")]
    Busy { pid: Option<u32> },
    /// The operation's `CancelToken` fired; nothing was left half-installed.
    #[error("cancelled")]
    Cancelled,
    /// No installed package, or no release, answers to `name`.
    #[error("`{name}` not found")]
    NotFound { name: String },
    /// A source could not be reached, or answered with an HTTP error.
    #[error("{message}")]
    Network { message: String },
    /// A download did not match its published or recorded checksum, or none
    /// was published where one was required.
    #[error("{message}")]
    Verification { message: String },
    /// Anything else, worded as the CLI would word it.
    #[error("{message}")]
    Other { message: String },
}

impl From<Error> for KetchError {
    fn from(error: Error) -> Self {
        match error {
            Error::Busy { pid, .. } => KetchError::Busy { pid },
            Error::Cancelled => KetchError::Cancelled,
            Error::NotInstalled(name) | Error::NoRelease(name) => KetchError::NotFound { name },
            Error::Http { .. } | Error::Network { .. } => KetchError::Network {
                message: describe(&error),
            },
            Error::ChecksumMismatch { .. } | Error::ChecksumMissing(_) => {
                KetchError::Verification {
                    message: describe(&error),
                }
            }
            other => KetchError::Other {
                message: describe(&other),
            },
        }
    }
}

/// The headline, the detail lines under it and the hint, one per line: what
/// the CLI prints for the same error, minus the colour.
///
/// Error text quotes a server's response body and a plugin's stderr, which
/// someone else wrote; it is filtered here, the one place it leaves the core,
/// so no front end has to remember to.
fn describe(error: &Error) -> String {
    let mut lines = vec![error.to_string()];
    lines.extend(error.details());
    lines.extend(error.hint());
    ketch_core::changelog::sanitize(&lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn a_held_lock_keeps_its_pid() {
        let error = Error::Busy {
            pid: Some(42),
            lock: PathBuf::from("/tmp/.lock"),
        };
        assert_eq!(KetchError::from(error), KetchError::Busy { pid: Some(42) });
    }

    #[test]
    fn cancellation_is_its_own_variant() {
        assert_eq!(KetchError::from(Error::Cancelled), KetchError::Cancelled);
    }

    #[test]
    fn a_missing_package_or_release_is_not_found() {
        assert_eq!(
            KetchError::from(Error::NotInstalled("rg".into())),
            KetchError::NotFound { name: "rg".into() }
        );
        assert_eq!(
            KetchError::from(Error::NoRelease("jq".into())),
            KetchError::NotFound { name: "jq".into() }
        );
    }

    #[test]
    fn http_errors_are_network_errors_with_detail_and_hint() {
        let error = Error::Http {
            url: "https://api.github.com/x".into(),
            status: 403,
            detail: Some("rate limited".into()),
        };
        let KetchError::Network { message } = KetchError::from(error) else {
            panic!("not a network error");
        };
        assert_eq!(
            message.lines().collect::<Vec<_>>(),
            vec![
                "HTTP 403 from https://api.github.com/x",
                "rate limited",
                "GitHub rate limit. Set GITHUB_TOKEN (or `gh auth token`) to raise it.",
            ]
        );
    }

    #[test]
    fn a_checksum_mismatch_is_a_verification_error() {
        let error = Error::ChecksumMismatch {
            name: "rg.tar.gz".into(),
            expected: "aa".into(),
            actual: "bb".into(),
        };
        let KetchError::Verification { message } = KetchError::from(error) else {
            panic!("not a verification error");
        };
        assert!(message.starts_with("checksum mismatch for rg.tar.gz"));
        assert!(message.contains("expected aa"));
    }

    #[test]
    fn everything_else_keeps_the_core_wording_without_control_characters() {
        let error = Error::msg("bad \u{1b}[2Jthing\u{202e}");
        assert_eq!(
            KetchError::from(error),
            KetchError::Other {
                message: "bad [2Jthing".into()
            }
        );
    }
}
