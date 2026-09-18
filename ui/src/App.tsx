import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api } from "./api/client";
import { errorMessage } from "./api/types";
import { Setup } from "./components/Setup";
import { Settings } from "./components/Settings";
import { Matches } from "./components/Matches";
import { SyncStrip } from "./components/SyncStrip";
import { MatchPage } from "./components/match/MatchPage";
import "./App.css";
import "./components/match/match.css";

type Tab = "matches" | "settings";

export default function App() {
  // Set when the user chooses to revisit setup after it is already complete.
  const [forceSetup, setForceSetup] = useState(false);
  const [tab, setTab] = useState<Tab>("matches");
  const [openLog, setOpenLog] = useState<number | null>(null);

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

  if (!data.ready || forceSetup) {
    return (
      <div className="shell">
        <Setup
          status={data}
          onDone={() => {
            setForceSetup(false);
            void status.refetch();
          }}
        />
      </div>
    );
  }

  return (
    <div className="shell">
      <header className="topbar">
        <div className="brand">
          <h1>HL Rating</h1>
          <nav className="tabs">
            {(["matches", "settings"] as const).map((t) => (
              <button
                key={t}
                className={tab === t ? "tab active" : "tab"}
                onClick={() => {
                  setTab(t);
                  setOpenLog(null);
                }}
              >
                {t === "matches" ? "Matches" : "Settings"}
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

      {tab === "matches" ? (
        <>
          {/* Kept mounted while a match is open, so the filter, page count and
              scroll position are all still there on the way back. */}
          <div hidden={openLog !== null}>
            <Matches onOpen={setOpenLog} />
          </div>
          {openLog !== null && <MatchPage logId={openLog} onBack={() => setOpenLog(null)} />}
        </>
      ) : (
        <Settings
          status={data}
          onReconfigure={() => {
            setForceSetup(true);
            setTab("matches");
          }}
        />
      )}
    </div>
  );
}
