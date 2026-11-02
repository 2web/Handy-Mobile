import React, { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "@/components/ui";
import { useSettings } from "../../hooks/useSettings";
import { LANGUAGES, languageName } from "../../lib/constants/languages";

interface TranslationTargetLanguageProps {
  grouped?: boolean;
}

export const TranslationTargetLanguage: React.FC<TranslationTargetLanguageProps> =
  React.memo(({ grouped = false }) => {
    const { t, i18n } = useTranslation();
    const uiLanguage = i18n.language;
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const value = getSetting("translation_target_language") || "en";

    const options = useMemo(
      () =>
        // Translation output needs an explicit script, so `zh` is offered as its
        // Simplified / Traditional variants (the tray shortlist uses `zh-Hans`).
        LANGUAGES.flatMap((l) =>
          l.code === "zh" ? ["zh-Hans", "zh-Hant"] : [l.code],
        )
          .map((code) => ({
            value: code,
            label: languageName(code, uiLanguage),
          }))
          .sort((a, b) => a.label.localeCompare(b.label, uiLanguage)),
      [uiLanguage],
    );

    return (
      <SettingContainer
        title={t("settings.translation.targetLanguage.title")}
        description={t("settings.translation.targetLanguage.description")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={grouped}
      >
        <Dropdown
          selectedValue={value}
          options={options}
          onSelect={(v) => updateSetting("translation_target_language", v)}
          disabled={isUpdating("translation_target_language")}
          placeholder={languageName(value, uiLanguage)}
          className="min-w-[200px]"
        />
      </SettingContainer>
    );
  });

TranslationTargetLanguage.displayName = "TranslationTargetLanguage";
