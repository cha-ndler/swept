# Releasing Swept

How a signed, notarized `.dmg` gets built and published. Read
[`LEGAL.md`](LEGAL.md) first if you are wondering *why* it is distributed this
way rather than through the Mac App Store — the short answer is that a
sandboxed app cannot read other applications' caches, so the store was never an
option.

Everything in Part 1 happens once. Everything in Part 3 happens per release.

---

## Part 1 — One-time setup

Do these in order. Steps 1 and 2 gate step 3, and step 3 gates everything else.

### 1. Form the entity

The Apple Developer Program treats individuals and organizations as separate
enrollments with separate Team IDs. **Enrolling as an individual and converting
later means a new Team ID, which means a new Developer ID certificate, which
means macOS treats the next release as a different developer.** So the entity
comes first.

- [ ] Form the LLC **in Maryland** — the state the work is actually done in.
      See [`LEGAL.md`](LEGAL.md), "Why Maryland and not Delaware", for the
      arithmetic and the case law. The short version: Delaware would cost two
      to three times Maryland's $300/yr and would not improve the shield,
      because Maryland is one of the hardest states in the country to pierce.
- [ ] Get an EIN from the IRS. Free, online, issued immediately.
- [ ] Open a business bank account and **keep it strictly separate**. The
      liability shield depends on the formalities being observed, not on the
      filing existing.
- [ ] Appoint a resident agent — Maryland's term for a registered agent
      (~$100–150/yr) — unless you are serving as your own and are comfortable
      with your home address being public record on SDAT's business search.
- [ ] Adopt a written operating agreement. Maryland does not file it, but for a
      single-member LLC it is a large part of what makes the entity look like an
      entity rather than a bank account with a name.
- [ ] **Diary April 15, every year.** Maryland's Annual Report / Personal
      Property Return (SDAT Form 1) is $300 and is not optional. Miss it for
      long enough and SDAT forfeits the charter — at which point there is no
      shield at all, which is the failure mode that matters here. The 2022
      proposal to zero-rate online filings did not become law; the only
      practical waiver requires employees and MarylandSaves payroll
      contributions, so it will not apply.
- [ ] Assign the copyright in the existing code to the LLC in writing, and
      update [`LICENSE`](../LICENSE) — see step 6. If the entity is the
      publisher but you personally still own the copyright, the two layers do
      not line up.
- [ ] Put the entity's exact legal name into `__LEGAL_ENTITY__` — see step 6.
      `__GOVERNING_STATE__` is already resolved to Maryland.

### 2. Get a D-U-N-S number

Organization enrollment requires one. It is **free** from Dun & Bradstreet, and
Apple has a dedicated request form for developers.

- [ ] Request at <https://developer.apple.com/enroll/duns-lookup/>.
- [ ] Use the **exact** legal entity name as filed, and an address that matches
      your formation documents. A mismatch here is the single most common
      reason organization enrollment stalls.
- [ ] Allow 1–5 business days, occasionally longer. Wait for it to appear in
      D&B's records before starting step 3.

### 3. Enroll in the Apple Developer Program

- [ ] Enroll as an **Organization** at <https://developer.apple.com/programs/enroll/>.
      $99/year.
- [ ] You will need: the D-U-N-S number, the legal entity name, a website at a
      domain associated with the entity, and authority to bind the entity to
      contracts.
- [ ] Expect verification to take days to a couple of weeks. Apple may
      telephone to confirm.

> **The website requirement is real.** Apple checks that the organization has a
> public web presence. Stand up a landing page before enrolling rather than
> discovering this mid-review — the roadmap's D4 item covers it anyway.

### 4. Create the certificate

Only a **Developer ID Application** certificate is needed. A *Developer ID
Installer* certificate signs `.pkg` files, and Swept ships a `.dmg`.

- [ ] In Keychain Access: **Certificate Assistant → Request a Certificate From a
      Certificate Authority**, save to disk. This makes the private key.
- [ ] At <https://developer.apple.com/account/resources/certificates/list>,
      create a **Developer ID Application** certificate from that request.
- [ ] Download and double-click it to install into the login keychain.
- [ ] Confirm it is there and note the exact identity string:

```bash
security find-identity -v -p codesigning
```

The line you want reads `Developer ID Application: Your Entity LLC (ABCDE12345)`.
**The whole string, parentheses included, is `APPLE_SIGNING_IDENTITY`.**

- [ ] Back the private key up. Export it as a `.p12` and store it somewhere you
      will still have it in three years. **A lost Developer ID private key
      cannot be recovered** — Apple can revoke and reissue, but every build
      signed with the old key is then orphaned.

### 5. Create the notarization API key

Notarization can authenticate with an Apple ID plus an app-specific password,
or with an App Store Connect API key. **Use the API key.** It is scoped to
notarization alone, so leaking it cannot touch the account, and it does not
break when the account's 2FA changes.

- [ ] At <https://appstoreconnect.apple.com/access/integrations/api>, create a
      key with the **Developer** role (Admin is not required).
- [ ] Note the **Key ID** and the **Issuer ID**.
- [ ] Download the `.p8`. **It can only be downloaded once.**

### 6. Fill in the placeholders

Two placeholders are seeded through the repository and
`./scripts/verify.sh --bundle` refuses to pass while any remain.

```bash
rg -l '__LEGAL_ENTITY__|__GOVERNING_STATE__'
```

- [ ] Replace `__LEGAL_ENTITY__` with the entity's exact legal name — the same
      string, character for character, as the D-U-N-S record and the Apple
      enrollment. A mismatch between the three is what stalls enrollment.
- [ ] `__GOVERNING_STATE__` is **already resolved to Maryland** in
      [`TERMS.md`](../TERMS.md). Nothing to do unless the entity is formed
      somewhere else, in which case revisit `LEGAL.md` first.
- [ ] Update the copyright line in [`LICENSE`](../LICENSE) from `cha-ndler` to
      the entity.
- [ ] Have an attorney read [`TERMS.md`](../TERMS.md) and
      [`PRIVACY.md`](../PRIVACY.md). One to three hours; this is a
      well-trodden document set.

### 7. Add the CI secrets

At **Settings → Secrets and variables → Actions**. All six, or none — the
`package` job checks for presence and quietly builds unsigned when they are
absent, which is what keeps forks working.

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` export password |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Entity LLC (TEAMID)` |
| `APPLE_API_KEY` | The App Store Connect **Key ID** |
| `APPLE_API_ISSUER` | The **Issuer ID** |
| `APPLE_API_KEY_CONTENT` | The entire `.p8`, including the BEGIN/END lines |

---

## Part 2 — Signing locally

Useful for the first signed build, because a failure here has a readable error
where a CI failure has a log to dig through.

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Entity LLC (TEAMID)"
export APPLE_API_KEY="XXXXXXXXXX"
export APPLE_API_ISSUER="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
export APPLE_API_KEY_PATH="$HOME/private_keys/AuthKey_XXXXXXXXXX.p8"
cd crates/gui && cargo tauri build
```

Notarization takes a few minutes. Tauri submits, waits, and staples.

### Verifying the signed build

**`cargo tauri build` succeeds whether or not it signed anything**, so verify
rather than assume. CI runs exactly these checks and fails the job if any of
them does.

```bash
APP="crates/gui/src-tauri/target/release/bundle/macos/Swept.app"

codesign --verify --deep --strict --verbose=2 "$APP"   # is it signed, and intact
codesign --display --entitlements - "$APP"             # which entitlements shipped
spctl --assess --type execute --verbose=4 "$APP"       # what Gatekeeper will decide
xcrun stapler validate "$APP"                          # is the ticket attached
```

`spctl` should say `accepted` and `source=Notarized Developer ID`. Anything
else means the build will show a Gatekeeper warning.

The stapler check is the one people skip and should not: without a stapled
ticket the app validates only while the machine can reach Apple, so a user who
first opens it offline sees a warning on an otherwise perfect build.

**Check for a secure timestamp too.** `codesign -dvv` must print a `Timestamp=`
line; a `Signed Time=` line instead means there is no *secure* timestamp, and
Apple's notary service rejects that. Tauri does not pass `--timestamp`
explicitly, and `man codesign` says that without it "a system-specific default
behavior is invoked", which "may result in some but not all code signatures
being timestamped" — so this is worth looking at rather than assuming.

The entitlements line should print an **empty** dict. Swept grants none, on
purpose: [`entitlements.plist`](../crates/gui/src-tauri/entitlements.plist)
records the evidence for each one considered and rejected. If that line ever
shows something, find out who added it and why.

### The real test

Verify on a Mac that has never seen the certificate, from a download rather
than a local path — quarantine attributes are the thing being tested, and a
locally built `.app` does not carry them.

```bash
xattr -l ~/Downloads/Swept_0.3.0_aarch64.dmg   # expect com.apple.quarantine
```

### If the notarized app crashes on launch

**Check for a malformed entitlements file first.** Since macOS 10.15.4 a process
with malformed embedded entitlements does not run at all — it aborts with a
code-signature validation error, which looks exactly like a missing entitlement
and is far more common than one. This is very likely the real origin of the
"Tauri crashes after notarization" folklore.

```bash
plutil -lint crates/gui/src-tauri/entitlements.plist
```

Only then treat it as a missing entitlement, and be sceptical — Swept ships an
empty entitlements dict on the evidence recorded in that file, and a minimal
WKWebView app signed the same way was measured launching and running JavaScript
fine. Check `Console.app` for a kill by `taskgated`, then read the file: it
names each entitlement considered, why it was rejected, and what would change
that. Add the narrowest one that actually fixes it and write down why, rather
than pasting in the usual three.

### The Trash goes through the Finder

Worth knowing before the first signed build, because it produces a prompt
nobody added on purpose. The `trash` crate's macOS default is
`DeleteMethod::Finder`, which shells out to `osascript` — so the first disposal
triggers a TCC automation consent dialog naming Swept. `Info.plist` carries an
`NSAppleEventsUsageDescription` explaining it, and no entitlement is required;
`entitlements.plist` says why. Switching to `DeleteMethod::NsFileManager` would
remove the prompt entirely, at the cost of Finder's "Put Back" — a real
trade-off for a tool whose recovery story is the Trash, and one to decide
deliberately rather than by leaving the default in place unexamined.

---

## Part 3 — Cutting a release

```bash
./scripts/verify.sh --bundle
```

This is the gate. It runs everything CI runs plus the bundler, the terms-version
agreement check, and the placeholder check. **CI is the second opinion; this is
the first** — macOS runners bill at 10x on a private repository, and the
`package` job is the most expensive one in the workflow.

1. [ ] `./scripts/verify.sh --bundle` passes, with nothing skipped that matters.
2. [ ] `CHANGELOG.md` has an entry for the version.
3. [ ] The version agrees across all five files (`verify.sh` checks this).
4. [ ] If `TERMS.md` changed, its `**Version X.Y.**` line is bumped — every user
       is then asked to accept again on next launch, which is the intended
       behaviour and worth being deliberate about.
5. [ ] Tag and push:

```bash
git tag -a v0.3.1 -m "Swept v0.3.1" && git push origin v0.3.1
```

6. [ ] The `package` job builds, signs, notarizes, verifies, and attaches the
       `.dmg` plus `SHA256SUMS.txt` to the GitHub Release.
7. [ ] Confirm the run finished by conclusion, not by the watcher:

```bash
gh run view <id> --json status,conclusion
```

`status: completed` **and** `conclusion: success`. `gh run watch --exit-status`
can return before the run is done.

> **A job that fails in ~2 seconds having run zero steps is a billing stop, not
> a broken build.** Check `gh api repos/{owner}/{repo}/actions/runs/<id>/jobs` —
> empty `steps` and a two-second duration means the Actions allowance is
> exhausted. Do not debug the build.

8. [ ] Download the `.dmg` from the Release and run the Part 2 verification on
       it, on a machine that is not this one.

---

## What is not automated yet

- **Universal binary** (roadmap D1). CI inherits the Apple Silicon runner, so
  Intel Macs get nothing. Note that `--target universal-apple-darwin` moves the
  bundle output path, so the upload and release globs both have to change with
  it.
- **Auto-update** (roadmap D3). Needs its own signing keypair, separate from
  the Developer ID certificate, and a `latest.json`. It also puts Swept on the
  network for the first time — [`PRIVACY.md`](../PRIVACY.md) must be revised in
  the same release, not after it.
- **Homebrew cask.** Wait until the Developer ID is stable; a cask pinned to an
  unsigned build then re-pinned to a signed one is a bad first impression.
