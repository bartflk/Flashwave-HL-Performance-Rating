import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api } from "./api/client";
import { errorMessage } from "./api/types";
import { Setup } from "./components/Setup";
import { Settings } from "./components/Settings";
import { Matches } from "./components/Matches";
import { SyncSummary } from "./components/SyncSummary";
import { MatchPage } from "./components/match/MatchPage";
import { ProfilePage } from "./components/profile/ProfilePage";
import { TeammatesPage } from "./components/teammates/TeammatesPage";
import { PlayersPage } from "./components/players/PlayersPage";
import { ToastHost } from "./lib/toast";
import { Notifications } from "./components/Notifications";
import { watchDownloads } from "./lib/downloads";
import { watchSync } from "./lib/sync";
import { OwnerBadge } from "./components/OwnerBadge";
import { RestoreBanner } from "./components/RestoreBanner";
import "./App.css";
import "./components/match/match.css";

type Tab = "matches" | "profile" | "teammates" | "players" | "settings";

// Settings is not one of these: it is the cog on the far right, where a
// setting belongs, rather than a fourth thing to read.
const TABS: Array<[Tab, string]> = [
  ["matches", "Matches"],
  ["profile", "Profile"],
  ["teammates", "Teammates"],
  ["players", "Players"],
];

export default function App() {
  // Set when the user chooses to revisit setup after it is already complete.
  const [forceSetup, setForceSetup] = useState(false);
  // A first run keeps the setup screen up until its last step, even once the
  // SteamID alone has made the app ready.
  const [onboarding, setOnboarding] = useState<boolean | null>(null);
  const [tab, setTab] = useState<Tab>("matches");
  const [openLog, setOpenLog] = useState<number | null>(null);
  // A sync and a demo download both keep running while you read other
  // matches, so the app follows them rather than the panel that started one.
  const qc = useQueryClient();
  useEffect(watchDownloads, []);
  useEffect(() => watchSync(qc), [qc]);
  // Pages stay mounted once visited, so their filters and scroll survive a
  // trip to a match and back.
  const [visited, setVisited] = useState<Set<Tab>>(new Set(["matches"]));

  const go = (t: Tab) => {
    setTab(t);
    setOpenLog(null);
    setVisited((v) => (v.has(t) ? v : new Set(v).add(t)));
  };

  const status = useQuery({
    queryKey: ["app_status"],
    queryFn: api.appStatus,
    retry: false,
  });

  if (status.isPending) {
    return (
      <div className="shell">
        <div className="centered">
          <p className="hint">Opening database…</p>
        </div>
      </div>
    );
  }

  if (status.isError) {
    return (
      <div className="shell">
        <div className="centered">
          <div className="card">
            <h1>Could not start</h1>
            <p className="error" style={{ marginTop: 12 }}>
              {errorMessage(status.error)}
            </p>
            <button style={{ marginTop: 18 }} onClick={() => void status.refetch()}>
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  }

  const data = status.data;

  // Before anything else, including setup: a wiped install looks exactly like
  // a new one, and only the backup beside it tells them apart.
  if (data.restore) {
    return (
      <div className="shell">
        <div className="centered">
          <RestoreBanner offer={data.restore} onSettled={() => void status.refetch()} />
        </div>
      </div>
    );
  }

  if (onboarding === null) setOnboarding(!data.ready);

  if (!data.ready || forceSetup || onboarding) {
    return (
      <div className="shell">
        <Setup
          status={data}
          onDone={() => void status.refetch()}
          onFinish={() => {
            setForceSetup(false);
            setOnboarding(false);
            go("matches");
          }}
        />
      </div>
    );
  }

  return (
    <div className="shell">
      <header className="topbar">
        <div className="brand">
          <h1 className="wordmark">
            <img src="/logo.svg" alt="" />
            <span>
              Flashwave<span className="hl">.tf</span>
            </span>
            <span className="beta-tag" title="Early build: expect rough edges, and please report them">
              alpha
            </span>
          </h1>
          <nav className="tabs">
            {TABS.map(([t, label]) => (
              <button key={t} className={tab === t ? "tab active" : "tab"} onClick={() => go(t)}>
                {label}
              </button>
            ))}
          </nav>
        </div>
        <div className="topbar-right">
          <SyncSummary />
          <OwnerBadge steamid={data.config.steamid} />
          <button
            className={tab === "settings" ? "cog active" : "cog"}
            title="Settings"
            aria-label="Settings"
            aria-pressed={tab === "settings"}
            onClick={() => go(tab === "settings" ? "matches" : "settings")}
          >
            <Cog />
          </button>
        </div>
      </header>

      <div hidden={tab !== "matches" || openLog !== null}>
        <Matches onOpen={setOpenLog} />
      </div>
      {visited.has("profile") && (
        <div hidden={tab !== "profile" || openLog !== null}>
          <ProfilePage onOpenMatch={setOpenLog} />
        </div>
      )}
      {visited.has("teammates") && (
        <div hidden={tab !== "teammates" || openLog !== null}>
          <TeammatesPage />
        </div>
      )}
      {visited.has("players") && (
        <div hidden={tab !== "players" || openLog !== null}>
          <PlayersPage onOpenMatch={setOpenLog} />
        </div>
      )}
      {tab === "settings" && openLog === null && (
        <Settings
          status={data}
          onReconfigure={() => {
            setForceSetup(true);
            go("matches");
          }}
        />
      )}
      {/* "Back" returns to whichever tab the match was opened from. */}
      {openLog !== null && <MatchPage logId={openLog} onBack={() => setOpenLog(null)} />}
      <Notifications onOpenMatch={setOpenLog} />
      <ToastHost />
    </div>
  );
}

/** A cogwheel, drawn rather than fetched: eight teeth and a hole. */
function Cog() {
  return (
    <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden focusable="false">
      <path
        fill="currentColor"
        d="M12 8.2a3.8 3.8 0 1 0 0 7.6 3.8 3.8 0 0 0 0-7.6Zm0 6a2.2 2.2 0 1 1 0-4.4 2.2 2.2 0 0 1 0 4.4Z"
      />
      <path
        fill="currentColor"
        d="m20.5 13.6.02-1.6-.02-1.6-2.1-.34a6.6 6.6 0 0 0-.62-1.5l1.24-1.73-2.26-2.26-1.73 1.24a6.6 6.6 0 0 0-1.5-.62L12.9 2.7h-1.8l-.63 2.1c-.53.14-1.03.35-1.5.62L7.24 4.18 4.98 6.44l1.24 1.73c-.27.47-.48.97-.62 1.5l-2.1.33v3.2l2.1.33c.14.53.35 1.03.62 1.5l-1.24 1.73 2.26 2.26 1.73-1.24c.47.27.97.48 1.5.62l.33 2.1h3.2l.33-2.1c.53-.14 1.03-.35 1.5-.62l1.73 1.24 2.26-2.26-1.24-1.73c.27-.47.48-.97.62-1.5l2.1-.33Zm-3.6-.55a5.1 5.1 0 0 1-.78 1.88l-.3.44.97 1.35-.43.43-1.35-.97-.44.3c-.57.38-1.2.64-1.88.78l-.52.1-.26 1.64h-.6l-.26-1.64-.52-.1a5.1 5.1 0 0 1-1.88-.78l-.44-.3-1.35.97-.43-.43.97-1.35-.3-.44a5.1 5.1 0 0 1-.78-1.88l-.1-.52-1.64-.26v-.6l1.64-.26.1-.52c.14-.68.4-1.31.78-1.88l.3-.44-.97-1.35.43-.43 1.35.97.44-.3c.57-.38 1.2-.64 1.88-.78l.52-.1.26-1.64h.6l.26 1.64.52.1c.68.14 1.31.4 1.88.78l.44.3 1.35-.97.43.43-.97 1.35.3.44c.38.57.64 1.2.78 1.88l.1.52 1.64.26v.6l-1.64.26-.1.52Z"
      />
    </svg>
  );
}
