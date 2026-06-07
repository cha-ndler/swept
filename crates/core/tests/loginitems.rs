//! Tests for the read-only login-items inspector. Fixtures only.

use std::fs;
use std::path::Path;

use macclean_core::loginitems::scan_dir;

fn write_plist(dir: &Path, name: &str, body: &str) {
    fs::write(dir.join(name), body).unwrap();
}

const FOO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.example.foo</string>
  <key>ProgramArguments</key><array><string>/usr/local/bin/foo</string><string>--flag</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#;

const BAR_DISABLED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.example.bar</string>
  <key>Program</key><string>/opt/bar/bard</string>
  <key>RunAtLoad</key><true/>
  <key>Disabled</key><true/>
</dict>
</plist>
"#;

#[test]
fn parses_login_items_from_fixture_dir() {
    let dir = tempfile::tempdir().unwrap();
    write_plist(dir.path(), "com.example.foo.plist", FOO);
    write_plist(dir.path(), "com.example.bar.plist", BAR_DISABLED);
    // Non-plist files are ignored.
    fs::write(dir.path().join("README.txt"), "not a plist").unwrap();

    let items = scan_dir(dir.path());
    assert_eq!(items.len(), 2, "should parse exactly the two .plist files");

    let foo = items.iter().find(|i| i.label == "com.example.foo").unwrap();
    assert!(foo.run_at_load);
    assert!(!foo.disabled);
    assert_eq!(foo.program.as_deref(), Some("/usr/local/bin/foo"));

    let bar = items.iter().find(|i| i.label == "com.example.bar").unwrap();
    assert!(bar.disabled, "Disabled=true must be reflected");
    assert_eq!(bar.program.as_deref(), Some("/opt/bar/bard"));
}

#[test]
fn missing_directory_yields_no_items() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");
    assert!(scan_dir(&missing).is_empty());
}

#[test]
fn unparseable_plist_is_skipped_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    write_plist(dir.path(), "broken.plist", "this is not xml");
    write_plist(dir.path(), "com.example.foo.plist", FOO);
    let items = scan_dir(dir.path());
    assert_eq!(items.len(), 1, "a broken plist is skipped, not fatal");
}
