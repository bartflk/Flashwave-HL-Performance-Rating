import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api } from "./api/client";
import { errorMessage } from "./api/types";
import { Setup } from "./components/Setup";
import { Ready } from "./components/Ready";
import "./App.css";

export default function App() {
  // Set when the user chooses to revisit setup after it is already complete.
  const [forceSetup, setForceSetup] = useState(false);

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
  const showSetup = !data.ready || forceSetup;

  return (
    <div className="shell">
      {showSetup ? (
        <Setup
          status={data}
          onDone={() => {
            setForceSetup(false);
            void status.refetch();
          }}
        />
      ) : (
        <Ready status={data} onReconfigure={() => setForceSetup(true)} />
      )}
      <footer className="footer">
        <span>HL Rating {data.version} — M0</span>
        <code>{data.dbPath}</code>
      </footer>
    </div>
  );
}
