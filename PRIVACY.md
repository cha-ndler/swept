# Privacy Policy — Swept

**Version 1.1.** Last revised 2026-09-24. Published by cha-ndler, an
individual — there is no company behind Swept, and nothing below changes if
that ever stops being true.

## The short version

**Swept collects nothing, sends nothing about you, and has no server.** It has
no analytics, no crash reporting, no telemetry and no account. It reads your
disk to tell you what is on it, and everything it learns stays on your machine.

It makes **one** kind of network request, and **only when you ask**: the update
check described below. Until you press *Check for updates* — or turn on *Check
at launch*, which is off unless you turn it on — Swept does not touch the
network at all.

This is a verifiable claim, not a promise. The app's Content Security Policy
permits the window no outbound connections (`connect-src 'self' ipc:`), the
source contains no HTTP client library, and the update check is the only place
a request is made. You can check all three:

```bash
grep -rn "reqwest\|hyper\|ureq\|curl\|fetch(" crates/*/src
# → only crates/gui-core/src/update.rs
```

## The update check

When you ask, Swept sends **one HTTPS request** to GitHub —
`https://api.github.com/repos/cha-ndler/swept/releases/latest` — to learn the
number of the newest published release, and tells you whether it is newer than
yours.

- **What GitHub receives:** what any web request carries — your IP address —
  plus a `User-Agent` header naming Swept and the version you are running.
  Nothing about your files, your scans or your Mac is sent. GitHub's own
  privacy statement governs what it does with the request.
- **What Swept does with the answer:** reads the version number and nothing
  else. It **downloads nothing and installs nothing**; if there is a newer
  version, it shows you the link, and you download it yourself.
- **When it happens:** only when you press *Check for updates*, or at launch if
  you turned on *Check at launch*. That setting is stored only in the app's own
  local storage on your Mac, and turning it off stops the launch check.

## What Swept reads

To do its job Swept reads file metadata — paths, sizes, modification dates —
and in a few narrow cases file contents: application `Info.plist` files to
identify bundles, browser profile files to count what a browser is holding, and
`LaunchAgents` property lists to list login items.

**All of this stays on your computer.** It is held in memory for the length of
a scan and shown to you on screen.

## What Swept writes

Two files, both under `~/Library/Application Support/swept/`:

| File | Contents |
|---|---|
| `audit.jsonl` | Every action Swept planned or carried out: timestamp, absolute path, size, disposition. Append-only. |
| `acceptance.json` | The record that you accepted [`TERMS.md`](TERMS.md): the terms version, its content hash, and a timestamp. |

Both are plain text you can read, and both are yours. Neither is transmitted
anywhere. To erase them, move the `swept` folder to the Trash — Swept will
treat the next launch as a first launch.

The audit log is a record of **paths**, and paths can be revealing. If you
share one when reporting a bug, read it first.

## Permissions Swept asks for

macOS may prompt you to grant access to your Desktop, Documents, Downloads, or
Full Disk Access. Swept asks because a scan cannot see those locations
otherwise, and a scan that silently cannot see them would report a total lower
than the truth. **Declining is supported**: Swept says which locations it could
not read rather than pretending the result is complete.

Grants are made to macOS, not to us. Withdraw them at any time in **System
Settings → Privacy & Security**.

## Children

Swept is not directed at children and collects no information from anyone,
including children under 13.

## If this ever changes

**Crash reporting**, if it is ever added, would be **opt-in** and off by
default. It does not exist today. Automatic *installation* of updates does not
exist either, and the update check above is not it.

If anything here changes, this policy will be revised **before** the release
that introduces it, and the change will be called out in
[`CHANGELOG.md`](CHANGELOG.md).

We will not add analytics, advertising identifiers, or data sharing with third
parties. There is nothing to sell and no one to sell it to.

## Your rights

Because we hold no data about you, there is nothing for us to disclose,
correct, port or erase — so requests under the GDPR, the UK GDPR, the CCPA/CPRA
or similar laws have nothing to act on. The data described above is on your
own computer and under your own control.

## Contact

Questions about this policy: open an issue, or use the private reporting route
in [`SECURITY.md`](SECURITY.md) if it concerns a vulnerability.
