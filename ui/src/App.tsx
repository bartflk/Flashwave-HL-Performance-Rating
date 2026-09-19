import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api } from "./api/client";
import { errorMessage } from "./api/types";
import { Setup } from "./components/Setup";
import { Settings } from "./components/Settings";
import { Matches } from "./components/Matches";
import { SyncStrip } from "./components/SyncStrip";
import { MatchPage } from "./components/match/MatchPage";
import { ProfilePage } from "./components/profile/ProfilePage";
import { TeammatesPage } from "./components/teammates/TeammatesPage";
import { ToastHost } from "./lib/toast";
import "./App.css";
import "./components/match/match.css";

type Tab = "matches" | "profile" | "teammates" | "settings";

const TABS: Array<[Tab, string]> = [
  ["matches", "Matches"],
  ["profile", "Profile"],
  ["teammates", "Teammates"],
  ["settings", "Settings"],
];

export default function App() {
  // Set when the user chooses to revisit setup after it is already complete.
  const [forceSetup, setForceSetup] = useState(false);
  // A first run keeps the setup screen up until its last step, even once the
  // SteamID alone has made the app ready.
  const [onboarding, setOnboarding] = useState<boolean | null>(null);
  const [tab, setTab] = useState<Tab>("matches");
  const [openLog, setOpenLog] = useState<number | null>(null);
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
              <span className="hl">HL</span> Rating
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
        <span className="who">
          <code>{data.config.steamid}</code>
        </span>
      </header>

      {/* The strip stays mounted on every tab so sync progress is never lost. */}
      <SyncStrip />

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
      <ToastHost />
    </div>
  );
}
