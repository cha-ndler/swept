---
description: Build a throwaway fixture HOME and run a read-only scan preview against it.
---

Demonstrate the scanner safely, without touching the real filesystem.

Run this (it uses a temp dir under `$TMPDIR`, never the real home):

```bash
FIX=$(mktemp -d)
mkdir -p "$FIX/Library/Caches/app" "$FIX/Library/Logs" "$FIX/Documents"
head -c 4096  /dev/zero > "$FIX/Library/Caches/app/blob.cache"
head -c 16384 /dev/zero > "$FIX/Library/Logs/old.log"
printf 'precious' > "$FIX/Documents/keep.txt"
echo "--- scan (read-only preview) ---"
HOME="$FIX" cargo run -q -p macclean -- scan
echo "--- fixture intact? Documents/keep.txt must still exist ---"
ls -R "$FIX/Documents"
rm -rf "$FIX"
```

Confirm the preview lists the cache + log files, does **not** list
`Documents/keep.txt`, and that nothing in the fixture was modified (this is
`scan`, which is read-only by construction). Report the preview output.
