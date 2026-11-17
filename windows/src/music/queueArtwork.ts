// SPDX-License-Identifier: GPL-3.0-or-later
import { create } from "zustand";
import type { QueueItem } from "./queueStore";

export interface QueueArtwork {
  image: string;
  link: string;
}
export const artworkKey = (item: QueueItem) => JSON.stringify([item.title, item.artist]);
const normalize = (text: string) =>
  text
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/&/g, " and ")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
// Providers differ on whether featured artists appear in the title or artist
// field. Keep remix/live/version suffixes significant when matching a recording.
const recordingTitle = (text: string, artist: string) =>
  normalize(
    text.replace(/\s*[([]([^\])]+)[)\]]/g, (suffix, credit: string) => {
      if (/^(?:feat\.?|ft\.?|featuring|with)\s+/i.test(credit)) return "";
      // Spotify sometimes includes the complete artist list in parentheses,
      // e.g. "Am I Dreaming (Metro Boomin & A$AP Rocky, Roisee)".
      let rest = ` ${normalize(credit)} `;
      for (const name of artist
        .split(",")
        .map(normalize)
        .filter(Boolean)
        .sort((a, b) => b.length - a.length))
        rest = rest.split(` ${name} `).join(" ");
      return rest.replace(/\band\b/g, "").trim() ? suffix : "";
    }),
  );

// A recording can also appear on a Various Artists compilation. Its title and
// track artist still match, but its cover belongs to a different release.
function isCompilation(artist: unknown, declared: unknown, recordType: unknown) {
  if (declared === true || recordType === "compilation") return true;
  return (
    typeof artist === "string" &&
    /^(?:various artists?|various|v a|multiple artists|verschiedene interpreten|divers artistes|varios artistas|artisti vari)$/.test(
      normalize(artist),
    )
  );
}

export function selectArtwork(item: QueueItem, results: unknown): QueueArtwork | null {
  if (!Array.isArray(results)) return null;
  const artist = normalize(item.artist.split(",")[0]);
  if (!artist) return null;
  for (const entry of results.slice(0, 12)) {
    if (!entry || typeof entry !== "object") continue;
    const row = entry as Record<string, unknown>;
    if (
      typeof row.trackName !== "string" ||
      typeof row.artistName !== "string" ||
      recordingTitle(row.trackName, item.artist) !== recordingTitle(item.title, item.artist) ||
      normalize(row.artistName.split(",")[0]) !== artist ||
      isCompilation(row.collectionArtistName, row.isCompilation, row.collectionType) ||
      (typeof row.collectionArtistId === "number" &&
        typeof row.artistId === "number" &&
        row.collectionArtistId !== row.artistId &&
        typeof row.collectionArtistName !== "string") ||
      typeof row.artworkUrl100 !== "string" ||
      typeof row.trackViewUrl !== "string"
    )
      continue;
    try {
      const image = new URL(row.artworkUrl100),
        link = new URL(row.trackViewUrl);
      if (
        image.protocol === "https:" &&
        image.hostname.endsWith(".mzstatic.com") &&
        link.protocol === "https:" &&
        ["music.apple.com", "itunes.apple.com"].includes(link.hostname)
      )
        return { image: image.href, link: link.href };
    } catch {
      /* Ignore malformed metadata. */
    }
  }
  return null;
}

export function selectSecondaryArtwork(item: QueueItem, metadata: unknown): QueueArtwork | null {
  if (!metadata || typeof metadata !== "object" || !("data" in metadata) || !Array.isArray(metadata.data)) return null;
  const artist = normalize(item.artist.split(",")[0]);
  if (!artist) return null;
  for (const row of metadata.data.slice(0, 12)) {
    if (
      !row ||
      typeof row.title !== "string" ||
      typeof row.artist?.name !== "string" ||
      recordingTitle(row.title, item.artist) !== recordingTitle(item.title, item.artist) ||
      normalize(row.artist.name) !== artist ||
      isCompilation(row.album?.artist?.name, row.album?.isCompilation, row.album?.record_type) ||
      typeof row.album?.cover_medium !== "string"
    )
      continue;
    try {
      const image = new URL(row.album.cover_medium);
      if (image.protocol === "https:" && image.hostname.endsWith(".dzcdn.net")) return { image: image.href, link: "" };
    } catch {
      /* Ignore malformed metadata. */
    }
  }
  return null;
}

export function createArtworkLoader(fetcher: typeof fetch = fetch, secondary?: (item: QueueItem) => Promise<unknown>) {
  const cache = new Map<string, { value: QueueArtwork | null; expires: number }>();
  const pending = new Map<string, Promise<QueueArtwork | null>>();
  let requests: number[] = [];
  const json = async (url: string) => {
    const response = await fetcher(url, {
      signal: AbortSignal.timeout(6000),
      credentials: "omit",
      referrerPolicy: "no-referrer",
    });
    if (!response.ok) throw Error("Artwork unavailable");
    const raw = await response.text();
    if (raw.length > 512_000) throw Error("Artwork metadata too large");
    return JSON.parse(raw);
  };
  return (item: QueueItem): Promise<QueueArtwork | null> => {
    const key = artworkKey(item),
      now = Date.now();
    const cached = cache.get(key);
    if (cached && cached.expires > now) return Promise.resolve(cached.value);
    const existing = pending.get(key);
    if (existing) return existing;
    requests = requests.filter((time) => now - time < 60_000);
    if (requests.length >= 60) return Promise.resolve(null);
    requests.push(now);
    const request = (async () => {
      try {
        const term = `${recordingTitle(item.title, item.artist)} ${item.artist.split(",")[0]}`.slice(0, 500);
        let result: QueueArtwork | null = null;
        try {
          const data = await json(
            `https://itunes.apple.com/search?${new URLSearchParams({ term, entity: "song", limit: "12" })}`,
          );
          result = selectArtwork(item, data.results);
        } catch {
          /* Try the second catalog if the first is unavailable. */
        }
        if (!result && secondary) result = selectSecondaryArtwork(item, await secondary(item));
        cache.set(key, { value: result, expires: now + (result ? 24 * 60 * 60_000 : 5 * 60_000) });
        if (cache.size > 200) cache.delete(cache.keys().next().value!);
        return result;
      } catch {
        cache.set(key, { value: null, expires: Date.now() + 30_000 });
        return null;
      } finally {
        pending.delete(key);
      }
    })();
    pending.set(key, request);
    return request;
  };
}

export async function verifySecondaryRelease(
  item: QueueItem,
  metadata: unknown,
  album: (id: number) => Promise<unknown>,
): Promise<unknown> {
  if (!metadata || typeof metadata !== "object" || !("data" in metadata) || !Array.isArray(metadata.data))
    return { data: [] };
  for (const row of metadata.data.slice(0, 12)) {
    if (!selectSecondaryArtwork(item, { data: [row] })) continue;
    const id = row.album?.id;
    if (!Number.isSafeInteger(id) || id <= 0) continue;
    try {
      const raw = await album(id);
      if (!raw || typeof raw !== "object") continue;
      const release = raw as Record<string, unknown>;
      if (
        release.id !== id ||
        !["album", "single", "ep"].includes(String(release.record_type)) ||
        !release.artist ||
        typeof release.artist !== "object" ||
        !("name" in release.artist) ||
        typeof release.artist.name !== "string"
      )
        continue;
      const candidate = { ...row, album: { ...row.album, ...release } };
      if (selectSecondaryArtwork(item, { data: [candidate] })) return { data: [candidate] };
    } catch {
      /* Try another verified release; never guess after a failed lookup. */
    }
  }
  return { data: [] };
}

// Keep decoded images alive, including the five lookahead covers. Several songs
// on the same album share a single download and decode job.
const preparedImages = new Map<string, Promise<HTMLImageElement>>();
export function prepareArtwork(cover: QueueArtwork): Promise<unknown> {
  const existing = preparedImages.get(cover.image);
  if (existing) return existing;
  const image = new Image();
  image.referrerPolicy = "no-referrer";
  const ready = new Promise<HTMLImageElement>((resolve, reject) => {
    image.onload = () => resolve(image);
    image.onerror = () => reject(Error("Cover image unavailable"));
    image.src = cover.image;
  }).then(async () => {
    if (image.decode) await image.decode();
    return image;
  });
  preparedImages.set(cover.image, ready);
  if (preparedImages.size > 200) preparedImages.delete(preparedImages.keys().next().value!);
  void ready.catch(() => preparedImages.delete(cover.image));
  return ready;
}

export function createArtworkStore(
  load: (item: QueueItem) => Promise<QueueArtwork | null>,
  prepare: (cover: QueueArtwork) => Promise<unknown>,
) {
  return create<{
    covers: Record<string, QueueArtwork>;
    request(item: QueueItem): Promise<void>;
  }>((set, get) => ({
    covers: {},
    async request(item) {
      const key = artworkKey(item);
      if (get().covers[key]) return;
      try {
        const cover = await load(item);
        if (!cover) return;
        await prepare(cover);
        set((state) => ({ covers: Object.fromEntries([...Object.entries(state.covers), [key, cover]].slice(-200)) }));
      } catch {
        /* Keep a failed image out of the ready-to-display cache. */
      }
    },
  }));
}

const albums = new Map<number, Promise<unknown>>();
const load = createArtworkLoader(fetch, async (item) => {
  const { bridge } = await import("../shared/bridge");
  const metadata = await bridge.music.queueArtwork(item.title, item.artist.split(",")[0]);
  return verifySecondaryRelease(item, metadata, (id) => {
    let pending = albums.get(id);
    if (!pending) {
      pending = bridge.music.queueAlbum(id);
      albums.set(id, pending);
      if (albums.size > 200) albums.delete(albums.keys().next().value!);
      void pending.catch(() => albums.delete(id));
    }
    return pending;
  });
});
export const useQueueArtwork = createArtworkStore(load, prepareArtwork);
