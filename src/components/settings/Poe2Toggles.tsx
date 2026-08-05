import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface Poe2TogglesProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const Poe2Toggles: React.FC<Poe2TogglesProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("poe2_enabled") || false;

    return (
      <>
        <ToggleSwitch
          checked={enabled}
          onChange={(value) => updateSetting("poe2_enabled", value)}
          isUpdating={isUpdating("poe2_enabled")}
          label={t("settings.advanced.poe2.label")}
          description={t("settings.advanced.poe2.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {/* The clipboard switch only appears once the section itself is on:
            showing a game panel is harmless, reading the clipboard in the
            background is what the user must knowingly agree to. */}
        {enabled && (
          <ToggleSwitch
            checked={getSetting("poe2_clipboard_watch") || false}
            onChange={(value) => updateSetting("poe2_clipboard_watch", value)}
            isUpdating={isUpdating("poe2_clipboard_watch")}
            label={t("settings.advanced.poe2Clipboard.label")}
            description={t("settings.advanced.poe2Clipboard.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          />
        )}
      </>
    );
  },
);
