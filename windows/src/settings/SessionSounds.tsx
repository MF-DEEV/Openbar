// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { Play } from "lucide-react";
import { t, tf, useLocale, type StringKey } from "../i18n";
import { bridge, type SoundChoice } from "../shared/bridge";
import type { SessionPrefs } from "../shared/prefs";
import { SOUND_EVENTS, SOUND_KEYS, soundsMuted } from "../sessions/soundPreferences";
import { Select } from "./Select";
import { Slider, SubRow } from "./ui";

export const soundText = (key: string) => t(`windows.settings.sessions.sounds.${key}` as StringKey);

/** Preview bypasses temporary notification mute; it never updates session state. */
export async function previewSound(choice: string, volume: number, play = bridge.sound.play): Promise<boolean> {
  if (choice === "none" || volume <= 0) return true;
  return (await play(choice, volume)) !== false;
}

/** One expiration timer while the control is on screen. Sleep/clock changes are
 * reconciled on focus; playback always compares the actual wall clock. */
export function useSoundMute(until: number) {
  const [, refresh] = useState(0);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const update = () => {
      clearTimeout(timer);
      refresh((value) => value + 1);
      const delay = until * 1000 - Date.now();
      if (delay > 0) timer = setTimeout(update, Math.min(delay + 10, 2147483647));
    };
    update();
    window.addEventListener("focus", update);
    document.addEventListener("visibilitychange", update);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("focus", update);
      document.removeEventListener("visibilitychange", update);
    };
  }, [until]);
  return until > Date.now() / 1000;
}

export function SessionSounds({
  prefs,
  set,
}: {
  prefs: SessionPrefs;
  set: <K extends keyof SessionPrefs>(key: K, value: SessionPrefs[K]) => void;
}) {
  useLocale();
  const [sounds, setSounds] = useState<SoundChoice[]>([]);
  const [failure, setFailure] = useState(false);
  const muted = useSoundMute(prefs.soundMutedUntil);
  useEffect(() => {
    let active = true;
    void bridge.sound
      .list()
      .then((list) => {
        if (active) setSounds(list ?? []);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);
  const options = [
    { value: "none", label: soundText("none") },
    { value: "", label: soundText("chime") },
    { value: "openbar:soft", label: soundText("soft") },
    { value: "openbar:alert", label: soundText("alert") },
    ...sounds.map((sound) => ({ value: sound.id, label: sound.name })),
  ];
  return (
    <section className="st-session-sounds" aria-label={soundText("title")}>
      <h3 className="st-group-title">{soundText("title")}</h3>
      <p className="st-card-caption">{soundText("caption")}</p>
      {SOUND_EVENTS.map((kind) => {
        const key = SOUND_KEYS[kind];
        const choice = prefs[key];
        const available = options.some((option) => option.value === choice);
        const choices = available
          ? options
          : [...options, { value: choice, label: `${choice} — ${soundText("unavailable")}` }];
        return (
          <SubRow key={kind} title={soundText(kind)}>
            <Select label={soundText(kind)} value={choice} options={choices} onChange={(value) => set(key, value)} />
            <button
              type="button"
              className="st-button st-sound-preview"
              aria-label={tf("windows.settings.sessions.sounds.preview" as StringKey, soundText(kind))}
              title={tf("windows.settings.sessions.sounds.preview" as StringKey, soundText(kind))}
              disabled={choice === "none" || prefs.soundVolume === 0}
              onClick={() => {
                setFailure(false);
                void previewSound(choice, prefs.soundVolume)
                  .then((played) => setFailure(!played))
                  .catch(() => setFailure(true));
              }}
            >
              <Play size={14} aria-hidden />
            </button>
          </SubRow>
        );
      })}
      <SubRow title={soundText("volume")}>
        <Slider
          label={soundText("volume")}
          value={prefs.soundVolume}
          min={0}
          max={100}
          step={1}
          format={(value) => `${Math.round(value)}%`}
          onChange={(value) => {
            set("soundVolume", value);
            if (value === 0) void bridge.sound.stopSession();
          }}
        />
      </SubRow>
      {muted && soundsMuted(prefs) ? (
        <SubRow title={soundText("muted")} caption={new Date(prefs.soundMutedUntil * 1000).toLocaleString()}>
          <button className="st-button" onClick={() => set("soundMutedUntil", 0)}>
            {soundText("unmute")}
          </button>
        </SubRow>
      ) : null}
      {failure ? (
        <p role="status" className="st-card-caption">
          {soundText("failed")}
        </p>
      ) : null}
    </section>
  );
}
