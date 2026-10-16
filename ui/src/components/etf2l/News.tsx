import { Fragment, useMemo, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { NewsItem } from "../../api/types";
import { errorMessage } from "../../api/types";
import { formatDate } from "../../lib/format";
import { openTeam } from "../../lib/goto";
import { t } from "../../lib/i18n";

/**
 * ETF2L's Highlander news, read in the app (Q74; Flashy: "if it's available
 * through the API I prefer it in the app"). The posts come from ETF2L's
 * WordPress as HTML; the page is built from that HTML element by element,
 * keeping text, headings, lists, quotes, tables, links and ETF2L's own
 * pictures, and dropping everything else. Nothing of the post's is inserted
 * as raw HTML, and nothing in it runs.
 */

export function useNews() {
  return useQuery({ queryKey: ["etf2l_news"], queryFn: api.getNews, staleTime: 10 * 60_000 });
}

// ---- The list --------------------------------------------------------------

export function NewsList({ onPost }: { onPost: (id: number) => void }) {
  const q = useNews();
  const items = q.data ?? [];
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
        <h1>{t("News")}</h1>
      </header>
      {q.isPending && <p className="hint">{t("Loading the news…")}</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data && items.length === 0 && <p className="hint">{t("No news could be read from ETF2L just now.")}</p>}
      {items.length > 0 && (
        <div className="np-grid">
          {items.map((n, i) => (
            <NewsCard key={n.id} n={n} lead={i === 0} onOpen={() => onPost(n.id)} />
          ))}
        </div>
      )}
      <p className="hint">
        {t("Older posts are on")}{" "}
        <a
          href="https://etf2l.org/"
          onClick={(e) => {
            e.preventDefault();
            void api.openExternal("https://etf2l.org/");
          }}
        >
          etf2l.org ↗
        </a>
      </p>
    </>
  );
}

export function NewsCard({ n, lead, onOpen }: { n: NewsItem; lead?: boolean; onOpen: () => void }) {
  return (
    <button className={lead ? "np-card lead" : "np-card"} onClick={onOpen}>
      {n.image ? <img src={n.image} alt="" loading="lazy" /> : <span className="np-noimg" aria-hidden />}
      <span className="np-card-text">
        <span className="np-date">{formatDate(n.time, true)}</span>
        <strong>{n.title}</strong>
        <span className="np-excerpt">{n.excerpt}</span>
      </span>
    </button>
  );
}

// ---- One post --------------------------------------------------------------

export function NewsReader({ id }: { id: number }) {
  const q = useQuery({ queryKey: ["etf2l_news_post", id], queryFn: () => api.getNewsPost(id), staleTime: 30 * 60_000 });
  const body = useMemo(() => (q.data ? build(q.data.html) : null), [q.data]);
  if (q.isPending) return <p className="hint">{t("Loading the post…")}</p>;
  if (q.isError) return <p className="error">{errorMessage(q.error)}</p>;
  if (!q.data) return <p className="hint">{t("That post could not be read from ETF2L.")}</p>;
  const p = q.data;
  return (
    <article className="np-post">
      <header className="np-post-head">
        <span className="np-date">{formatDate(p.time, true)}</span>
        <h1>{p.title}</h1>
      </header>
      <div className="np-body">{body}</div>
      <footer className="np-post-foot">
        <button className="km-chip" onClick={() => void api.openExternal(p.link)}>
          {t("Read on ETF2L")} ↗
        </button>
      </footer>
    </article>
  );
}

/** A link's target made absolute; null for anything but http(s). */
function absolute(href: string | null): string | null {
  if (!href) return null;
  try {
    const u = new URL(href, "https://etf2l.org/");
    return u.protocol === "https:" || u.protocol === "http:" ? u.href : null;
  } catch {
    return null;
  }
}

/** Pictures the app may show: ETF2L's own (the app's content policy allows
 *  no other host). Anything else becomes a link to it. */
function showable(src: string): boolean {
  const host = new URL(src).hostname;
  return host === "etf2l.org" || host.endsWith(".etf2l.org");
}

/** A link in a post: a team opens in the app, the rest in the browser. */
function follow(href: string) {
  const team = /^https?:\/\/etf2l\.org\/teams\/(\d+)\/?$/.exec(href);
  if (team) openTeam(Number(team[1]));
  else void api.openExternal(href);
}

/** What an embedded frame is, for the link that stands in for it. */
function embedLabel(src: string): string {
  const host = new URL(src).hostname;
  if (host.includes("google.com")) return t("Open the spreadsheet");
  if (host.includes("youtube") || host.includes("youtu.be")) return t("Watch on YouTube");
  if (host.includes("twitch")) return t("Watch on Twitch");
  return t("Open the embedded page");
}

function build(html: string): ReactNode {
  const doc = new DOMParser().parseFromString(html, "text/html");
  return children(doc.body, "b");
}

function children(el: Node, key: string): ReactNode[] {
  const out: ReactNode[] = [];
  el.childNodes.forEach((n, i) => {
    const r = node(n, `${key}.${i}`);
    if (r !== null && r !== "") out.push(r);
  });
  return out;
}

const SAME = new Set(["p", "ul", "ol", "li", "blockquote", "strong", "em", "b", "i", "u", "code", "pre", "figure", "figcaption", "table", "thead", "tbody", "tr", "td", "th", "sup", "sub", "s"]);

function node(n: Node, key: string): ReactNode {
  if (n.nodeType === Node.TEXT_NODE) return n.textContent;
  if (n.nodeType !== Node.ELEMENT_NODE) return null;
  const el = n as Element;
  const tag = el.tagName.toLowerCase();
  if (tag === "br") return <br key={key} />;
  if (tag === "hr") return <hr key={key} />;
  if (tag === "h1" || tag === "h2") return <h2 key={key}>{children(el, key)}</h2>;
  if (/^h[3-6]$/.test(tag)) return <h3 key={key}>{children(el, key)}</h3>;
  if (tag === "a") {
    const href = absolute(el.getAttribute("href"));
    const inner = children(el, key);
    if (!href) return <Fragment key={key}>{inner}</Fragment>;
    return (
      <a
        key={key}
        href={href}
        onClick={(e) => {
          e.preventDefault();
          follow(href);
        }}
      >
        {inner}
      </a>
    );
  }
  if (tag === "img") {
    const src = absolute(el.getAttribute("src"));
    if (!src) return null;
    const alt = el.getAttribute("alt") ?? "";
    // The little flags next to a name are ETF2L's too, and kept inline.
    const small = Number(el.getAttribute("width") ?? 0) > 0 && Number(el.getAttribute("width")) < 40;
    if (!showable(src)) {
      return (
        <a key={key} className="np-embed" href={src} onClick={(e) => (e.preventDefault(), void api.openExternal(src))}>
          {t("Open the picture")} ↗
        </a>
      );
    }
    return <img key={key} src={src} alt={alt} loading="lazy" className={small || src.includes("/images/flags/") ? "np-flag" : undefined} />;
  }
  if (tag === "iframe") {
    const src = absolute(el.getAttribute("src"));
    if (!src) return null;
    return (
      <a key={key} className="np-embed" href={src} onClick={(e) => (e.preventDefault(), void api.openExternal(src))}>
        {embedLabel(src)} ↗
      </a>
    );
  }
  if (tag === "script" || tag === "style" || tag === "noscript" || tag === "svg" || tag === "form" || tag === "button" || tag === "input") return null;
  if (SAME.has(tag)) {
    const inner = children(el, key);
    if (tag === "p" && inner.length === 0) return null;
    const Tag = tag as keyof React.JSX.IntrinsicElements;
    const cls = el.classList.contains("has-text-align-right") ? "np-right" : el.classList.contains("has-text-align-center") ? "np-center" : undefined;
    const table = tag === "table";
    const made = (
      <Tag key={table ? undefined : key} className={cls}>
        {inner}
      </Tag>
    );
    return table ? (
      <div key={key} className="np-table">
        {made}
      </div>
    ) : (
      made
    );
  }
  // div, span, section and the rest: their contents only.
  return <Fragment key={key}>{children(el, key)}</Fragment>;
}
