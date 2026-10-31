/**
 * How long a player's profile header is kept before it is asked for again
 * (Flashy: "see if we can cache stuff to lessen the load"). The backend
 * keeps ETF2L's page a week; this only spares re-reading the catalogue and
 * re-asking a site that is down each time a profile is opened again.
 */
export const PROFILE_KEEP_MS = 10 * 60_000;
