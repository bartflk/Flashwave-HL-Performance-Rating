import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "./index.css";
import { initTheme } from "./lib/theme";
import { initScale, watchScaleKeys } from "./lib/scale";
import { loadUserLanguages } from "./lib/userLang";
import { loadEtf2lNames } from "./lib/etf2lNames";
import { onNamesChanged } from "./lib/names";

// Local IPC, not a network: refetching on every window focus buys nothing.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { refetchOnWindowFocus: false, staleTime: 5_000 },
  },
});

// Before the first render, so the window never paints the default palette
// and then swaps to the chosen one.
initTheme();
// The interface's size (2K and 4K screens), and Ctrl + / - / 0 for it.
initScale();
watchScaleKeys();

// Translation fixes from the user's lang folder. Not awaited: the built-in
// text paints first, and the fixes swap in the moment they are read.
void loadUserLanguages();

// Players' ETF2L names, for when they are the chosen ones. A change of
// choice, or new names after a sync, refetches so every page follows.
void loadEtf2lNames();
onNamesChanged(() => void queryClient.invalidateQueries());

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
