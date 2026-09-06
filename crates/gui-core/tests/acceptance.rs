//! Tests for the first-run terms acceptance record.
//!
//! This is the assent layer described in `docs/LEGAL.md`: a disclaimer nobody
//! agreed to is worth much less than one they did, so the record of *which*
//! terms were accepted and *when* is the artefact that matters. It is written
//! to a throwaway fixture home here (SAFETY CONTRACT item 7) and never to a
//! real one.
//!
//! Nothing in this module removes anything — the acceptance gate is additive
//! and does not sit in the deletion path.

use std::fs;
use std::path::PathBuf;

use swept_gui_core::acceptance::{
    accept, status, terms_digest, terms_text, ACCEPTANCE_FILE, TERMS_VERSION,
};

fn fake_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    (dir, home)
}

fn record_path(home: &std::path::Path) -> PathBuf {
    home.join("Library/Application Support/swept")
        .join(ACCEPTANCE_FILE)
}

#[test]
fn a_fresh_home_has_not_accepted_anything() {
    let (_g, home) = fake_home();

    let s = status(&home);

    assert!(!s.accepted, "a first launch must not count as accepted");
    assert_eq!(s.accepted_version, None);
    assert_eq!(s.terms_version, TERMS_VERSION);
    assert!(
        !record_path(&home).exists(),
        "asking about status must not create the record"
    );
}

#[test]
fn accepting_records_it_and_status_then_agrees() {
    let (_g, home) = fake_home();

    accept(&home, "0.3.0").unwrap();

    let s = status(&home);
    assert!(s.accepted);
    assert_eq!(s.accepted_version.as_deref(), Some(TERMS_VERSION));

    let raw = fs::read_to_string(record_path(&home)).unwrap();
    assert!(raw.contains(TERMS_VERSION), "the version accepted");
    assert!(raw.contains(&terms_digest()), "the text that was accepted");
    assert!(raw.contains("0.3.0"), "the build that presented it");
}

#[test]
fn an_acceptance_of_older_terms_does_not_carry_forward() {
    let (_g, home) = fake_home();
    let path = record_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // A record from a build that shipped terms we have since revised.
    fs::write(
        &path,
        r#"{"acceptances":[{"terms_version":"0.9","terms_digest":"deadbeefdeadbeef","app_version":"0.2.0","epoch_ms":1}]}"#,
    )
    .unwrap();

    let s = status(&home);

    assert!(
        !s.accepted,
        "revised terms must be presented again, not inherited"
    );
    assert_eq!(
        s.accepted_version.as_deref(),
        Some("0.9"),
        "but we still say what they had accepted before"
    );
}

#[test]
fn the_same_terms_accepted_under_a_different_digest_is_not_accepted() {
    let (_g, home) = fake_home();
    let path = record_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Same version string, different text: a build that edited TERMS.md
    // without bumping the version. Fail closed and ask again.
    fs::write(
        &path,
        format!(
            r#"{{"acceptances":[{{"terms_version":"{TERMS_VERSION}","terms_digest":"0000000000000000","app_version":"0.2.0","epoch_ms":1}}]}}"#
        ),
    )
    .unwrap();

    let s = status(&home);

    assert!(!s.accepted, "the digest is what pins the text they saw");
}

#[test]
fn a_corrupt_record_fails_closed() {
    let (_g, home) = fake_home();
    let path = record_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{ this is not json").unwrap();

    let s = status(&home);

    assert!(
        !s.accepted,
        "an unreadable record means we ask again, never that we assume yes"
    );
}

#[test]
fn accepting_twice_keeps_the_earlier_record() {
    let (_g, home) = fake_home();

    accept(&home, "0.3.0").unwrap();
    accept(&home, "0.4.0").unwrap();

    let raw = fs::read_to_string(record_path(&home)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries = parsed["acceptances"].as_array().unwrap();

    assert_eq!(entries.len(), 2, "the record is a history, not a flag");
    assert_eq!(entries[0]["app_version"], "0.3.0");
    assert_eq!(entries[1]["app_version"], "0.4.0");
}

#[test]
fn the_embedded_terms_are_the_real_document() {
    let text = terms_text();

    assert!(
        text.contains("LIMITATION OF LIABILITY"),
        "the terms shipped in the binary must be the terms in the repo"
    );
    assert!(text.contains(TERMS_VERSION));
    // The app has no general URL-opening capability by design, so the full text
    // has to be readable in-app rather than behind a link.
    assert!(text.len() > 2000, "not a stub");
}

#[test]
fn the_digest_is_stable_and_derived_from_the_text() {
    let a = terms_digest();
    let b = terms_digest();

    assert_eq!(a, b, "same input, same digest");
    assert_eq!(a.len(), 16, "a 64-bit digest in hex");
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn the_record_lands_beside_the_audit_log_and_nowhere_else() {
    let (_g, home) = fake_home();

    accept(&home, "0.3.0").unwrap();

    let expected = home
        .join("Library/Application Support/swept")
        .join(ACCEPTANCE_FILE);
    assert!(expected.exists());
    // Same directory the audit log uses, so one folder holds the whole record.
    assert_eq!(
        expected.parent().unwrap(),
        home.join("Library/Application Support/swept")
    );
}

// --- Path safety -----------------------------------------------------------
//
// `accept` is the one place this module touches the filesystem, and it writes
// to a fixed, predictable name inside the home directory. These pin the checks
// that make that safe. They mirror the three the CLI has around
// `resolve_audit_path`, which is the established convention for the other write
// that does not pass through `guard`.

#[test]
fn accept_refuses_a_data_dir_symlinked_into_a_protected_location() {
    let (_g, home) = fake_home();
    let keychains = home.join("Library/Keychains");
    fs::create_dir_all(&keychains).unwrap();
    fs::create_dir_all(home.join("Library/Application Support")).unwrap();
    // The data directory itself is a symlink to a denylisted location.
    std::os::unix::fs::symlink(&keychains, home.join("Library/Application Support/swept")).unwrap();

    let err = accept(&home, "0.3.0").expect_err("must refuse");

    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(
        !keychains.join(ACCEPTANCE_FILE).exists(),
        "nothing may be written into the protected location"
    );
}

#[test]
fn accept_does_not_write_through_a_symlink_at_the_record_path() {
    let (_g, home) = fake_home();
    let dir = home.join("Library/Application Support/swept");
    fs::create_dir_all(&dir).unwrap();
    let victim = home.join("Documents/important.txt");
    fs::create_dir_all(victim.parent().unwrap()).unwrap();
    fs::write(&victim, b"do not clobber me").unwrap();
    // A symlink planted where the record goes. Writing through it would
    // truncate the target; replacing it is the correct outcome.
    std::os::unix::fs::symlink(&victim, dir.join(ACCEPTANCE_FILE)).unwrap();

    accept(&home, "0.3.0").unwrap();

    assert_eq!(
        fs::read_to_string(&victim).unwrap(),
        "do not clobber me",
        "the symlink target must be untouched"
    );
    assert!(
        !fs::symlink_metadata(record_path(&home))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the record must be a real file, not the planted link"
    );
}

#[test]
fn accept_preserves_an_unreadable_record_instead_of_discarding_it() {
    let (_g, home) = fake_home();
    let path = record_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{ this is not json").unwrap();

    accept(&home, "0.3.0").unwrap();

    // The new record is well-formed...
    let raw = fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["acceptances"].as_array().unwrap().len(), 1);

    // ...and the bytes we could not read are still on disk. They are evidence
    // of an earlier acceptance even when we cannot parse them, so overwriting
    // them silently would lose the only copy.
    let dir = path.parent().unwrap();
    let kept: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("unreadable"))
        .collect();
    assert_eq!(kept.len(), 1, "the unparseable record was set aside");
    assert_eq!(
        fs::read_to_string(kept[0].path()).unwrap(),
        "{ this is not json"
    );
}

#[test]
fn accept_leaves_no_staging_file_behind() {
    let (_g, home) = fake_home();

    accept(&home, "0.3.0").unwrap();

    let dir = home.join("Library/Application Support/swept");
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec![ACCEPTANCE_FILE.to_string()], "no litter");
}

#[test]
fn accept_does_not_write_through_a_symlink_at_the_staging_path() {
    let (_g, home) = fake_home();
    let dir = home.join("Library/Application Support/swept");
    fs::create_dir_all(&dir).unwrap();
    let victim = home.join("Documents/important.txt");
    fs::create_dir_all(victim.parent().unwrap()).unwrap();
    fs::write(&victim, b"do not clobber me").unwrap();

    // The staging name is the weak point, not the record name: the record is
    // replaced by a rename (which cannot write through a link), but the staged
    // file is *written*, and an open-for-write follows a symlink and truncates
    // whatever is on the other end. A predictable staging name is therefore a
    // planted-symlink primitive that turns "record a consent" into "overwrite
    // an arbitrary file".
    std::os::unix::fs::symlink(&victim, dir.join("acceptance.json.staged")).unwrap();

    let _ = accept(&home, "0.3.0");

    assert_eq!(
        fs::read_to_string(&victim).unwrap(),
        "do not clobber me",
        "a symlink at the staging path must not redirect the write"
    );
}
