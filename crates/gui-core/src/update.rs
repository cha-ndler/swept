//! The update check — the **only** network request Swept can make.
//!
//! It exists because an app with no auto-update strands every user on whatever
//! version they downloaded, and on a data-destroying tool the fixes that
//! matter most are safety fixes. It is shaped so that it cannot become more
//! than it says it is:
//!
//! - **Off unless asked.** It runs when the user presses *Check for updates*, or
//!   at launch only if they ticked the box that says so. Nothing here runs on
//!   its own; the caller decides, and the default is not to.
//! - **One fixed URL, read-only.** A GET to GitHub's "latest release" endpoint
//!   for this repository. Nothing is sent but what any HTTPS request carries:
//!   the IP address, and a `User-Agent` naming the running version.
//! - **It downloads nothing and installs nothing.** It answers "is there a
//!   newer version, and where is its page" — the user fetches it themselves,
//!   the same way they got this one, and checks its `.sha256`.
//! - **Nothing from the response reaches the UI unparsed.** The tag must parse
//!   as `MAJOR.MINOR.PATCH`, and the link is rebuilt from those three numbers
//!   against a fixed prefix; the response's own URLs are never used.
//!
//! The request is made by the system's `/usr/bin/curl` rather than an HTTP
//! crate: no new dependencies, the system's TLS and proxy settings, and the
//! webview's Content Security Policy stays closed to all outbound connections.
//! `PRIVACY.md` describes this in the user's terms; keep the two in step.

use std::process::Command;

use serde::Serialize;

/// The endpoint. Fixed; never derived from configuration or input.
pub const RELEASES_API: &str = "https://api.github.com/repos/cha-ndler/swept/releases/latest";

/// Every release page lives under this prefix.
const RELEASE_TAG_PAGE: &str = "https://github.com/cha-ndler/swept/releases/tag/";

/// The running version, from the single-sourced workspace version.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the check found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateStatus {
    /// The version running now.
    pub current: String,
    /// The latest published release.
    pub latest: String,
    /// True only when `latest` is strictly newer than `current`.
    pub newer: bool,
    /// The release page for `latest`, rebuilt from its parsed version number.
    pub url: String,
}

/// `MAJOR.MINOR.PATCH`, with an optional leading `v` and nothing else — a
/// pre-release or build suffix is not a version this check will offer.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.split('.');
    let mut next = || -> Option<u64> {
        let p = parts.next()?;
        // `parse` alone would accept "+1"; a version component is digits only.
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };
    let v = (next()?, next()?, next()?);
    if parts.next().is_some() {
        return None;
    }
    Some(v)
}

/// Decide, from GitHub's response body, whether there is something newer.
pub fn evaluate(current: &str, body: &str) -> Result<UpdateStatus, String> {
    let cur = parse_version(current)
        .ok_or_else(|| format!("this build's version {current:?} is not MAJOR.MINOR.PATCH"))?;
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "GitHub's answer was not readable".to_string())?;
    // `releases/latest` already excludes drafts and pre-releases; refusing
    // them here as well keeps that from being a property of someone else's API.
    if json["draft"].as_bool() == Some(true) || json["prerelease"].as_bool() == Some(true) {
        return Err("GitHub named a draft or pre-release as the latest".to_string());
    }
    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| "GitHub's answer named no release".to_string())?;
    let (a, b, c) = parse_version(tag)
        .ok_or_else(|| "the latest release has no plain version number".to_string())?;
    let latest = format!("{a}.{b}.{c}");
    Ok(UpdateStatus {
        current: format!("{}.{}.{}", cur.0, cur.1, cur.2),
        url: format!("{RELEASE_TAG_PAGE}v{latest}"),
        newer: (a, b, c) > cur,
        latest,
    })
}

/// Make the request. Called only on the user's say-so; see the module docs.
pub fn check() -> Result<UpdateStatus, String> {
    let out = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            // HTTPS only, including on any redirect.
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            "10",
            // The real answer is a few kilobytes.
            "--max-filesize",
            "1000000",
            "--header",
            "Accept: application/vnd.github+json",
            "--user-agent",
            &format!("Swept/{CURRENT_VERSION}"),
            RELEASES_API,
        ])
        .output()
        .map_err(|e| format!("could not run curl: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if why.is_empty() {
            "the check did not complete".to_string()
        } else {
            why
        });
    }
    evaluate(CURRENT_VERSION, &String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(tag: &str) -> String {
        serde_json::json!({ "tag_name": tag, "draft": false, "prerelease": false }).to_string()
    }

    #[test]
    fn versions_parse_only_in_their_plain_form() {
        assert_eq!(parse_version("0.5.1"), Some((0, 5, 1)));
        assert_eq!(parse_version("v10.0.22"), Some((10, 0, 22)));
        for bad in [
            "",
            "v",
            "1.2",
            "1.2.3.4",
            "1.2.3-rc1",
            "1.2.x",
            "+1.2.3",
            "1..3",
            " 1.2.3",
            "vv1.2.3",
        ] {
            assert_eq!(parse_version(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_newer_release_is_reported_with_a_rebuilt_link() {
        let s = evaluate("0.5.0", &body("v0.6.0")).unwrap();
        assert!(s.newer);
        assert_eq!(s.latest, "0.6.0");
        assert_eq!(
            s.url,
            "https://github.com/cha-ndler/swept/releases/tag/v0.6.0"
        );
    }

    #[test]
    fn the_same_or_an_older_release_is_not_newer() {
        assert!(!evaluate("0.5.0", &body("v0.5.0")).unwrap().newer);
        assert!(!evaluate("0.10.0", &body("v0.9.9")).unwrap().newer);
    }

    /// Numeric, not lexical: "0.10.0" > "0.9.0" although "1" < "9".
    #[test]
    fn comparison_is_numeric() {
        assert!(evaluate("0.9.0", &body("v0.10.0")).unwrap().newer);
    }

    /// The response never chooses the link. A hostile or broken answer can
    /// make the check fail; it cannot put an arbitrary URL in front of the user.
    #[test]
    fn nothing_but_the_version_number_is_taken_from_the_response() {
        let hostile = serde_json::json!({
            "tag_name": "v9.9.9",
            "html_url": "https://example.invalid/phish",
            "draft": false,
            "prerelease": false,
        })
        .to_string();
        let s = evaluate("0.5.0", &hostile).unwrap();
        assert_eq!(
            s.url,
            "https://github.com/cha-ndler/swept/releases/tag/v9.9.9"
        );

        for tag in ["v1.0.0/../../evil", "v1.0.0?x=1", "<script>", "v1.0.0 "] {
            assert!(evaluate("0.5.0", &body(tag)).is_err(), "{tag:?}");
        }
    }

    #[test]
    fn drafts_prereleases_and_garbage_are_errors_not_offers() {
        let draft = serde_json::json!({ "tag_name": "v1.0.0", "draft": true }).to_string();
        let pre = serde_json::json!({ "tag_name": "v1.0.0", "prerelease": true }).to_string();
        for b in [draft.as_str(), pre.as_str(), "", "{}", "not json", "[]"] {
            assert!(evaluate("0.5.0", b).is_err(), "{b:?}");
        }
    }

    #[test]
    fn this_builds_own_version_is_checkable() {
        assert!(
            parse_version(CURRENT_VERSION).is_some(),
            "{CURRENT_VERSION}"
        );
    }

    /// The URL is a constant, and it is the repository this project publishes
    /// from — a fork that forgets to change it checks upstream, which is the
    /// safe direction (it reports, it never installs).
    #[test]
    fn the_endpoint_is_https_and_fixed() {
        assert!(RELEASES_API.starts_with("https://api.github.com/repos/"));
        assert!(RELEASE_TAG_PAGE.starts_with("https://github.com/"));
    }
}
