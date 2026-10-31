// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island on or off (the Mac's header switch) and starting with Windows.

import { useLocale, t, LANGUAGES, applyLanguage, type LanguagePreference } from "../i18n";

import { Power, RectangleHorizontal, Languages } from "lucide-react";
import { useEffect, useState } from "react";
import { launchAtLogin } from "../shared/prefs";
import { useSettings } from "./state";
import { T } from "./strings";
import { Group, Page, Row, Toggle, Select } from "./ui";

export function GeneralSettings() {
  useLocale();
  const enabled = useSettings((s) => s.prefs.island.enabled);
  const language = useSettings((s) => s.prefs.island.language);
  const loaded = useSettings((s) => s.loaded);
  const set = useSettings((s) => s.island);
  const [login, setLogin] = useState<boolean | null>(null);
  const [loginError, setLoginError] = useState<string | null>(null);
  useEffect(() => {
    if (loaded) void applyLanguage(language);
  }, [language, loaded]);

  useEffect(() => {
    void launchAtLogin.get().then(setLogin);
  }, []);

  return (
    <Page title={T.nav.general}>
      <Group>
        <Row icon={Languages} title={t("s.languageLabel")} caption={t("windows.settings.languageCaption")}>
          <Select<LanguagePreference>
            className="st-language"
            label={t("s.languageLabel")}
            value={language}
            options={[{ value: "auto", label: t("notch.automatic") }, ...LANGUAGES]}
            onChange={(value) => set("language", value)}
          />
        </Row>
        <Row icon={RectangleHorizontal} title={T.general.island} caption={T.general.islandCaption}>
          <Toggle label={T.general.island} checked={enabled} onChange={(on) => set("enabled", on)} />
        </Row>
        <Row
          icon={Power}
          title={T.general.login}
          caption={loginError ? <span className="st-error">{loginError}</span> : T.general.loginCaption}
        >
          <Toggle
            label={T.general.login}
            checked={login ?? false}
            disabled={login === null}
            onChange={(on) => {
              setLogin(on);
              setLoginError(null);
              launchAtLogin
                .set(on)
                .then(setLogin)
                .catch((e: unknown) => {
                  setLogin(!on);
                  setLoginError(String(e));
                });
            }}
          />
        </Row>
      </Group>
    </Page>
  );
}
