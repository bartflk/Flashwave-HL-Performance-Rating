import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { LeagueSamplePanel } from "./LeagueSamplePanel";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api, inTauri } from "../api/client";
import { errorMessage, type AppStatus, type Cleaned, type DemoIndexSummary } from "../api/types";
import { formatDate } from "../lib/format";
import { HistoryPanel } from "./HistoryPanel";
import { SettingsContext, SettingsSection } from "./settings/SettingsSection";
import { SettingsIcon, type IconName } from "./settings/icons";
import { MapsPanel } from "./MapsPanel";
import { ImportPanel } from "./ImportPanel";
import { startRebuild, useSyncStatus } from "../lib/sync";
import { contrast, deleteCustomTheme, luminance, previewTheme, readThemeCode, saveCustomTheme, setTheme, themeById, themeCode, useTheme, useThemes, type Base, type Theme } from "../lib/theme";
import { DEFAULT_SCALE, SCALES, setScale, stepScale, useScale } from "../lib/scale";
import { allLanguages, languageFile, setLanguage, t, t as tr, useLanguage, userLanguageFiles, tx } from "../lib/i18n";
import { loadUserLanguages } from "../lib/userLang";
import { setNameSource, useNameSource } from "../lib/names";
import { checkForUpdate, installUpdate, restartNow, useUpdate } from "../lib/update";
import { RELEASES } from "../lib/changelog";
import { Markdown } from "./Markdown";
import { clearProblems, markProblemsSeen, report, useProblems } from "../lib/problems";

/** The groups of Settings, in order, for the side menu and the page. */
type Group = { id: string; title: string; sections: { id: string; title: string; icon: IconName }[] };

export function Settings({
  status,
  onReconfigure,
}: {
  status: AppStatus;
  onReconfigure: () => void;
}) {
  // Rebuilding reports from the corner like a sync, because it is one: the
  // same events, the same minutes of work.
  const sync = useSyncStatus();
  const busy = sync.state === "running";

  // Flashy's UX pass: a search, a side menu, and sections that fold.
  const [query, setQuery] = useState("");
  const [foldAll, setFoldAll] = useState({ n: 0, closed: false });
  const [focus, setFocus] = useState<{ id: string; n: number } | null>(null);
  const [matched, setMatched] = useState<Record<string, boolean>>({});
  const report = useCallback((id: string, m: boolean) => setMatched((x) => (x[id] === m ? x : { ...x, [id]: m })), []);
  const shared = useMemo(() => ({ query, foldAll, report, focus }), [query, foldAll, report, focus]);

  const groups: Group[] = [
    {
      id: "general",
      title: t("General"),
      sections: [
        { id: "updates", title: tr("Updates"), icon: "update" },
        { id: "language", title: t("Language"), icon: "globe" },
        { id: "names", title: tr("Player names"), icon: "user" },
        { id: "theme", title: t("Theme"), icon: "palette" },
        { id: "scale", title: t("Interface size"), icon: "scale" },
        { id: "problems", title: t("Problems"), icon: "alert" },
      ],
    },
    {
      id: "matches",
      title: t("Matches and data"),
      sections: [
        { id: "history", title: t("How far back"), icon: "clock" },
        { id: "import", title: t("Logs that didn't import"), icon: "fileX" },
        { id: "etf2l", title: t("ETF2L and match types"), icon: "shield" },
        { id: "rawlogs", title: t("Raw logs"), icon: "list" },
        { id: "maps", title: t("Maps"), icon: "map" },
      ],
    },
    {
      id: "demos",
      title: t("Demos"),
      sections: [
        { id: "demos", title: t("Demos"), icon: "film" },
        { id: "downloaded", title: t("Downloaded demos"), icon: "drive" },
      ],
    },
    {
      id: "storage",
      title: t("Your data"),
      sections: [
        { id: "setup", title: t("Setup"), icon: "gear" },
        { id: "data", title: t("Data"), icon: "database" },
        { id: "backups", title: t("Backups"), icon: "archive" },
      ],
    },
    {
      id: "about",
      title: t("About"),
      sections: [
        { id: "sources", title: t("Data sources"), icon: "globe" },
        { id: "changelog", title: tr("Changelog"), icon: "scroll" },
      ],
    },
    ...(import.meta.env.DEV ? [{ id: "dev", title: t("Developer"), sections: [{ id: "league", title: t("League sample"), icon: "network" as IconName }] }] : []),
  ];
  const nothing = query.trim() !== "" && Object.values(matched).every((m) => !m);
  // A group's heading only while one of its sections is showing.
  const shows = (gid: string) => query.trim() === "" || (groups.find((g) => g.id === gid)?.sections.some((x) => matched[x.id] !== false) ?? true);

  return (
    <SettingsContext.Provider value={shared}>
      <div className="content settings-page">
        <header className="settings-top">
          <h1>{t("Settings")}</h1>
          <label className="settings-search">
            <SettingsIcon name="search" />
            <input
              type="search"
              value={query}
              placeholder={t("Search settings")}
              aria-label={t("Search settings")}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setQuery("")}
            />
          </label>
          <div className="settings-fold">
            <button className="linkish" onClick={() => setFoldAll((f) => ({ n: f.n + 1, closed: true }))}>
              {t("Collapse all")}
            </button>
            <button className="linkish" onClick={() => setFoldAll((f) => ({ n: f.n + 1, closed: false }))}>
              {t("Expand all")}
            </button>
          </div>
        </header>

        <div className="settings-layout">
          <nav className="settings-nav" aria-label={t("Settings sections")}>
            {groups.map((g) => (
              <div key={g.id} className="settings-nav-group">
                <span className="settings-nav-title">{g.title}</span>
                {g.sections.map((x) => (
                  <button key={x.id} className={matched[x.id] === false ? "settings-nav-item dim" : "settings-nav-item"} onClick={() => setFocus((f) => ({ id: x.id, n: (f?.n ?? 0) + 1 }))}>
                    <SettingsIcon name={x.icon} size={16} />
                    <span>{x.title}</span>
                  </button>
                ))}
              </div>
            ))}
          </nav>

          <div className="settings-main">
            {nothing && <p className="hint settings-none">{t("No setting matches “{0}”.", { "0": query.trim() })}</p>}

            {shows("general") && <h2 className="settings-group">{t("General")}</h2>}
            <SettingsSection id="updates" icon="update" title={tr("Updates")} keywords="version release github install"
              summary={tr("Version {version}; checked at every start", { version: status.version })}
              info={t("The app asks GitHub for a newer release once when it starts. Installing downloads it and asks you to restart: nothing is swapped while the app has your database open.")}>
              <UpdatesPanel version={status.version} />
            </SettingsSection>
            <SettingsSection id="language" icon="globe" title={t("Language")} keywords="language translation lang"
              summary={t("The app's language, and its translation files")}
              info={t("Translations other than English are drafts until a native speaker checks them; anything untranslated shows in English. Each language is a plain .lang file: fix a line, press Reload, and send the file on Discord to share it.")}>
              <LanguagePanel />
            </SettingsSection>
            <SettingsSection id="names" icon="user" title={tr("Player names")} keywords="names etf2l alias"
              summary={t("Names as in the log, or as on ETF2L")}
              info={t("ETF2L names come from the official rosters already read, so they cost no requests. Anyone who never played an official keeps the name from the log.")}>
              <NamesPanel />
            </SettingsSection>
            <SettingsSection id="theme" icon="palette" title={t("Theme")} keywords="theme colours colors dark custom own make share code"
              summary={t("The app's colours")}>
              <ThemePanel />
            </SettingsSection>
            <SettingsSection id="scale" icon="scale" title={t("Interface size")} keywords="scale zoom size 4k 2k 1440p dpi bigger smaller text"
              summary={t("How big everything is drawn")}>
              <ScalePanel />
            </SettingsSection>
            <SettingsSection id="problems" icon="alert" title={t("Problems")} keywords="errors bug report discord"
              summary={t("What went wrong since the app started")}
              info={t("Failed downloads, unreadable files and the like, since you opened the app. Copy the report into Discord when asking for help.")}>
              <ProblemsPanel version={status.version} />
            </SettingsSection>

            {shows("matches") && <h2 className="settings-group">{t("Matches and data")}</h2>}
            <SettingsSection id="history" icon="clock" title={t("How far back")} keywords="history years old scrims pugs"
              summary={t("Which of your older matches are downloaded")}
              info={t("Officials always count, however old. Older scrims and pugs are only listed: they cost nothing until you open one.")}>
              <HistoryPanel />
            </SettingsSection>
            <SettingsSection id="import" icon="fileX" title={t("Logs that didn't import")} keywords="failed import log add logs.tf"
              summary={t("Logs that failed, and adding one by hand")}
              info={t("A log that could not be downloaded or read is listed here with the reason. Paste a log id or logs.tf link to fetch one right away.")}>
              <ImportPanel />
            </SettingsSection>
            <SettingsSection id="etf2l" icon="shield" title={t("ETF2L and match types")} keywords="etf2l official scrim pug"
              summary={t("Your officials, and how each match is sorted")} hideIntro
              info={t("Read on every sync. Each Highlander match is then an official (on ETF2L), a scrim (most of your side are your regular teammates or roster) or a pug.")}>
              <Etf2lPanel />
            </SettingsSection>
            <SettingsSection id="rawlogs" icon="list" title={t("Raw logs")} keywords="server log kills positions"
              summary={t("Every kill, with its time, classes and positions")} hideIntro
              info={t("The server log behind each logs.tf page. With it each kill is valued on its own: by the victim's class, the map, and whether they were defending. Read on every sync.")}>
              <RawlogPanel />
            </SettingsSection>
            <SettingsSection id="maps" icon="map" title={t("Maps")} keywords="maps images callouts overview"
              summary={t("Map images and callouts for the kill maps")} hideIntro
              info={t("The default top-down images are more.tf's. Import your own for any map and line it up; callouts export as a file to share, and a .callouts.json dropped on the window imports.")}>
              <MapsPanel />
            </SettingsSection>

            {shows("demos") && <h2 className="settings-group">{t("Demos")}</h2>}
            <SettingsSection id="demos" icon="film" title={t("Demos")} keywords="demos folder archive stv pov"
              summary={t("Where your demos are found, and how they link")} hideIntro
              info={t("Demos in tf, tf/demos and tf/demos/stv, and in any other folders you add, are matched to their logs by map and time.")}>
              <DemosPanel />
            </SettingsSection>
            <SettingsSection id="downloaded" icon="drive" title={t("Downloaded demos")} keywords="downloaded delete space stv"
              summary={t("SourceTV demos the app fetched, and clearing them")} hideIntro
              info={t("Each is read once; after that it only takes space and can be downloaded again any time. Your own recordings are never touched.")}>
              <DownloadedDemosPanel />
            </SettingsSection>

            {shows("storage") && <h2 className="settings-group">{t("Your data")}</h2>}
            <SettingsSection id="setup" icon="gear" title={t("Setup")} keywords="steamid tf2 folder path"
              summary={t("Your SteamID and TF2 folder")}
              info={t("The TF2 folder is what demo jumps and downloads need; everything else works without it.")}>
              <div className="panel">
                <h2>{t("Setup")}</h2>
                <dl className="kv" style={{ marginTop: 14 }}>
                  <dt>{tr("SteamID")}</dt>
                  <dd>
                    <code>{status.config.steamid}</code>
                  </dd>
                  <dt>{tr("TF2 folder")}</dt>
                  <dd>
                    {status.config.tfPath ? (
                      <code>{status.config.tfPath}</code>
                    ) : (
                      <span className="muted">{tr("Not set: demo jumps and downloads are off. Everything else works.")}</span>
                    )}
                  </dd>
                </dl>
                <button className="linkish" style={{ marginTop: 14 }} onClick={onReconfigure}>{tr("Change these")}</button>
              </div>
            </SettingsSection>
            <SettingsSection id="data" icon="database" title={t("Data")} keywords="rebuild database reprocess"
              summary={t("Rebuild every match from what is stored")}
              info={t("Works everything out again from the logs already stored, without downloading anything. Worth running after an update that changes how matches are read.")}>
              <div className="panel">
                <h2>{t("Data")}</h2>
                <div className="row" style={{ marginTop: 4 }}>
                  <button onClick={() => void startRebuild()} disabled={busy}>
                    {busy ? tr("Working…") : tr("Rebuild from stored data")}
                  </button>
                </div>
                <dl className="kv" style={{ marginTop: 16 }}>
                  <dt>{tr("Database")}</dt>
                  <dd className="path-row">
                    <code>{status.dbPath}</code>
                    {/* Every log, rating and demo link is in this one file, and it is
                        the only thing here that cannot be fetched again. */}
                    <button className="linkish" onClick={() => void api.revealPath(status.dbPath)}>{tr("Show in Explorer")}</button>
                  </dd>
                  <dt>{tr("Version")}</dt>
                  <dd>{status.version}</dd>
                </dl>
              </div>
            </SettingsSection>
            <SettingsSection id="backups" icon="archive" title={t("Backups")} keywords="backup restore copy database"
              summary={t("Copies of your database, taken before every sync")} hideIntro
              info={t("The newest five are kept beside the database. To restore one, close the app and rename it over hl.sqlite3. Uninstalling can delete them, so keep a copy on another drive too.")}>
              <BackupsPanel />
            </SettingsSection>

            {shows("about") && <h2 className="settings-group">{t("About")}</h2>}
            <SettingsSection id="sources" icon="globe" title={t("Data sources")} keywords="sources credits drops.tf logs.tf trends.tf etf2l demos.tf more.tf icewind"
              summary={t("Where every match, log and demo comes from")}>
              <SourcesPanel />
            </SettingsSection>
            <SettingsSection id="changelog" icon="scroll" title={tr("Changelog")} keywords="changelog release notes version"
              summary={t("What changed in each release")} hideIntro
              info={t("Newest first. The notes are in English.")}>
              <ChangelogPanel />
            </SettingsSection>

            {/* The mass log downloader: the developer's tool, dev builds only. */}
            {import.meta.env.DEV && (
              <>
                {shows("dev") && <h2 className="settings-group">{t("Developer")}</h2>}
                <SettingsSection id="league" icon="network" title={t("League sample")} keywords="league sample downloader"
                  summary={t("Every ETF2L official, downloaded in the background")} hideIntro
                  info={t("Every ETF2L Highlander official of the last six years, playoffs and cups included, downloaded slowly while the app is open and kept apart from your own matches. It lets ratings be read against the whole league.")}>
                  <LeagueSamplePanel />
                </SettingsSection>
              </>
            )}
          </div>
        </div>
      </div>
    </SettingsContext.Provider>
  );
}

/**
 * Where the data comes from, and who made it possible: the sites this app
 * reads, credited by name where a person runs them (Flashy: drops.tf is
 * Icewind's).
 */
function SourcesPanel() {
  const rows: [string, string, string][] = [
    ["drops.tf", "https://drops.tf/about", t("By Icewind. logs.tf's logs and raw server logs, asked first: fast and with no rate limit, about an hour behind logs.tf.")],
    ["logs.tf", "https://logs.tf", t("Every log, and the raw server log behind it, for what drops.tf has not got yet: a game just played, or a log it lost.")],
    ["trends.tf", "https://trends.tf", t("Which matches you played, which logs belong together, and each player's ETF2L career.")],
    ["ETF2L", "https://etf2l.org", t("Officials, divisions, rosters, transfers, seasons and fixtures.")],
    ["demos.tf", "https://demos.tf", t("SourceTV demos, downloaded only when you ask. Its files go back to 14 October 2019; older demos are listed but gone.")],
    ["more.tf", "https://more.tf", t("The top-down map images under the kill map, and logs while logs.tf is resting.")],
  ];
  return (
    <div className="panel">
      <h2>{t("Data sources")}</h2>
      <dl className="kv sources-list">
        {rows.map(([name, href, what]) => (
          <Fragment key={name}>
            <dt>
              <button className="linkish" onClick={() => void api.openExternal(href)}>
                {name} ↗
              </button>
            </dt>
            <dd>{what}</dd>
          </Fragment>
        ))}
      </dl>
      <p className="hint" style={{ marginTop: 10 }}>{t("Your own demos are read on your PC and never uploaded.")}</p>
    </div>
  );
}

/**
 * Which version this is, and a way to ask for a newer one. The app looks
 * once at startup on its own; this is for when you have heard there is a new
 * release and do not want to restart to find out.
 */
function UpdatesPanel({ version }: { version: string }) {
  const u = useUpdate();
  return (
    <div className="panel">
      <h2>{tr("Updates")}</h2>
      <div className="row" style={{ marginTop: 10, gap: 12, flexWrap: "wrap", alignItems: "center" }}>
        <span>{tx("You have {0}", { "0": <strong>{tr("Version {version}", { version: version })}</strong> })}</span>
        <button onClick={() => void checkForUpdate(true)} disabled={u.state === "checking" || u.state === "downloading"}>
          {u.state === "checking" ? tr("Checking…") : tr("Check for updates")}
        </button>
      </div>
      <div className="update-status" aria-live="polite">
        {u.state === "checking" && (
          <p className="hint"><span className="spin" aria-hidden /> {tr("Asking GitHub for the newest release…")}</p>
        )}
        {u.state === "current" && <p className="hint">{tr("Up to date: this is the newest release.")}</p>}
        {u.state === "available" && (
          <>
            <p>{tr("Version {version} is available.", { version: u.version })}</p>
            <button className="primary" style={{ marginTop: 8 }} onClick={() => void installUpdate()}>{u.manual ? tr("Open the download page") : tr("Download and install")}</button>
          </>
        )}
        {u.state === "downloading" && (
          <>
            <p className="hint">
              <span className="spin" aria-hidden /> {tr("Downloading {version}…", { version: u.version })}
              {u.total ? ` ${Math.round((u.got / u.total) * 100)}%` : ""}
            </p>
            <span className="dl-bar" aria-hidden>
              <span className={u.total ? "dl-fill" : "dl-fill dl-unknown"} style={u.total ? { width: `${(u.got / u.total) * 100}%` } : undefined} />
            </span>
          </>
        )}
        {u.state === "ready" && (
          <>
            <p>{tr("Version {version} is installed. Restart to use it.", { version: u.version })}</p>
            <button className="primary" style={{ marginTop: 8 }} onClick={() => void restartNow()}>{tr("Restart now")}</button>
          </>
        )}
        {u.state === "failed" && <p className="error">{u.message}</p>}
        {!inTauri && <p className="hint">{tr("Updates are checked in the installed app, not in a browser preview.")}</p>}
      </div>
    </div>
  );
}

/** What changed in each release, newest first; the same notes as on GitHub. */
function ChangelogPanel() {
  return (
    <div className="panel">
      <h2>{tr("Changelog")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("What changed in each release, newest first. The notes are in English.")}</p>
      <div className="changelog">
        {RELEASES.map((r, i) => (
          <details key={r.version} open={i === 0} className="changelog-entry">
            <summary>{tr("Version {version}", { version: r.version })}</summary>
            <Markdown text={r.body} />
          </details>
        ))}
      </div>
    </div>
  );
}

/**
 * Everything that went wrong, and a button that turns it into a message.
 *
 * A tester with a problem had nothing to send but a screenshot of a black
 * window. This is the thing to paste instead.
 */
function ProblemsPanel({ version }: { version: string }) {
  const problems = useProblems();
  const [copied, setCopied] = useState(false);
  useEffect(() => markProblemsSeen(), [problems.length]);

  if (problems.length === 0) {
    return (
      <div className="panel">
        <h2>{t("Problems")}</h2>
        <p className="hint" style={{ marginTop: 6 }}>{tr("Nothing has gone wrong since the app started.")}</p>
      </div>
    );
  }
  return (
    <div className="panel">
      <h2>{t("Problems")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        {tx("{problems} since the app started. Copy this into Discord if you are reporting something.", { problems: problems.length })}</p>
      <div className="row" style={{ marginTop: 12 }}>
        <button
          onClick={() => {
            void navigator.clipboard?.writeText(report({ version }));
            setCopied(true);
            window.setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? tr("Copied") : tr("Copy report")}
        </button>
        <button className="linkish" onClick={() => clearProblems()}>{tr("Clear")}</button>
      </div>
      <div className="table-wrap" style={{ marginTop: 14 }}>
        <table className="match-table">
          <thead>
            <tr>
              <th>{tr("When")}</th>
              <th>{tr("What")}</th>
              <th>{tr("Why")}</th>
            </tr>
          </thead>
          <tbody>
            {problems.slice(0, 50).map((p) => (
              <tr key={p.id}>
                <td className="muted nowrap">{new Date(p.at).toLocaleTimeString()}</td>
                <td className="nowrap">
                  {tr(p.what)}
                  {p.count > 1 && <span className="muted"> ×{p.count}</span>}
                </td>
                <td className="muted">{p.message}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/**
 * Pick a palette (Q10).
 *
 * Every theme is dark. The app is built from translucent light tints over
 * dark surfaces, so a light theme is not a swap of this list — it is its
 * own piece of work, and half-doing it would look worse than not offering
 * it. RED and BLU are never themed: they mean something in TF2, and a
 * scoreboard that recolours them is lying about which team is which.
 */
/**
 * Which language the app speaks (Q13). A missing translation shows the
 * English rather than a blank, so a partly translated language is usable
 * from the first entry a volunteer adds.
 *
 * Below the choice, the way in for volunteers: save a language as a `.lang`
 * file, edit it, press Reload, send it back. Files in that folder beat the
 * built-in text, so a fix shows the moment it is made, not at the next
 * release.
 */
function LanguagePanel() {
  const lang = useLanguage();
  const files = userLanguageFiles();
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = (what: () => Promise<void>) => async () => {
    setBusy(true);
    setNote(null);
    try {
      await what();
    } catch (e) {
      setNote(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  // English saves the blank template as new.lang: the start of a language
  // the app does not have yet.
  const edit = run(async () => {
    const saved = await api.saveLanguageFile(lang === "en" ? "new" : lang, languageFile(lang));
    setNote(
      !saved.created
        ? tr("{0} is in the language folder. Edit it, then press Reload.", { "0": saved.path })
        : lang === "en"
          ? tr("Saved {0}. Rename it to your language's code, like pl.lang, fill in the right-hand side, then press Reload.", { "0": saved.path })
          : tr("Saved {0}. Edit it, then press Reload.", { "0": saved.path }),
    );
    await api.revealPath(saved.path);
  });
  const open = run(async () => {
    const found = await loadUserLanguages();
    if (found) await api.revealPath(found.dir);
  });
  const reload = run(async () => {
    const found = await loadUserLanguages();
    setNote(found ? tr("Language files read: {n}.", { n: found.files.length }) : tr("The language folder could not be read."));
  });

  return (
    <div className="panel">
      <h2>{t("Language")}</h2>
      <div className="row" style={{ marginTop: 10, gap: 8, flexWrap: "wrap" }}>
        {allLanguages().map((l) => (
          <button
            key={l.id}
            className={lang === l.id ? "primary lang-choice" : "lang-choice"}
            aria-pressed={lang === l.id}
            onClick={() => setLanguage(l.id)}
          >
            {l.name}
          </button>
        ))}
      </div>
      {lang !== "en" && (
        <p className="hint" style={{ marginTop: 8 }}>
          {t("Draft translations, not yet checked by a native speaker. Anything untranslated is shown in English.")}
        </p>
      )}

      <h3 style={{ marginTop: 18 }}>{tr("Translation files")}</h3>
      <p className="hint" style={{ marginTop: 6 }}>
        {tr("Fix a line or start a new language, then press Reload to see it.")}
      </p>
      <div className="row" style={{ marginTop: 10, gap: 8, flexWrap: "wrap" }}>
        <button onClick={() => void edit()} disabled={busy}>
          {lang === "en" ? tr("Start a new language") : tr("Edit this language")}
        </button>
        <button onClick={() => void open()} disabled={busy}>{tr("Show the language folder")}</button>
        <button onClick={() => void reload()} disabled={busy}>{tr("Reload")}</button>
      </div>
      {note && <p className="hint" style={{ marginTop: 8 }}>{note}</p>}
      {files.length > 0 && (
        <ul className="hint" style={{ marginTop: 8 }}>
          {files.map((f) => (
            <li key={f.id}>
              <code>{f.id}.lang</code>{" "}
              {f.added
                ? tr("adds {name}: {n} lines", { name: f.name, n: f.lines })
                : f.lines === 0
                  ? tr("as shipped, nothing changed")
                  : tr("{n} lines over the built-in {name}", { name: f.name, n: f.lines })}
              {f.errors.length > 0 && (
                <span className="error">
                  {" · "}
                  {tr("{n} lines could not be read, first {first}", { n: f.errors.length, first: f.errors[0] })}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * The name a player goes by on every page: the one from the log, or their
 * ETF2L name. The ETF2L names come from official rosters the app has
 * already read, so the choice costs no requests to ETF2L.
 */
function NamesPanel() {
  const { source, count } = useNameSource();
  return (
    <div className="panel">
      <h2>{tr("Player names")}</h2>
      <div className="row" style={{ marginTop: 10, gap: 8, flexWrap: "wrap" }}>
        <button className={source === "log" ? "primary" : undefined} aria-pressed={source === "log"} onClick={() => setNameSource("log")}>
          {tr("As in the log")}
        </button>
        <button className={source === "etf2l" ? "primary" : undefined} aria-pressed={source === "etf2l"} onClick={() => setNameSource("etf2l")}>
          {tr("ETF2L names")}
        </button>
      </div>
      <p className="hint" style={{ marginTop: 8 }}>{tr("{n} players have an ETF2L name.", { n: count.toLocaleString() })}</p>
    </div>
  );
}

function ThemePanel() {
  const theme = useTheme();
  const themes = useThemes();
  // The theme being made or edited; null while the editor is shut.
  const [editing, setEditing] = useState<{ id?: string; name: string; base: Base } | null>(null);
  const [code, setCode] = useState("");
  const [codeError, setCodeError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  // Show the palette as it is edited; put the chosen theme back on closing.
  useEffect(() => {
    previewTheme(editing ? editing.base : null);
  }, [editing]);
  useEffect(() => () => previewTheme(null), []);

  const startFrom = (th: Theme, id?: string) => setEditing({ id, name: id ? th.name : t("{0}, my version", { "0": th.name }), base: { ...th.base } });
  const copy = async (th: Theme) => {
    await navigator.clipboard?.writeText(themeCode(th));
    setCopied(th.id);
    window.setTimeout(() => setCopied(null), 1600);
  };
  const add = () => {
    const got = readThemeCode(code);
    if (!got) {
      setCodeError(t("That is not a theme code. One starts with hltheme:"));
      return;
    }
    setCodeError(null);
    setCode("");
    saveCustomTheme(got.name, got.base);
  };

  const b = editing?.base;
  const lowContrast = b ? contrast(b.text, b.bg) < 7 || contrast(b.text, b.panel) < 4.5 : false;
  const light = b ? luminance(b.bg) > 0.25 : false;
  const field = (key: keyof Base, label: string) =>
    b && (
      <label className="th-field">
        <span>{label}</span>
        <input type="color" value={b[key]} onChange={(e) => setEditing({ ...editing!, base: { ...b, [key]: e.target.value } })} />
        <input
          type="text"
          value={b[key]}
          maxLength={7}
          onChange={(e) => {
            const v = e.target.value.trim();
            if (/^#[0-9a-f]{6}$/i.test(v)) setEditing({ ...editing!, base: { ...b, [key]: v.toLowerCase() } });
          }}
          aria-label={label}
        />
      </label>
    );

  return (
    <div className="panel">
      <h2>{t("Theme")}</h2>
      <div className="theme-grid">
        {themes.map((th) => (
          <div key={th.id} className={theme === th.id && !editing ? "theme-card active" : "theme-card"}>
            <button className="theme-pick" onClick={() => setTheme(th.id)} aria-pressed={theme === th.id}>
              <span className="theme-swatch" aria-hidden>
                <i style={{ background: th.base.bg }} />
                <i style={{ background: th.base.panel }} />
                <i style={{ background: th.base.accent }} />
              </span>
              <span className="theme-name">{th.name}</span>
              <span className="theme-hint muted">{th.custom ? t("your own") : th.hint ? tr(th.hint) : ""}</span>
            </button>
            {th.custom && (
              <span className="theme-tools">
                <button className="linkish" onClick={() => startFrom(th, th.id)}>
                  {t("Edit")}
                </button>
                <button className="linkish" onClick={() => void copy(th)}>
                  {copied === th.id ? t("Copied") : t("Share")}
                </button>
                <button className="linkish" onClick={() => deleteCustomTheme(th.id)}>
                  {t("Delete")}
                </button>
              </span>
            )}
          </div>
        ))}
        {!editing && (
          <button className="theme-card theme-new" onClick={() => startFrom(themeById(theme))}>
            <span className="theme-new-plus" aria-hidden>
              +
            </span>
            <span className="theme-name">{t("Make your own")}</span>
            <span className="theme-hint muted">{t("starts from the theme you use now")}</span>
          </button>
        )}
      </div>

      {editing && b && (
        <section className="th-editor">
          <div className="th-row">
            <label className="th-name">
              <span>{t("Name")}</span>
              <input type="text" value={editing.name} maxLength={40} onChange={(e) => setEditing({ ...editing, name: e.target.value })} />
            </label>
            <span className="hint">{t("The whole app shows the theme as you change it.")}</span>
          </div>
          <div className="th-fields">
            {field("bg", t("Background"))}
            {field("panel", t("Panels"))}
            {field("text", t("Text"))}
            {field("accent", t("Accent"))}
          </div>
          {lowContrast && <p className="warn-text">{t("The text is hard to read on this background: make one lighter or the other darker.")}</p>}
          {light && <p className="warn-text">{t("The app is built for dark backgrounds; a light one will look washed out in places.")}</p>}
          <div className="th-actions">
            <button
              onClick={() => {
                saveCustomTheme(editing.name, b, editing.id);
                setEditing(null);
              }}
            >
              {editing.id ? t("Save changes") : t("Save theme")}
            </button>
            <button className="linkish" onClick={() => setEditing(null)}>
              {t("Cancel")}
            </button>
          </div>
        </section>
      )}

      <div className="th-import">
        <input
          type="text"
          value={code}
          placeholder={t("Paste a theme code someone shared (hltheme:…)")}
          onChange={(e) => {
            setCode(e.target.value);
            setCodeError(null);
          }}
          onKeyDown={(e) => e.key === "Enter" && code.trim() && add()}
        />
        <button disabled={!code.trim()} onClick={add}>
          {t("Add theme")}
        </button>
      </div>
      {codeError && <p className="error">{codeError}</p>}
    </div>
  );
}

/** How big the whole interface is drawn: for 2K and 4K screens (Flashy). */
function ScalePanel() {
  const scale = useScale();
  return (
    <div className="panel">
      <h2>{t("Interface size")}</h2>
      <p className="hint">{t("Makes everything in the app bigger or smaller: text, maps and charts together. Useful on 2K and 4K screens. Ctrl + and Ctrl − change it from anywhere, Ctrl 0 resets it.")}</p>
      <div className="scale-row">
        <button className="km-chip scale-step" onClick={() => stepScale(-1)} disabled={scale <= SCALES[0]} aria-label={t("Smaller")}>
          −
        </button>
        <div className="km-seg scale-seg" role="radiogroup" aria-label={t("Interface size")}>
          {SCALES.map((s) => (
            <button key={s} role="radio" aria-checked={Math.abs(scale - s) < 0.001} className={Math.abs(scale - s) < 0.001 ? "on" : undefined} onClick={() => setScale(s)}>
              {Math.round(s * 100)}%
            </button>
          ))}
        </div>
        <button className="km-chip scale-step" onClick={() => stepScale(1)} disabled={scale >= SCALES[SCALES.length - 1]} aria-label={t("Bigger")}>
          +
        </button>
        {Math.abs(scale - DEFAULT_SCALE) > 0.001 && (
          <button className="km-chip" onClick={() => setScale(DEFAULT_SCALE)}>
            {t("Reset")}
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * Copies of the database. One is taken before every sync and rebuild, five
 * are kept, and this is where to check they exist — the file holds every log,
 * demo index and rating, and nothing else here can rebuild it from nothing.
 */
function BackupsPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["backups"], queryFn: api.listBackups });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  /** A name that says what it is and when, so a folder of them sorts. */
  function suggestedName() {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `flashwave-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}.sqlite3`;
  }

  async function saveElsewhere() {
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      const b = await api.saveBackupAs(suggestedName());
      if (b) setSaved(b.path);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function backupNow() {
    setBusy(true);
    setError(null);
    try {
      await api.backupNow();
      await qc.invalidateQueries({ queryKey: ["backups"] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const items = q.data?.items ?? [];
  return (
    <div className="panel">
      <h2>{t("Backups")}</h2>
      <p className="hint">{tx("Taken before every sync and rebuild; the newest five are kept. To restore one, close the app and rename it over {0}.", { "0": <code>{tr("hl.sqlite3")}</code> })}</p>
      <p className="hint">
        {tx("{0} Keep a copy on another drive.", { "0": <strong>{tr("Uninstalling can delete these.")}</strong> })}</p>
      <div className="row" style={{ marginTop: 14 }}>
        <button onClick={() => void backupNow()} disabled={busy}>
          {busy ? tr("Copying…") : tr("Back up now")}
        </button>
        <button onClick={() => void saveElsewhere()} disabled={busy}>
          {busy ? tr("Copying…") : tr("Save a copy elsewhere…")}
        </button>
      </div>
      {saved && (
        <p className="hint" style={{ marginTop: 10 }}>{tx("Written to {0}.", { "0": <code>{saved}</code> })}</p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}
      {items.length === 0 ? (
        <p className="hint" style={{ marginTop: 14 }}>{tr("No copies yet. The next sync makes one.")}</p>
      ) : (
        <dl className="kv" style={{ marginTop: 16 }}>
          <dt>{tr("Folder")}</dt>
          <dd className="path-row">
            <code>{q.data?.dir}</code>
            <button className="linkish" onClick={() => void api.revealPath(q.data!.dir)}>{tr("Show in Explorer")}</button>
          </dd>
          {items.map((b) => (
            <Fragment key={b.path}>
              <dt>{formatDate(b.madeAt, true)}</dt>
              <dd>{tx("{0} MB", { "0": (b.bytes / 1_000_000).toFixed(0) })}</dd>
            </Fragment>
          ))}
        </dl>
      )}
    </div>
  );
}

/** Demo index counts, and a rescan for when you have just recorded. */
function DemosPanel() {
  const qc = useQueryClient();
  const stats = useQuery({ queryKey: ["demo_stats"], queryFn: api.demoStats });
  const [scan, setScan] = useState<{ busy: boolean; result: DemoIndexSummary | null; error: string | null }>({
    busy: false,
    result: null,
    error: null,
  });

  async function rescan() {
    setScan({ busy: true, result: null, error: null });
    try {
      const result = await api.scanDemos();
      setScan({ busy: false, result, error: null });
      void qc.invalidateQueries({ queryKey: ["demo_stats"] });
      void qc.invalidateQueries({ queryKey: ["matches"] });
      void qc.invalidateQueries({ queryKey: ["match"] });
    } catch (e) {
      setScan({ busy: false, result: null, error: errorMessage(e) });
    }
  }

  const s = stats.data;
  return (
    <div className="panel">
      <h2>{t("Demos")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tx("Demos in {0}, {1} and {2}, matched to logs by map and time.", { "0": <code>{tr("tf")}</code>, "1": <code>{tr("tf/demos")}</code>, "2": <code>{tr("tf/demos/stv")}</code> })}</p>
      {s && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>{tr("Demos found")}</dt>
          <dd>
            {s.demos}
            {s.stv > 0 && tr(" ({stv} STV)", { stv: s.stv })}
          </dd>
          <dt>{tr("Linked")}</dt>
          <dd>
            {tx("{linked} demos, covering {matchesWithDemo} matches", { linked: s.linked, matchesWithDemo: s.matchesWithDemo })}</dd>
          <dt>{tr("Markers")}</dt>
          <dd>{tx("{markers} killstreak markers from Demo Support", { markers: s.markers })}</dd>
          <dt title={tr("Every demo the app reads is kept as a compact timeline, so later versions can still learn from it once the file is gone.")}>{tr("Kept")}</dt>
          <dd>
            {s.timelines === 0
              ? <span className="muted">{tr("None yet: the next sync keeps each demo it reads.")}</span>
              : tr("{timelines} demos, {size}", { timelines: s.timelines, size: `${Math.max(1, Math.round(s.timelineBytes / 1e6))} MB` })}
            {s.timelinesFileGone > 0 && <span className="muted">{tr(" · {gone} no longer on disk", { gone: s.timelinesFileGone })}</span>}
          </dd>
        </dl>
      )}
      <DemoFolders onScanned={(result) => setScan({ busy: false, result, error: null })} />
      <div className="row" style={{ marginTop: 14 }}>
        <button onClick={() => void rescan()} disabled={scan.busy}>
          {scan.busy ? tr("Scanning…") : tr("Rescan demos")}
        </button>
        {scan.result && (
          <span className="hint">{tx("Scanned {scanned}, linked {demosLinked}.", { scanned: scan.result.scanned, demosLinked: scan.result.demosLinked })}</span>
        )}
      </div>
      {scan.error && <p className="error" style={{ marginTop: 10 }}>{scan.error}</p>}
    </div>
  );
}

/**
 * Other folders with demos in them (Flashy): an archive on another drive,
 * demoreviews folders. Scanned with their subfolders, and only ever read:
 * the clean-up after a sync deletes demos the app downloaded, never these.
 */
function DemoFolders({ onScanned }: { onScanned: (s: DemoIndexSummary) => void }) {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["demo_folders"], queryFn: api.getDemoFolders });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const folders = q.data ?? [];

  async function save(next: string[]) {
    setBusy(true);
    setError(null);
    try {
      const result = await api.setDemoFolders(next);
      onScanned(result);
      for (const key of [["demo_folders"], ["demo_stats"], ["matches"], ["match"]]) void qc.invalidateQueries({ queryKey: key });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function add() {
    const picked = inTauri ? await open({ directory: true, title: tr("Choose a folder with demos in it") }) : window.prompt(tr("Path to a folder with demos in it"));
    if (typeof picked === "string" && picked.length > 0 && !folders.includes(picked)) await save([...folders, picked]);
  }

  return (
    <div className="demo-folders">
      <h3>{tr("Other demo folders")}</h3>
      <p className="hint">{tr("An archive on another drive, demo review folders: every demo in them, subfolders included, is matched to its match like those in tf/demos. They are only read, never moved or deleted.")}</p>
      {folders.length > 0 && (
        <ul>
          {folders.map((f) => (
            <li key={f}>
              <code>{f}</code>
              <button className="linkish" onClick={() => void api.revealPath(f)}>
                {tr("Show in Explorer")}
              </button>
              <button className="linkish" disabled={busy} onClick={() => void save(folders.filter((x) => x !== f))}>
                {tr("Remove")}
              </button>
            </li>
          ))}
        </ul>
      )}
      <button onClick={() => void add()} disabled={busy}>
        {busy ? tr("Scanning…") : tr("Add a folder")}
      </button>
      {error && <p className="error">{error}</p>}
    </div>
  );
}

/**
 * The STV demos the app downloaded, and getting rid of them (PLAN Q23).
 *
 * A demo is read once and never needed again, and each is around 80 MB.
 * Deleting one removes the file and nothing else: the row stays, with the
 * demos.tf id it can be fetched back from, and everything already derived
 * from it -- aim, deaths, routes -- was derived and stored while the file
 * was here. The match page does not change.
 *
 * Two things this panel never offers, and says so, because the reasonable
 * guess is that it would:
 *
 * - **Your own POV demos.** TF2 wrote those, in your folder, and some are
 *   the only copy of a match older than this app. They are also the only
 *   source of your own view angles as you made them. 3.5 GB of
 *   irreplaceable against 600 MB of replaceable is the wrong trade.
 * - **An STV you already had.** No demos.tf id means no way back, which
 *   makes it your file too.
 */
function DownloadedDemosPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["downloaded_demos"], queryFn: api.downloadedDemos });
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<Cleaned | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  async function remove(only: number | null) {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const r = await api.deleteDownloadedDemos(only, false);
      setDone(r);
      await qc.invalidateQueries({ queryKey: ["downloaded_demos"] });
      await qc.invalidateQueries({ queryKey: ["demo_stats"] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      setConfirming(false);
    }
  }

  const items = q.data ?? [];
  const total = items.reduce((n, d) => n + d.sizeBytes, 0);
  const ready = items.filter((d) => d.read);
  const readySize = ready.reduce((n, d) => n + d.sizeBytes, 0);
  const gb = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`);

  return (
    <div className="panel">
      <h2>{t("Downloaded demos")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("STV demos the app fetched. Each is read once; after that it is only taking up space, and it can be downloaded again whenever it is wanted. Your own recordings are never touched.")}</p>

      {items.length === 0 ? (
        <p className="hint" style={{ marginTop: 14 }}>
          {q.isPending ? tr("Looking…") : tr("None downloaded yet.")}
        </p>
      ) : (
        <>
          <dl className="kv" style={{ marginTop: 14 }}>
            <dt>{tr("Held")}</dt>
            <dd>
              {tx("{items} demo{1}, {2}", { "1": items.length === 1 ? "" : "s", "2": gb(total), items: items.length })}
            </dd>
            <dt>{tr("Finished with")}</dt>
            <dd>
              {tx("{ready} of {items}, {2}", { "2": gb(readySize), ready: ready.length, items: items.length })}
            </dd>
          </dl>

          <div className="table-wrap" style={{ marginTop: 12 }}>
            <table className="match-table">
              <thead>
                <tr>
                  <th>{tr("Demo")}</th>
                  <th className="num">{tr("Size")}</th>
                  <th>{tr("State")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {items.map((d) => (
                  <tr key={d.demoId}>
                    <td className="nowrap">{d.map ?? d.fileName}</td>
                    <td className="num">{gb(d.sizeBytes)}</td>
                    <td>
                      {d.read ? (
                        <span className="muted">{tr("read")}</span>
                      ) : (
                        <span className="muted" title={tr("The app has not finished reading this one, so it is kept.")}>{tr("still needed")}</span>
                      )}
                    </td>
                    <td>
                      <button className="linkish" disabled={busy || !d.read} onClick={() => void remove(d.demoId)}>{tr("Delete")}</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="row" style={{ marginTop: 14 }}>
            {confirming ? (
              <>
                <span className="hint">{tx("Delete {ready} demo{1} ({2})?", { "1": ready.length === 1 ? "" : "s", "2": gb(readySize), ready: ready.length })}</span>
                <button className="primary" disabled={busy} onClick={() => void remove(null)}>
                  {busy ? tr("Deleting…") : tr("Yes, delete")}
                </button>
                <button disabled={busy} onClick={() => setConfirming(false)}>{tr("Cancel")}</button>
              </>
            ) : (
              <button disabled={busy || ready.length === 0} onClick={() => setConfirming(true)}>{tr("Delete all read demos")}</button>
            )}
          </div>
        </>
      )}

      <AutoDelete />

      {done && (
        <p className="hint" style={{ marginTop: 10 }}>{tx("Deleted {deleted}, freeing {1}.{2}", { "1": gb(done.bytes), "2": done.skipped > 0 && tr(" {skipped} kept: not finished reading them yet.", { skipped: done.skipped }), deleted: done.deleted })}
        </p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}
    </div>
  );
}

/**
 * Delete each downloaded demo as soon as the app is finished with it.
 *
 * Off by default, and that is the considered choice rather than caution.
 * The aim pass has had eleven versions, and every one of them re-read every
 * demo to store something better than the version before. A deleted demo
 * cannot take part in the twelfth: its rows stay frozen at whatever the
 * pass knew when the file was still there. That is a fine trade if disk is
 * the thing you are short of, and a bad one otherwise, so it is asked
 * rather than assumed.
 */
function AutoDelete() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["auto_delete_demos"], queryFn: api.autoDeleteDemos });
  const [error, setError] = useState<string | null>(null);

  async function set(on: boolean) {
    setError(null);
    try {
      await api.setAutoDeleteDemos(on);
      await qc.invalidateQueries({ queryKey: ["auto_delete_demos"] });
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  return (
    <div style={{ marginTop: 14 }}>
      <label className="row" style={{ gap: 8, cursor: "pointer", fontSize: "0.9rem" }}>
        <input
          type="checkbox"
          checked={q.data ?? false}
          disabled={q.isPending}
          onChange={(e) => void set(e.target.checked)}
        />
        <span>{tr("Delete each demo once it has been read")}</span>
      </label>
      <p className="hint" style={{ marginTop: 6 }}>{tr("Runs after every sync. A later version of the rating re-reads demos to improve what it stores, and a deleted one cannot take part until it is downloaded again.")}</p>
      {error && <p className="error">{error}</p>}
    </div>
  );
}

/** What ETF2L added: officials, and how every match was classified. */
function Etf2lPanel() {
  const q = useQuery({ queryKey: ["context_counts"], queryFn: api.contextCounts });
  const c = q.data;
  return (
    <div className="panel">
      <h2>{t("ETF2L and match types")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("Your ETF2L results are fetched on every sync. Each Highlander match you played is then sorted into an official, a scrim (most of your side are regular teammates or your ETF2L roster) or a pug.")}</p>
      {c && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>{tr("ETF2L player")}</dt>
          <dd>
            {c.etf2lPlayer ? (
              <button className="linkish" onClick={() => void api.openExternal(`https://etf2l.org/forum/user/${c.etf2lPlayer}/`)}>
                #{c.etf2lPlayer} ↗
              </button>
            ) : (
              <span className="muted">{tr("not found yet — sync to look it up")}</span>
            )}
          </dd>
          <dt>{tr("Officials")}</dt>
          <dd>
            {tx("{officials} logs{1}", { "1": c.rosterOfficials > 0 && (
              <span className="muted">{" "}{tx("· {rosterOfficials} found by roster that trends.tf had not tagged", { rosterOfficials: c.rosterOfficials })}</span>
            ), officials: c.officials })}
          </dd>
          <dt>{tr("Scrims")}</dt>
          <dd>{c.scrims}</dd>
          <dt>{tr("Pugs")}</dt>
          <dd>{c.pugs}</dd>
          <dt>{tr("Last fetched")}</dt>
          <dd>
            {c.lastFetch ? (
              <>
                {new Date(c.lastFetch * 1000).toLocaleString()}{" "}
                <span className="muted">{tx("({etf2lMatches} ETF2L matches stored)", { etf2lMatches: c.etf2lMatches })}</span>
              </>
            ) : (
              <span className="muted">{tr("never")}</span>
            )}
          </dd>
        </dl>
      )}
    </div>
  );
}

/** Raw server logs: where every kill, with its time and position, comes from. */
function RawlogPanel() {
  const q = useQuery({ queryKey: ["rawlog_stats"], queryFn: api.rawlogStats });
  const s = q.data;
  return (
    <div className="panel">
      <h2>{t("Raw logs")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("The server log behind each logs.tf page. It has every kill with its time, both classes and where both players stood, so kills are valued one by one: by the victim's class, the map, and whether they were defending. Fetched on every sync.")}</p>
      {s && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>{tr("Stored")}</dt>
          <dd>
            {tx("{0} matches {1}", { "0": s.stored.toLocaleString(), "1": <span className="muted">{tx("({0} MB)", { "0": (s.bytes / 1e6).toFixed(0) })}</span> })}
          </dd>
          <dt>{tr("Kills")}</dt>
          <dd>{s.kills.toLocaleString()}</dd>
          <dt>{tr("Still to fetch")}</dt>
          <dd>
            {s.pending}
            {s.pending > 0 && <span className="muted">{" "}{tr("· retried on the next sync")}</span>}
          </dd>
          {s.missing > 0 && (
            <>
              <dt>{tr("Not on logs.tf")}</dt>
              <dd>
                {s.missing} <span className="muted">{tr("· these use logs.tf's totals instead")}</span>
              </dd>
            </>
          )}
        </dl>
      )}
    </div>
  );
}
