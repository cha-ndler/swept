# What mac-cleaner will and won't touch

This is the plain-English version. The enforced rules live in
[`CLAUDE.md`](../CLAUDE.md) → SAFETY CONTRACT, and the code that implements them
is in `crates/safety` and `crates/core/src/executor.rs`.

If anything here disagrees with the code, **the code is what runs** — please
open an issue, because that means this page is wrong.

---

## The short version

- Nothing is removed unless you confirm it, in that session, for that list.
- Files go to the **Trash**, not into the void. You can drag them back out.
- There are places the app refuses to touch no matter what you click.
- Everything it plans and everything it does is written down.

---

## Where it cleans without being asked

"Cleanup" only ever works inside four kinds of location:

| Location | Why it is safe to clear |
|---|---|
| `~/Library/Caches` | Applications recreate what they need. |
| `~/Library/Logs` | Diagnostic records; nothing depends on them. |
| `~/Library/Developer/Xcode/DerivedData` | Build intermediates and indexes; Xcode rebuilds them. |
| `~/.Trash` | You already threw these away. |

Homebrew's download cache lives under `~/Library/Caches` and is included for the
same reason: `brew` re-downloads on demand.

That list is the whole of it. Cleanup cannot reach your documents, your photos,
your mail, or anything else — not through a setting, and not through a bug in
the interface, because the restriction is enforced in the engine rather than in
the buttons.

## Where it will never go

These are refused first, before any other check, and the refusal cannot be
overridden from the interface:

- `/System`, `/usr`, `/bin`, `/sbin`, `/Library`, `/Applications`
- `~/Library/Keychains` — your passwords
- `~/Library/Mail` — your mail
- your home folder itself
- **anything inside a `.git` directory** — a repository's history is not junk,
  however much space its object store takes
- **any folder that contains one of the above.** `~/Library` is refused as a
  whole precisely because Keychains and Mail live inside it, even though
  `~/Library/Caches` is cleanable.

Matching ignores capitalisation. macOS disks are usually case-insensitive while
paths are not normalised, so `~/library/MAIL` names the same mail that
`~/Library/Mail` does, and a repository directory spelled `.GIT` is the same
repository. Both are refused.

## Reading more than it can act on

Large & Old looks in `~/Documents`, `~/Downloads`, `~/Desktop`, `~/Movies`,
`~/Music`, `~/Pictures` and `~/Library/Application Support`. Those are your
files, not junk, and the app treats them that way:

- **Nothing is ever pre-selected.** There is no select-all. The button stays
  disabled until you tick something yourself.
- These are never part of a default clean.
- The confirmation sheet **names the files**, rather than giving you a count.

Being *shown* a file grants nothing. When you confirm, each chosen path is
checked again from scratch: it must still pass every protection rule, it must
already be spelled the way the disk spells it (so a symbolic link put in place
after the list was drawn cannot redirect the action onto something else), it
must be inside the folders that were actually searched, and its size is re-read
from the disk rather than trusted from the screen.

If a single item fails any of those, **the entire request is refused** and
nothing is touched. A partial run is not what you agreed to.

Space Lens goes further: it cannot act on anything **at all**. It measures the
same folders and draws them, and there is no command behind that screen that
accepts a folder back — nothing there is selectable, and no button exists to
select it with. To act on something you spot in it, you go to Large & Old and
consent there, file by file. The toolbar says "Read-only view" for exactly this
reason: a screen full of large clickable shapes would otherwise be reasonable to
mistake for a control panel.

## What happens to a file

It is moved to the Trash. That is recoverable: open the Trash and drag it back.

Permanent removal exists only in the command-line tool, behind an explicit
`--permanent` flag, and even then only for the four cleanup locations above. The
desktop app cannot do it at all.

## Large actions

Past a threshold — many files at once, or a lot of space — an extra confirmation
is required, and the interface tells you that is why it is asking. The threshold
is enforced in the engine, so an interface that forgot to ask would be refused
rather than obeyed.

## The record

Every planned action and every carried-out action is appended to:

```
~/Library/Application Support/macclean/audit.jsonl
```

One JSON object per line, with absolute paths and sizes. Previews are recorded
too, so you can see what a dry run *would* have done.

If that log cannot be written, **the run stops**. The app will not remove
something it cannot record removing.

## Numbers you can trust

A scan can be incomplete for ordinary reasons: macOS privacy controls hide
`~/.Trash` and `~/Library/Containers` from apps without Full Disk Access, a
folder may be unreadable, a search may hit its own limit.

When that happens the app says so and presents the figure as a floor — "this is
at least this much" — rather than showing a smaller number as if it were the
whole truth. If you see a total without a caveat, nothing was hidden from it.

Two details worth knowing:

- **Hard-linked files are excluded** from Large & Old and counted once in Space
  Lens. When two names point at the same data, removing one frees nothing, so
  promising that space would be a lie.
- **Space totals are an upper bound.** APFS "clones" — what Finder's *Duplicate*
  and many installers create — share their storage while each copy reports its
  full size, and there is no cheap way to tell them apart. You may free slightly
  less than the figure says. Never more.

## Getting files back

1. **From the Trash.** Open it and drag them out. This is the normal path.
2. **From the audit log.** It records the original absolute path of everything
   moved, so you can always find out exactly where something came from.

## Reporting a safety problem

If you find a path that should be refused and is not, that is the most serious
kind of bug this project can have. Please open an issue with the exact path and
what happened — or, if you would rather not do that publicly, contact the
maintainer directly.
