import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Bookmark, BookmarkKind } from "../api/types";
import { openPlayer, openRgl, openSeason, openTeam } from "../lib/goto";
import { t, k } from "../lib/i18n";

/**
 * Bookmarks (Q60, Clark): a star on match, player, team and season pages,
 * and a list in the top bar to open them from. Kept in the database, so
 * they survive an update and a restart.
 */

const KEY = ["bookmarks"];

export function useBookmarks() {
  return useQuery({ queryKey: KEY, queryFn: api.getBookmarks, staleTime: Infinity });
}

/** The star: hollow when the page is not kept, filled when it is. */
export function BookmarkButton({ b }: { b: Bookmark }) {
  const qc = useQueryClient();
  const list = useBookmarks().data ?? [];
  const on = list.some((x) => x.kind === b.kind && x.id === b.id);
  const [busy, setBusy] = useState(false);

  async function toggle(e: React.MouseEvent) {
    e.stopPropagation();
    setBusy(true);
    try {
      qc.setQueryData(KEY, await api.setBookmark(b, !on));
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      type="button"
      className={on ? "bm-star on" : "bm-star"}
      onClick={(e) => void toggle(e)}
      disabled={busy}
      aria-pressed={on}
      title={on ? t("Remove from bookmarks") : t("Bookmark this page")}
      aria-label={on ? t("Remove from bookmarks") : t("Bookmark this page")}
    >
      {on ? "★" : "☆"}
    </button>
  );
}

const GROUPS: Array<[BookmarkKind, string]> = [
  ["match", k("Matches")],
  ["player", k("Players")],
  ["team", k("Teams")],
  ["season", k("Seasons")],
  ["rgl_team", k("RGL teams")],
  ["rgl_season", k("RGL seasons")],
];

/** The top bar's list of bookmarks, grouped by kind, newest first. */
export function BookmarksMenu({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const qc = useQueryClient();
  const list = useBookmarks().data ?? [];
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  // A click anywhere else, or Escape, closes it.
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  function go(b: Bookmark) {
    setOpen(false);
    if (b.kind === "match") onOpenMatch(b.id);
    else if (b.kind === "player") openPlayer(b.id);
    else if (b.kind === "team") openTeam(b.id);
    else if (b.kind === "rgl_team") openRgl({ kind: "team", id: b.id });
    else if (b.kind === "rgl_season") openRgl({ kind: "season", id: b.id });
    else openSeason(b.id);
  }

  async function remove(b: Bookmark) {
    qc.setQueryData(KEY, await api.setBookmark(b, false));
  }

  return (
    <div className="bm-menu" ref={box}>
      <button
        className={open ? "bm-open on" : "bm-open"}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        title={t("Bookmarks")}
        aria-label={t("Bookmarks")}
      >
        ★{list.length > 0 && <span className="bm-count">{list.length}</span>}
      </button>
      {open && (
        <div className="bm-pop" role="dialog" aria-label={t("Bookmarks")}>
          <h3>{t("Bookmarks")}</h3>
          {list.length === 0 && <p className="hint">{t("Nothing kept yet. The ☆ beside a match, player, team or season keeps it here.")}</p>}
          {GROUPS.map(([kind, label]) => {
            const rows = list.filter((b) => b.kind === kind);
            if (rows.length === 0) return null;
            return (
              <section key={kind}>
                <h4>{t(label)}</h4>
                <ul>
                  {rows.map((b) => (
                    <li key={`${b.kind}-${b.id}`}>
                      <button className="bm-go" onClick={() => go(b)}>
                        <span className="bm-label">{b.label}</span>
                        {b.sub && <span className="hint bm-sub">{b.sub}</span>}
                      </button>
                      <button className="bm-x" onClick={() => void remove(b)} title={t("Remove from bookmarks")} aria-label={t("Remove from bookmarks")}>
                        ×
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}
