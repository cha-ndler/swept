# Privacy Policy — Swept

**Version 1.0.** Last revised 2026-09-06. Published by cha-ndler, an
individual — there is no company behind Swept, and nothing below changes if
that ever stops being true.

## The short version

**Swept collects nothing, sends nothing, and has no server.** It has no
analytics, no crash reporting, no telemetry, no account, and no network code of
any kind. It reads your disk to tell you what is on it, and everything it
learns stays on your machine.

This is a verifiable claim, not a promise. The app's Content Security Policy
permits no outbound connections (`connect-src 'self' ipc:`), and the source
contains no HTTP client. You can check both:

```bash
grep -rn "reqwest\|hyper\|ureq\|curl\|fetch(" crates/*/src
```

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

Two planned features would involve the network for the first time:

- **Automatic updates** (roadmap D3) would contact a release server to ask
  whether a newer version exists. That request necessarily reveals your IP
  address and the version you are running.
- **Crash reporting**, if it is ever added, would be **opt-in** and off by
  default.

Neither exists today. If either ships, this policy will be revised **before**
the release that introduces it, the change will be called out in
[`CHANGELOG.md`](CHANGELOG.md), and any update check will be one you can turn
off.

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
