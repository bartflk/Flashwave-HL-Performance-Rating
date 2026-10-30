import { useEffect, useRef, useState } from "react";
import { friendlyText } from "../lib/apierror";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { type Owner } from "../api/types";
import { startSync } from "../lib/sync";
import { t } from "../lib/i18n";

/** The players this PC has been switched between, newest first. */
const RECENT_KEY = "hl.recentOwners";
const RECENT_MAX = 6;

type Recent = { steamid: string; name: string | null; avatar: string | null };

function readRecent(): Recent[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function remember(r: Recent) {
  try {
    const rest = readRecent().filter((x) => x.steamid !== r.steamid);
    localStorage.setItem(RECENT_KEY, JSON.stringify([r, ...rest].slice(0, RECENT_MAX)));
  } catch {
    // Blocked storage only costs the shortcut list.
  }
}

/**
 * Your picture and name, top right. Both come from your ETF2L profile, or
 * Steam's public profile without one; until they arrive (the lookup runs in
 * the background after setup and on every sync) the SteamID stands in.
 *
 * Clicking it switches whose app this is (vilden: a PC shared between
 * players, or a SteamID typed wrong). Nothing is deleted: every match stays
 * stored, a sync fetches the new player's, and switching back is instant.
 */
export function OwnerBadge({ steamid }: { steamid: string | null }) {
  const q = useQuery({
    queryKey: ["owner", steamid],
    queryFn: api.getOwner,
    // Keep asking until the picture has arrived; then it only changes on a sync.
    refetchInterval: (query) => (query.state.data?.avatar ? false : 5000),
  });
  const o = q.data;
  const name = o?.name ?? null;
  const initial = (name ?? "?").trim().charAt(0).toUpperCase();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (o && o.steamid64) remember({ steamid: o.steamid64, name: o.name, avatar: o.avatar });
  }, [o]);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  return (
    <div className="owner-wrap" ref={box}>
      <button
        className={open ? "owner owner-button open" : "owner owner-button"}
        title={t("Switch player")}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <div className="owner-text">
          <span className="owner-name">{name ?? t("You")}</span>
          <code className="owner-id">{steamid}</code>
        </div>
        <Avatar src={o?.avatar ?? null} initial={initial} />
      </button>
      {open && <SwitchMenu current={o ?? null} steamid={steamid} onDone={() => setOpen(false)} />}
    </div>
  );
}

function Avatar({ src, initial, small }: { src: string | null; initial: string; small?: boolean }) {
  const cls = small ? "owner-avatar small" : "owner-avatar";
  return src ? (
    <img className={cls} src={src} alt="" />
  ) : (
    <span className={`${cls} owner-initial`} aria-hidden>
      {initial}
    </span>
  );
}

function SwitchMenu({ current, steamid, onDone }: { current: Owner | null; steamid: string | null; onDone: () => void }) {
  const qc = useQueryClient();
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const others = readRecent().filter((r) => r.steamid !== (current?.steamid64 ?? steamid));

  async function switchTo(input: string) {
    setBusy(true);
    setError(null);
    try {
      await api.setSteamId(input.trim());
      // Every panel was about the previous player.
      await qc.invalidateQueries();
      onDone();
      // Their matches may not be stored yet; one already running will do.
      if (!(await api.syncBusy())) void startSync(false);
    } catch (e) {
      setError(friendlyText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="owner-menu" role="dialog" aria-label={t("Switch player")}>
      <h3>{t("Switch player")}</h3>
      <p className="hint">{t("Every match stays stored. A sync fetches the new player's matches, and switching back is instant.")}</p>
      {others.length > 0 && (
        <div className="owner-recent">
          {others.map((r) => (
            <button key={r.steamid} className="owner-recent-row" disabled={busy} onClick={() => void switchTo(r.steamid)}>
              <Avatar src={r.avatar} initial={(r.name ?? "?").trim().charAt(0).toUpperCase()} small />
              <span className="owner-recent-name">{r.name ?? r.steamid}</span>
              <code>{r.steamid}</code>
            </button>
          ))}
        </div>
      )}
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) void switchTo(value);
        }}
      >
        <input
          type="text"
          autoFocus
          placeholder={t("SteamID or profile link")}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          disabled={busy}
        />
        <button className="primary" type="submit" disabled={busy || !value.trim()}>
          {busy ? t("Switching…") : t("Switch")}
        </button>
      </form>
      {error && <p className="error">{error}</p>}
    </div>
  );
}
