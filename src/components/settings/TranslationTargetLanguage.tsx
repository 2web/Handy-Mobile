import React, { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "@/components/ui";
import { useSettings } from "../../hooks/useSettings";
import {
  MODEL_CAPABILITY_LANGUAGES,
  getLanguageLabel,
} from "../../lib/constants/languages";

interface TranslationTargetLanguageProps {
  grouped?: boolean;
}

export const TranslationTargetLanguage: React.FC<TranslationTargetLanguageProps> =
  React.memo(({ grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const value = getSetting("translation_target_language") || "en";

    const options = useMemo(
      () =>
        // Translation output needs an explicit script, so `zh` is offered as its
        // Simplified / Traditional variants (the tray shortlist uses `zh-Hans`).
        MODEL_CAPABILITY_LANGUAGES.flatMap((l) =>
          l.value === "zh"
            ? [
                { value: "zh-Hans", label: "Chinese (Simplified)" },
                { value: "zh-Hant", label: "Chinese (Traditional)" },
              ]
            : [{ value: l.value, label: l.label }],
        ),
      [],
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
          placeholder={getLanguageLabel(value) || "English"}
          className="min-w-[200px]"
        />
      </SettingContainer>
    );
  });

TranslationTargetLanguage.displayName = "TranslationTargetLanguage";
