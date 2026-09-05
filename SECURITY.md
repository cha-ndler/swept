# Security policy

Swept removes files. A bug here does not leak data — it destroys it. That makes
the interesting reports different from most projects': the ones that matter are
about **acting on the wrong path**, **acting without consent**, or **saying
something untrue about what will happen**.

## Reporting

**Please do not open a public issue for anything in the classes below.** Use
GitHub's private reporting instead — *Security* → *Report a vulnerability* on
this repository — which reaches the maintainer without disclosing the path to
anyone else first.

Include the smallest reproduction you can: a fixture directory layout is far
more useful than a description, and this project's own tests are all built that
way (`tempfile::tempdir()`, never a real path — please do the same, and never
send a reproduction that runs against your actual home directory).

You should get an acknowledgement within a few days. This is a personal project
with no on-call rotation, so please do not expect an SLA; it will be taken
seriously and you will be credited unless you would rather not be.

## What counts

These are the classes worth reporting privately, in rough order of severity:

- **Disposal outside the allowlist.** Any path reaching the executor that is not
  inside `allowlist::default_roots`, or a granted path that did not come from an
  individual human choice.
- **A denylist bypass.** Anything that gets `~/Library/Keychains`,
  `~/Library/Mail`, `/System`, `/Applications`, the home root itself, or a
  directory containing a `.git` past `guard` or `guard_dir` — including through
  symlinks, `..`, case variation, or a TOCTOU race.
- **Acting without the consent that was given.** A disposal that happens without
  `Consent::execute`, a mass delete without confirmation, or — specific to this
  app — anything that carries a *consequence* (signs you out, erases history,
  loses site data) being acted on without the acknowledgement for that
  consequence.
- **The preview and the action disagreeing.** If what a screen shows you and
  what the confirmed run does can differ, that is a report even when nothing is
  destroyed, because every safety property here rests on the user having seen
  what they agreed to.
- **A figure that is not true.** Reporting bytes as freed that were not freed, or
  presenting an incomplete scan as a complete one. This project treats an
  overstated total as a defect of the same family as a wrong deletion: both make
  a person act on something false.
- **Escaping the audit log.** A mutation that is not recorded, or a refusal that
  leaves no trace.

## What does not count

- **The `.dmg` is unsigned and un-notarized.** That is known, documented in the
  README, and tracked as roadmap item D2. macOS will warn you, and it should.
- **Reports that the app can remove files.** That is the feature. It previews
  first and acts only on explicit consent; a report needs to show one of those
  two properties failing.
- **Anything requiring an attacker who can already write to your home
  directory.** They can remove your files without this app.
- **Findings produced by running the tool against your real disk to see what
  happens.** Please use a fixture.

## Scope

The safety substrate is `crates/safety` (the trust kernel: denylist,
canonicalization, scoped allowlist) and `crates/core` (scanner, planner,
executor, audit log). `crates/gui-core` is the command layer where every
frontend-facing ceiling lives, and it is where most of the interesting
boundaries are. The React frontend in `crates/gui/src` is **not** trusted by the
backend: a finding of the form "the frontend could send X" is only a finding if
the backend accepts X.

The full, non-negotiable rules are in [`CLAUDE.md`](CLAUDE.md) under SAFETY
CONTRACT, with a plain-English version in [`docs/SAFETY.md`](docs/SAFETY.md).

## Supported versions

The latest release only. This project is pre-1.0 and there are no maintenance
branches.
