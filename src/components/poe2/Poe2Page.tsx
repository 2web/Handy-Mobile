import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { EquipmentTab } from "./EquipmentTab";
import { ItemsPage } from "./ItemsPage";
import { ProgressTab } from "./ProgressTab";

type Tab = "progress" | "items" | "equipment";

export const Poe2Page: React.FC = () => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("progress");

  return (
    <div className="max-w-3xl w-full mx-auto">
      <div className="flex gap-2 border-b border-mid-gray/30 px-4 pt-3">
        {(["progress", "items", "equipment"] as Tab[]).map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={
              tab === id
                ? "border-b-2 border-current px-3 py-2 font-medium"
                : "px-3 py-2 opacity-60"
            }
          >
            {t(`poe2.tabs.${id}`)}
          </button>
        ))}
      </div>
      {tab === "progress" && <ProgressTab />}
      {tab === "items" && <ItemsPage />}
      {tab === "equipment" && <EquipmentTab />}
    </div>
  );
};
