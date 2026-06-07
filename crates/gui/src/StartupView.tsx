import { useEffect, useState } from "react";
import type { LoginItem } from "./types";
import { SAMPLE_LOGIN_ITEMS } from "./sample";

export default function StartupView() {
  const [items, setItems] = useState<LoginItem[] | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        setItems(await invoke<LoginItem[]>("login_items"));
      } catch {
        setItems(SAMPLE_LOGIN_ITEMS);
      }
    })();
  }, []);

  if (!items) {
    return <p className="text-muted text-sm">Loading…</p>;
  }

  const active = items.filter((i) => i.run_at_load && !i.disabled).length;

  return (
    <>
      <section className="rounded-xl border border-border bg-surface p-5">
        <p className="text-lg font-medium">
          {active} app{active === 1 ? "" : "s"} run at login
        </p>
        <p className="text-muted mt-0.5 text-sm">
          Review what starts automatically to speed up boot. This view is read-only —
          mac-cleaner never changes your startup items.
        </p>
      </section>

      <ul className="mt-3 space-y-2">
        {items.map((it) => (
          <li key={it.label} className="rounded-xl border border-border bg-surface px-4 py-3">
            <div className="flex items-baseline justify-between gap-3">
              <span className="truncate font-medium">{it.label}</span>
              <Badge item={it} />
            </div>
            {it.program && <p className="text-muted mt-1 truncate text-xs">{it.program}</p>}
          </li>
        ))}
      </ul>
    </>
  );
}

function Badge({ item }: { item: LoginItem }) {
  if (item.disabled) {
    return (
      <span className="text-muted shrink-0 rounded-full border border-border px-2 py-0.5 text-xs">
        disabled
      </span>
    );
  }
  if (item.run_at_load) {
    return (
      <span className="shrink-0 rounded-full bg-accent px-2 py-0.5 text-xs font-medium text-white">
        runs at login
      </span>
    );
  }
  return (
    <span className="text-muted shrink-0 rounded-full border border-border px-2 py-0.5 text-xs">
      on demand
    </span>
  );
}
