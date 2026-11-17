// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "../shared/bridge";
import { loadPrefs, onPrefs } from "../shared/prefs";
import { applyLanguage, setRegionalSettings, type RegionalSettings } from "./index";
import "./localization.css";
let ready: Promise<void> | undefined;
/** Subscribe before reading so a preference changed during startup wins. */
export function initializeLocalization(): Promise<void> {
  return (ready ??= (async () => {
    let currentPreference: string | undefined;
    let pending = Promise.resolve();
    const applyPreference = (value: string) => {
      if (value !== currentPreference) {
        currentPreference = value;
        pending = applyLanguage(value).then(() => {});
      }
      return pending;
    };
    if (inTauri) {
      setRegionalSettings(await invoke<RegionalSettings>("locale_settings"));
      await listen<RegionalSettings>("locale://changed", async (event) => {
        setRegionalSettings(event.payload);
        await applyLanguage((await loadPrefs()).island.language);
      });
    }
    let events = 0;
    await onPrefs((prefs) => {
      events++;
      void applyPreference(prefs.island.language);
    });
    const before = events;
    const prefs = await loadPrefs();
    if (events === before) await applyPreference(prefs.island.language);
    else await pending;
  })());
}
