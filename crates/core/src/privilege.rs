//! Who this process is acting as.
//!
//! This exists because one of the trust kernel's guarantees is conditional on
//! it, and a conditional guarantee that nothing enforces is not a guarantee.
//!
//! [`Sink::delete`](crate::executor::Sink::delete) is files-only *by
//! construction*: `authorize` refuses a directory target, and because that
//! check and the removal cannot be made atomic, `remove_file` is called
//! unconditionally so that a directory swapped onto the name in between is
//! refused by the operating system rather than removed. That backstop is
//! `unlink(2)` returning `EPERM` for a directory — which `unlink(2)` documents
//! as applying when the effective user is **not** the super-user.
//!
//! So under `sudo` the backstop is simply absent.
//!
//! ## Why the whole run is refused, and not only the removal
//!
//! Refusing at the point of removal would be the narrowest fix and the wrong
//! one, because the super-user does two other things to this tool that no
//! per-path check can undo:
//!
//! - **It poisons the audit log.** `~/Library/Application Support/swept/`
//!   and the JSONL inside it are created by whoever writes them first. One
//!   `sudo swept clean` leaves both owned by root, and every subsequent
//!   ordinary run fails to append — and this project aborts a run it cannot
//!   record. A single privileged run can therefore disable the tool for the
//!   user who owns the files, in a way that looks like a bug rather than a
//!   consequence.
//! - **It widens the disposal scope silently.** Every location the allowlist
//!   names is inside one user's home. Running as root does not add anything
//!   the tool asks for; it only removes the file-permission floor that has been
//!   quietly underwriting every scope decision above it.
//!
//! Nothing this tool does needs the super-user. Caches, logs, derived data and
//! the user Trash all belong to the person running it, so refusing outright
//! costs no functionality — which is what makes a flat refusal affordable
//! rather than a trade.
//!
//! ## The one `unsafe`
//!
//! `geteuid(2)` takes no arguments, cannot fail, sets no `errno`, and returns
//! an integer. It is the smallest possible FFI surface, and the alternative —
//! inferring the effective user from the ownership of a file we create, or
//! shelling out to `id -u` — would be both slower and less trustworthy than
//! the syscall it is imitating.
//!
//! **What it costs, stated accurately** because the first draft of this comment
//! got it wrong and the `unsafe` rests on it: `libc` was a *dev-only*
//! dependency here, reached through `tempfile` and `filetime`. `trash` does not
//! depend on it at all. So this promotes `libc` into the shipped binary's
//! runtime graph for the first time, for one call. Checked with
//! `cargo tree -p swept-core -i libc -e normal`, which printed nothing before
//! this module existed.

/// The super-user's effective id.
///
/// Named rather than written as a literal `0` at the comparison, because the
/// number is the whole of the policy and should be greppable.
pub const SUPER_USER: u32 = 0;

/// The effective user id of this process.
///
/// Effective rather than real, deliberately: `sudo` sets both, but a setuid
/// binary sets only the effective one, and the effective id is what the kernel
/// consults when it decides whether `unlink(2)` may remove a directory entry.
pub fn effective_uid() -> u32 {
    // SAFETY: `geteuid` takes no arguments, touches no memory the caller owns,
    // is documented as always succeeding, and returns a plain integer.
    unsafe { libc::geteuid() }
}

/// Why a run under this effective user must be refused, if it must be.
///
/// Pure, so the policy can be tested without the test process becoming root —
/// which it cannot do, and which is exactly why the decision is separated from
/// the reading of it.
pub fn refusal(euid: u32) -> Option<&'static str> {
    if euid == SUPER_USER {
        Some(
            "refusing to run as the super-user: the directory backstop on permanent \
             removal is not in force for root, and a privileged run leaves the audit \
             log owned by root, which stops every later ordinary run from recording \
             itself. Nothing this tool cleans needs elevated privileges — run it as \
             yourself.",
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_super_user_is_refused() {
        assert!(refusal(SUPER_USER).is_some());
        assert!(refusal(0).is_some());
    }

    #[test]
    fn every_other_user_is_allowed() {
        // 501 is the first ordinary account on macOS; the rest are chosen to
        // cover the boundary above zero, a system account, and the top of the
        // range, since `uid_t` is unsigned and -1 is a real value there.
        for euid in [1, 2, 501, 502, 65_534, u32::MAX] {
            assert!(refusal(euid).is_none(), "uid {euid} should be allowed");
        }
    }

    /// The message is user-facing and is the only thing a person gets when the
    /// tool refuses to start, so it has to say what to do instead.
    #[test]
    fn the_refusal_says_what_to_do_about_it() {
        let why = refusal(SUPER_USER).unwrap();
        assert!(why.contains("run it as yourself"), "{why}");
        assert!(why.contains("audit log"), "{why}");
    }

    /// Not a tautology: this asserts the reading is wired to the *effective*
    /// id and that the test runner is not privileged — if it ever were, every
    /// other test in this crate would be exercising a different kernel policy
    /// than the one they were written against.
    #[test]
    fn the_test_runner_is_not_the_super_user() {
        assert_ne!(
            effective_uid(),
            SUPER_USER,
            "the suite is running as root; its file-permission assumptions do not hold"
        );
    }
}
