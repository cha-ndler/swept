# Notices and attributions

## Trademarks

Swept is an independent, unaffiliated project. The following marks belong to
their respective owners and are used here **only to describe compatibility,
interoperability, or comparison** — a use commonly called *nominative fair
use*. No affiliation, sponsorship or endorsement is claimed or implied.

| Mark | Owner |
|---|---|
| Apple, macOS, Mac, Finder, Safari, Time Machine, Xcode, Apple Silicon | Apple Inc. |
| CleanMyMac | MacPaw Inc. |
| Google, Chrome | Google LLC |
| Firefox | Mozilla Foundation |
| Microsoft, Edge | Microsoft Corporation |
| Brave | Brave Software, Inc. |
| Arc | The Browser Company of New York, Inc. |
| Opera | Opera Norway AS |
| Vivaldi | Vivaldi Technologies AS |
| Homebrew | Homebrew maintainers |

**On the CleanMyMac comparison.** Swept's documentation describes it as "an
open-source alternative to CleanMyMac". That is a factual comparative
statement about what kind of program this is, using MacPaw's mark to identify
MacPaw's product. Swept is not a CleanMyMac derivative, contains no MacPaw
code, and is neither endorsed by nor connected to MacPaw Inc.

**Rules for this repository.** To keep that use defensible, do not:

- use another party's mark in the product name, the bundle identifier, a domain
  name, an icon, or any logo;
- style another party's mark in its own typeface, colour or logotype;
- use a mark more prominently than necessary to make the comparison; or
- write anything that could read as affiliation, partnership or endorsement.

## Third-party components

Swept links Rust crates and bundles JavaScript packages, each under its own
licence. The authoritative, machine-generated inventory ships with each release
and can be regenerated locally:

```bash
cargo install cargo-about && cargo about generate about.hbs > third-party-licenses.html
```

The principal direct dependencies and their licences:

| Component | Licence |
|---|---|
| Tauri, `wry`, `tao` | MIT OR Apache-2.0 |
| `serde`, `serde_json` | MIT OR Apache-2.0 |
| `walkdir`, `clap`, `dirs`, `plist`, `tempfile`, `filetime`, `proptest` | MIT OR Apache-2.0 |
| `trash` | MIT |
| React, React DOM | MIT |
| Vite, Tailwind CSS | MIT |

All are permissive. **Swept ships no copyleft-licensed code**, and adding a
dependency under GPL, AGPL or LGPL terms requires a deliberate decision — see
[`docs/LEGAL.md`](docs/LEGAL.md).

## Swept's own licence

Swept's source is © 2026 __LEGAL_ENTITY__, licensed under the MIT License. See
[`LICENSE`](LICENSE) for the grant and [`TERMS.md`](TERMS.md) for the terms
that accompany the official binaries.
