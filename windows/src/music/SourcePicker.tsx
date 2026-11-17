// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { tf, t, useLocale } from "../i18n";

import { Check, ChevronDown } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { usePageLayer } from "../island/pageLayer";
import { Switch } from "../shared/ui/controls";
import { bridge } from "../shared/bridge";
import { loadPrefs, onPrefs, setPrefs } from "../shared/prefs";
import { useMusic } from "./store";
import type { Source } from "./types";

export function playerChoices(players: Source[], available: Source[], saved: string): Source[] {
  const all = new Map([...players, ...available].map((player) => [player.id, player]));
  if (saved && !all.has(saved)) all.set(saved, { id: saved, name: saved, isPlaying: false });
  return [...all.values()].sort((a, b) => a.name.localeCompare(b.name));
}

/** Installed choices are scanned once on mount, with live sessions supplied by SMTC. */
export function usePlayerChoices(preferred = "") {
  const [players, setPlayers] = useState<Source[]>([]);
  const [unavailable, setUnavailable] = useState(false);
  useEffect(() => {
    let stopped = false;
    void bridge.music
      .players()
      .then((players) => {
        if (!stopped) setPlayers(players ?? []);
      })
      .catch(() => {
        if (!stopped) setUnavailable(true);
      });
    return () => {
      stopped = true;
    };
  }, [preferred]);
  return { players, unavailable };
}

export function SourcePicker() {
  useLocale();
  const { playback, sources, selectedSource, selecting, includeOtherPlayers, idlePlayer, sourceError } = useMusic();
  const [preferred, setPreferred] = useState("");
  const [error, setError] = useState<string | null>(null);
  const { players } = usePlayerChoices(preferred);
  useEffect(() => {
    let stopped = false;
    const apply = (p: Awaited<ReturnType<typeof loadPrefs>>) => {
      if (!stopped) setPreferred(p.island.preferredPlayer);
    };
    const off = onPrefs(apply);
    void loadPrefs().then(apply);
    return () => {
      stopped = true;
      void off.then((stop) => stop());
    };
  }, []);
  async function save(patch: { preferredPlayer?: string; includeOtherPlayers?: boolean }) {
    try {
      await setPrefs({ island: patch });
      setError(null);
    } catch {
      setError(t("windows.music.couldNotSaveMusicPreferencesTryAgain"));
    }
  }
  const missing = selectedSource && !sources.some((s) => s.id === selectedSource && s.hasTrack !== false);
  const selected = sources.find((s) => s.id === selectedSource);
  const label = missing
    ? tf("windows.music.valueUnavailableAutomaticFallback", selected?.name ?? selectedSource)
    : selectedSource
      ? (selected?.name ?? selectedSource)
      : `${t("notchMusicExtras.automaticSource")}${playback ? ` · ${playback.sourceName}` : ""}`;
  return (
    <div className="m-player-options">
      <div className="m-player-options-title">{t("windows.music.musicSource")}</div>
      <div
        className="m-source-list"
        role="radiogroup"
        aria-label={t("windows.music.musicSource")}
        onKeyDown={(event) => {
          if (selecting || !["ArrowDown", "ArrowUp"].includes(event.key)) return;
          event.preventDefault();
          event.stopPropagation();
          const buttons = [
            ...event.currentTarget.querySelectorAll<HTMLButtonElement>('button[role="radio"]:not(:disabled)'),
          ];
          const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
          const next = buttons[(index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length];
          next?.focus();
          next?.click();
        }}
      >
        <button
          type="button"
          role="radio"
          className="m-source-option"
          aria-checked={selectedSource === null}
          aria-disabled={selecting || undefined}
          onClick={() => void useMusic.getState().selectSource(null)}
        >
          <span className="m-source-check">{selectedSource === null ? <Check size={12} /> : null}</span>
          <span>{t("notchMusicExtras.automaticSource")}</span>
        </button>
        {sources.map((source) => (
          <button
            key={source.id}
            type="button"
            role="radio"
            className="m-source-option"
            aria-checked={selectedSource === source.id}
            aria-disabled={selecting || undefined}
            onClick={() => void useMusic.getState().selectSource(source.id)}
          >
            <span className="m-source-check">{selectedSource === source.id ? <Check size={12} /> : null}</span>
            <span className="m-source-name">{source.name}</span>
            {source.isPlaying ? <small>{t("windows.music.playing")}</small> : null}
          </button>
        ))}
      </div>
      {missing || sourceError || error ? (
        <div className="m-source-status" role={sourceError || error ? "alert" : undefined}>
          {sourceError ?? error ?? label}
        </div>
      ) : null}
      <div className="m-preferred-player">
        <span>{t("windows.music.preferredPlayer")}</span>
        <PreferredPlayerPicker
          value={preferred}
          onChange={(value) => void save({ preferredPlayer: value })}
          options={[
            { value: "", label: t("notchMusicExtras.automaticSource") },
            ...playerChoices(players, sources, preferred).map((player) => ({ value: player.id, label: player.name })),
          ]}
        />
      </div>
      <div className="m-other-players">
        <span>{t("windows.music.includeOtherPlayers")}</span>
        <Switch
          label={t("windows.music.includeOtherPlayers")}
          on={includeOtherPlayers}
          onChange={(on) => void save({ includeOtherPlayers: on })}
        />
      </div>
      {!playback && idlePlayer ? (
        <button
          type="button"
          className="n-pill-button"
          onClick={() => {
            void bridge.music
              .openPlayer(idlePlayer.id)
              .then((ok) => {
                if (ok === false) setError(t("windows.music.couldNotOpenThePlayer"));
              })
              .catch(() => setError(t("windows.music.couldNotOpenThePlayer")));
          }}
        >
          {tf("notchMusicExtras.openNamedPlayer", idlePlayer.name)}
        </button>
      ) : null}
    </div>
  );
}

/** Draw inside the options panel: native select menus cannot share its dark,
 * rounded surface and can escape the island's interactive bounds. */
function PreferredPlayerPicker({
  value,
  options,
  onChange,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange(value: string): void;
}) {
  useLocale();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [bounds, setBounds] = useState({ width: 180, maxHeight: 120 });
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const id = useId();
  const selected = Math.max(
    0,
    options.findIndex((option) => option.value === value),
  );
  useLayoutEffect(() => {
    if (!open) return;
    const menu = root.current?.querySelector<HTMLElement>('[role="listbox"]');
    const option = menu?.querySelectorAll<HTMLElement>('[role="option"]')[active];
    if (!menu || !option) return;
    if (option.offsetTop < menu.scrollTop) menu.scrollTop = option.offsetTop;
    else if (option.offsetTop + option.offsetHeight > menu.scrollTop + menu.clientHeight)
      menu.scrollTop = option.offsetTop + option.offsetHeight - menu.clientHeight;
  }, [active, open, bounds.maxHeight]);
  usePageLayer(open, () => setOpen(false));
  useLayoutEffect(() => {
    if (!open || !root.current || !trigger.current) return;
    const panel = root.current.closest(".m-volume-popover");
    if (!panel) return;
    const style = getComputedStyle(panel);
    setBounds({
      width: panel.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
      maxHeight: Math.max(
        32,
        Math.min(144, trigger.current.getBoundingClientRect().top - panel.getBoundingClientRect().top - 10),
      ),
    });
  }, [open, options.length]);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  const choose = (index: number) => {
    const option = options[index];
    if (!option) return;
    setOpen(false);
    onChange(option.value);
  };
  return (
    <div className="m-preferred-choice" ref={root}>
      <button
        type="button"
        ref={trigger}
        className="m-preferred-trigger"
        role="combobox"
        aria-label={t("windows.music.preferredPlayer")}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-activedescendant={open ? id + "-" + active : undefined}
        onClick={() => {
          setActive(selected);
          setOpen(!open);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape" && !open) return;
          if (["ArrowDown", "ArrowUp", "Enter", " ", "Escape"].includes(event.key)) {
            event.preventDefault();
            event.stopPropagation();
            if (event.key === "Escape") {
              setOpen(false);
              return;
            }
            if (event.key === "Enter" || event.key === " ") {
              if (open) choose(active);
              else {
                setActive(selected);
                setOpen(true);
              }
              return;
            }
            if (!open) {
              setActive(selected);
              setOpen(true);
            } else setActive((active + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length);
          } else if (event.key === "Tab") setOpen(false);
        }}
      >
        <span>{options[selected]?.label}</span>
        <ChevronDown size={11} />
      </button>
      {open ? (
        <div
          id={id}
          role="listbox"
          aria-label={t("windows.music.preferredPlayer")}
          className="m-preferred-menu"
          style={bounds}
        >
          {options.map((option, index) => (
            <button
              key={option.value}
              type="button"
              id={id + "-" + index}
              role="option"
              aria-selected={option.value === value}
              className={"m-preferred-option" + (index === active ? " is-active" : "")}
              onPointerMove={() => setActive(index)}
              onPointerDown={(event) => event.preventDefault()}
              onClick={() => choose(index)}
            >
              <span className="m-source-check">{option.value === value ? <Check size={12} /> : null}</span>
              <span>{option.label}</span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
