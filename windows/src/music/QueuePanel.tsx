// SPDX-License-Identifier: GPL-3.0-or-later
import { ListMusic, LoaderCircle, Music, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { t, useLocale } from "../i18n";
import { bridge } from "../shared/bridge";
import { useQueue, type QueueItem } from "./queueStore";
import { artworkKey, useQueueArtwork } from "./queueArtwork";
import type { Playback } from "./types";

export function QueuePanel({ playback }: { playback: Playback }) {
  useLocale();
  const snapshot = useQueue((s) => s.snapshot);
  const loading = useQueue((s) => s.loading);
  const [refreshing, setRefreshing] = useState(false);
  const { source, title } = playback;
  const opened = useRef<string | null>(null);
  const reduce = useReducedMotion();
  useEffect(() => () => useQueue.getState().cancel(), []);
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async (open: boolean) => {
      const ready = useQueue.getState().snapshot?.status === "ready";
      await useQueue.getState().refresh(source, title, open || !ready);
      if (!cancelled) timer = setTimeout(() => void poll(false), useQueue.getState().recovering ? 500 : 2500);
    };
    void poll(opened.current !== source);
    opened.current = source;
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [source, title]);
  const current = snapshot?.source === source ? snapshot : null;
  useEffect(() => {
    if (current?.status === "ready")
      for (const item of [...current.items, ...(current.prefetch ?? [])]) void useQueueArtwork.getState().request(item);
  }, [current?.items, current?.prefetch]);
  return (
    <section className="m-queue" id="music-queue" aria-label={t("notchMusicExtras.queue")} aria-busy={loading}>
      <header className="m-queue-heading">
        <ListMusic size={15} aria-hidden="true" />
        <strong>{t("notchMusicExtras.queue")}</strong>
        <button
          type="button"
          className="m-button is-small"
          aria-label={t("notchMusicExtras.refresh")}
          disabled={refreshing}
          aria-busy={refreshing}
          onClick={async () => {
            if (refreshing) return;
            setRefreshing(true);
            try {
              await useQueue.getState().refresh(source, title, true);
            } finally {
              setRefreshing(false);
            }
          }}
        >
          <RefreshCw size={14} className={refreshing ? "m-queue-spinner" : undefined} />
        </button>
      </header>
      {current?.status === "ready" && current.items.length > 0 ? (
        <ol className="m-queue-list">
          <AnimatePresence initial={false}>
            {current.items.map((item, index) => (
              <motion.li
                className="m-queue-row"
                key={item.id ?? `${item.title}:${item.artist}`}
                initial={{ opacity: 0, y: index * 48 + (reduce ? 0 : 12) }}
                animate={{ opacity: 1, y: index * 48 }}
                exit={{ opacity: 0 }}
                transition={{
                  opacity: { duration: reduce ? 0 : 0.2 },
                  y: { duration: reduce ? 0 : 0.28, ease: [0.22, 1, 0.36, 1] },
                }}
              >
                <span className="m-queue-number" aria-hidden="true">
                  {index + 1}
                </span>
                <QueueCover item={item} />
                <span className="m-queue-song">
                  <strong>{item.title}</strong>
                  <span>{item.artist}</span>
                </span>
              </motion.li>
            ))}
          </AnimatePresence>
        </ol>
      ) : (
        <div className="m-queue-empty" role="status">
          {loading && !current ? <LoaderCircle size={22} className="m-queue-spinner" /> : <ListMusic size={24} />}
          {loading && !current ? null : (
            <p>
              {t(current?.status === "ready" ? "notchMusicExtras.queueEmpty" : "notchMusicExtras.queueUnavailable")}
            </p>
          )}
          {!loading && current?.status !== "ready" ? (
            <button className="m-queue-open" type="button" onClick={() => void bridge.music.openPlayer(source)}>
              {t("notchMusicExtras.openPlayer")}
            </button>
          ) : null}
        </div>
      )}
    </section>
  );
}

function QueueCover({ item }: { item: QueueItem }) {
  const cover = useQueueArtwork((s) => s.covers[artworkKey(item)]);
  const [failedImage, setFailedImage] = useState<string | null>(null);
  if (!cover || failedImage === cover.image)
    return (
      <span className="m-queue-art" aria-hidden="true">
        <Music size={18} />
      </span>
    );
  return (
    <span className="m-queue-art" aria-hidden="true">
      <Music size={18} aria-hidden="true" />
      <img
        src={cover.image}
        alt=""
        draggable={false}
        decoding="async"
        referrerPolicy="no-referrer"
        className="is-loaded"
        onError={() => setFailedImage(cover.image)}
      />
    </span>
  );
}
