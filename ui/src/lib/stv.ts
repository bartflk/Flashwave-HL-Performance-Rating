import { useState } from "react";
import { api } from "../api/client";
import { errorMessage, type MatchDetail } from "../api/types";
import { beginDownload, failDownload, useDownload } from "./downloads";
import { t } from "./i18n";

/**
 * Whether a match has an STV demo to read, and how to get one: shared by
 * every panel that needs it, so a download started from any of them shows
 * everywhere (the download itself is followed app-wide).
 *
 * A deleted STV still counts: what was read from it -- the timeline --
 * is kept, and every STV-only panel reads that, not the file.
 */
/**
 * Q62: demos.tf's oldest file still there -- demo 314574, 14 October 2019.
 * Every demo listed before it is marked deleted, with no link. Kept in step
 * with `hl_ingest::demostf::OLDEST_AVAILABLE` (and its three-hour slack: a
 * match is stamped up to that long before its demo).
 */
export const DEMOSTF_OLDEST = 1_571_085_763 - 3 * 3600;

/** A match older than any demo demos.tf still has. */
export function beforeDemosTf(playedAt: number | null): boolean {
  return playedAt !== null && playedAt < DEMOSTF_OLDEST;
}

/** Why there is no STV to download, in one line. */
export function noStvReason(d: MatchDetail): string {
  return beforeDemosTf(d.playedAt)
    ? t("demos.tf's demos go back to 14 October 2019; this match is older.")
    : t("demos.tf has no STV demo for this match.");
}

export function useStv(d: MatchDetail) {
  const has = d.demos.some((x) => x.kind === "stv");
  const canFetch = !has && d.demosTfId !== null && !beforeDemosTf(d.playedAt);
  const download = useDownload(d.logId);
  const [error, setError] = useState<string | null>(null);
  async function fetch() {
    setError(null);
    beginDownload(d.logId, `${d.map ?? "this match"}, log ${d.logId}`);
    try {
      await api.fetchStv(d.logId);
    } catch (e) {
      setError(errorMessage(e));
      failDownload(d.logId, errorMessage(e));
    }
  }
  return { has, canFetch, download, error, fetch };
}
