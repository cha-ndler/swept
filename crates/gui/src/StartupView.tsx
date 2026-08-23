import { useEffect, useState } from "react";
import type { LoginItem } from "./types";
import { call, describeError, isDesktopApp } from "./backend";
import { Banner, Group, InfoIcon, ShieldIcon, Toolbar } from "./Shell";

export default function StartupView({
  onCount,
}: {
  /** Reports how many items run at login, for the sidebar badge. */
  onCount?: (n: number | null) => void;
}) {
  const [items, setItems] = useState<LoginItem[] | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    void (async () => {
      try {
        const list = await call<LoginItem[]>("login_items");
        setItems(list);
        onCount?.(list.filter((i) => i.run_at_load && !i.disabled).length);
      } catch (e) {
        // Never fall back to sample items: a fabricated startup list would have
        // the user reasoning about launch agents that may not exist.
        setItems(null);
        onCount?.(null);
        setError(describeError(e));
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <>
      <Toolbar title="Startup" />
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <Body items={items} error={error} />
      </div>
    </>
  );
}

function Body({ items, error }: { items: LoginItem[] | null; error: string }) {
  if (error) {
    return (
      <section className="rounded-card border border-separator bg-surface p-8 text-center">
        <div className="text-danger mx-auto grid h-12 w-12 place-items-center rounded-panel border border-danger/25 bg-danger/10">
          <InfoIcon size={24} />
        </div>
        <p className="mt-4 text-title font-semibold">
          {isDesktopApp() ? "Couldn’t read your login items" : "mac-cleaner runs as a desktop app"}
        </p>
        <p className="text-muted mx-auto mt-1.5 max-w-md text-body">
          {isDesktopApp()
            ? error
            : "This page is a preview shell with no access to your disk. Open the mac-cleaner app to review startup items."}
        </p>
      </section>
    );
  }

  if (!items) {
    return (
      <p className="text-muted text-body" role="status" aria-live="polite">
        Loading…
      </p>
    );
  }

  const active = items.filter((i) => i.run_at_load && !i.disabled).length;

  return (
    <>
      <section className="rounded-card border border-separator bg-surface p-5">
        <p className="font-mono text-display font-semibold tabular-nums">{active}</p>
        <p className="text-muted mt-1 text-body">
          app{active === 1 ? "" : "s"} run automatically at login
        </p>
      </section>

      <div className="mt-3">
        <Banner tone="safe" icon={<ShieldIcon size={15} />}>
          Read-only. mac-cleaner never changes your startup items — reviewing what starts
          automatically is the whole feature.
        </Banner>
      </div>

      <Group className="mt-3">
        <ul>
          {items.map((it, i) => (
            <li
              key={it.label}
              className={`flex items-center gap-3 px-4 py-3 transition-colors duration-fast ease-mac hover:bg-surface2 ${
                i > 0 ? "border-t border-separator" : ""
              }`}
            >
              <div className="min-w-0 flex-1">
                <p className="truncate text-body font-medium">{it.label}</p>
                {it.program && (
                  <p className="text-subtle mt-0.5 truncate font-mono text-caption">{it.program}</p>
                )}
              </div>
              <Badge item={it} />
            </li>
          ))}
        </ul>
      </Group>
    </>
  );
}

function Badge({ item }: { item: LoginItem }) {
  if (item.disabled) {
    return (
      <span className="text-subtle shrink-0 rounded-full border border-separator px-2 py-0.5 text-caption">
        disabled
      </span>
    );
  }
  if (item.run_at_load) {
    return (
      <span className="shrink-0 rounded-full bg-accent px-2 py-0.5 text-caption font-medium text-white">
        runs at login
      </span>
    );
  }
  return (
    <span className="text-subtle shrink-0 rounded-full border border-separator px-2 py-0.5 text-caption">
      on demand
    </span>
  );
}
