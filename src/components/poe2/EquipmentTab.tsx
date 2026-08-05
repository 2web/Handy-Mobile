import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type EquipmentView } from "../../bindings";
import { Button } from "../ui/Button";

export const EquipmentTab: React.FC = () => {
  const { t } = useTranslation();
  const [view, setView] = useState<EquipmentView | null>(null);
  const [penaltyDraft, setPenaltyDraft] = useState("");
  const [penaltyError, setPenaltyError] = useState(false);

  const load = useCallback(async () => {
    const result = await commands.poe2Equipment();
    if (result.status === "ok") {
      setView(result.data);
      setPenaltyDraft(
        result.data.summary.penalty === null
          ? ""
          : String(result.data.summary.penalty),
      );
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const savePenalty = useCallback(async () => {
    const trimmed = penaltyDraft.trim();
    if (trimmed === "") return;
    const parsed = Number(trimmed.replace(",", "."));
    if (!Number.isFinite(parsed) || parsed < 0 || parsed > 100) {
      setPenaltyError(true);
      return;
    }
    setPenaltyError(false);
    await commands.changePoe2ResistancePenaltySetting(parsed);
    await load();
  }, [penaltyDraft, load]);

  const clearPenalty = useCallback(async () => {
    setPenaltyError(false);
    await commands.changePoe2ResistancePenaltySetting(null);
    await load();
  }, [load]);

  const toggleExcluded = useCallback(
    async (id: number, excluded: boolean) => {
      await commands.poe2SetItemExcluded(id, excluded);
      await load();
    },
    [load],
  );

  if (!view) return null;

  if (view.items.length === 0) {
    return <p className="p-4 text-sm opacity-70">{t("poe2.equipment.noItems")}</p>;
  }

  const slotName = (slot: string) => t(`poe2.equipment.slot.${slot}`);

  const statusLabels: Record<string, string> = {
    worn: t("poe2.equipment.statusWorn"),
    superseded: t("poe2.equipment.statusSuperseded"),
    unrecognised: t("poe2.equipment.statusUnrecognised"),
    excluded: t("poe2.equipment.statusExcluded"),
  };
  const statusLabel = (status: string) => statusLabels[status] ?? status;

  return (
    <div className="p-4 space-y-4">
      <section>
        <h2 className="text-lg font-semibold">{t("poe2.equipment.title")}</h2>
        {view.summary.penalty === null && (
          <p className="mt-1 text-sm opacity-70">{t("poe2.equipment.penaltyUnset")}</p>
        )}
        <ul className="mt-2 list-none p-0">
          {view.summary.lines.map((l) => (
            <li key={l.element} className="border-t border-mid-gray/30 py-2">
              <div className="flex flex-wrap items-baseline gap-x-3">
                <span className="w-24 font-medium">
                  {t(`poe2.equipment.element.${l.element}`)}
                </span>
                <span className="opacity-70">
                  {t("poe2.equipment.fromGear")}: {l.from_gear}%
                </span>
                {l.total !== null && (
                  <span className="font-semibold">
                    {t("poe2.equipment.total")}: {l.total}%
                  </span>
                )}
                <span className="opacity-60">
                  {t("poe2.equipment.cap")} {l.cap}%
                </span>
                {l.total !== null && (
                  <span className={l.short_by === null ? "opacity-70" : "font-semibold"}>
                    {l.short_by === null
                      ? t("poe2.equipment.atCap")
                      : t("poe2.equipment.short", { amount: l.short_by })}
                  </span>
                )}
              </div>
            </li>
          ))}
        </ul>
      </section>

      {view.summary.lines.some(
        (l) => l.missing_from.length > 0 || l.empty_slots.length > 0,
      ) && (
        <section>
          <p className="font-medium">{t("poe2.equipment.gapsTitle")}</p>
          <ul className="mt-2 list-none p-0">
            {view.summary.lines
              .filter((l) => l.missing_from.length > 0 || l.empty_slots.length > 0)
              .map((l) => (
                <li key={l.element} className="border-t border-mid-gray/30 py-2 text-sm opacity-70">
                  <span className="font-medium opacity-100">
                    {t(`poe2.equipment.element.${l.element}`)}
                  </span>
                  {l.empty_slots.length > 0 && (
                    <p>
                      {t("poe2.equipment.emptySlots", {
                        slots: l.empty_slots.map(slotName).join(", "),
                      })}
                    </p>
                  )}
                  {l.missing_from.length > 0 && (
                    <p>
                      {t("poe2.equipment.givesNothing", {
                        slots: l.missing_from.map(slotName).join(", "),
                      })}
                    </p>
                  )}
                </li>
              ))}
          </ul>
        </section>
      )}

      <section className="rounded-md border border-mid-gray/30 p-3">
        <p className="font-medium">{t("poe2.equipment.penaltyTitle")}</p>
        <p className="mt-1 text-sm opacity-70">{t("poe2.equipment.penaltyHow")}</p>
        <div className="mt-2 flex items-center gap-2">
          <label className="text-sm opacity-70" htmlFor="poe2-penalty">
            {t("poe2.equipment.penaltyLabel")}
          </label>
          <input
            id="poe2-penalty"
            value={penaltyDraft}
            onChange={(e) => setPenaltyDraft(e.target.value)}
            inputMode="numeric"
            className="w-20 rounded-md border border-mid-gray/40 bg-transparent px-2 py-1"
          />
          <Button onClick={savePenalty}>{t("poe2.equipment.penaltySave")}</Button>
          <Button onClick={clearPenalty}>{t("poe2.equipment.penaltyClear")}</Button>
        </div>
        {penaltyError && (
          <p className="mt-2 text-sm text-red-400">{t("poe2.equipment.penaltyInvalid")}</p>
        )}
      </section>

      <section>
        <p className="font-medium">{t("poe2.equipment.itemsTitle")}</p>
        <ul className="mt-2 list-none p-0">
          {view.items.map((item) => (
            <li
              key={item.id}
              className="flex flex-wrap items-baseline gap-x-3 border-t border-mid-gray/30 py-2"
            >
              <span className="font-medium">{item.name ?? item.base_type ?? "—"}</span>
              <span className="text-sm opacity-60">
                {item.slot ? slotName(item.slot) : (item.item_class ?? "")}
              </span>
              <span className="text-sm opacity-60">{statusLabel(item.status)}</span>
              <button
                type="button"
                className="text-sm underline opacity-70"
                onClick={() => toggleExcluded(item.id, !item.excluded)}
              >
                {item.excluded ? t("poe2.equipment.include") : t("poe2.equipment.exclude")}
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
};
