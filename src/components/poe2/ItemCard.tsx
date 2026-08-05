import React from "react";
import { useTranslation } from "react-i18next";
import type { StoredItem } from "../../bindings";

interface ItemCardProps {
  item: StoredItem;
}

export const ItemCard: React.FC<ItemCardProps> = React.memo(({ item }) => {
  const { t } = useTranslation();

  const subtitle = [
    item.base_type,
    item.item_level ? t("poe2.items.itemLevel", { level: item.item_level }) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <li className="border-t border-mid-gray/30 py-3">
      <p className="font-semibold">{item.name || item.base_type || "—"}</p>
      <p className="text-sm opacity-60">{subtitle}</p>
      <ul className="mt-1 pl-4 text-sm list-disc">
        {item.mods.map((mod) => (
          <li key={`${mod.position}-${mod.effect_index}`}>
            {mod.text}
            {mod.tier ? ` (T${mod.tier})` : ""}
            {/* A rune can be pulled out and moved to another item; an affix
                cannot. The player must be able to tell them apart. */}
            {mod.kind === "rune" ? ` — ${t("poe2.items.rune")}` : ""}
          </li>
        ))}
      </ul>
      {!item.advanced && (
        <p className="mt-1 text-sm opacity-60">{t("poe2.items.simpleFormat")}</p>
      )}
    </li>
  );
});
