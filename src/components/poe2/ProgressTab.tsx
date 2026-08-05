import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, type ProgressSnapshot } from "../../bindings";
import { Button } from "../ui/Button";
import { useSettings } from "../../hooks/useSettings";

// Emitted by src-tauri/src/poe2/tracker.rs after a poll that added events.
const STATE_CHANGED_EVENT = "poe2://state-changed";

function formatDuration(seconds: number | null): string {
  // The backend reports a signed duration honestly (clock drift, a machine
  // waking from sleep, etc. can put `zone_since` slightly in the future).
  // A negative duration is not a duration, so it's treated the same as
  // unknown here rather than letting it flow into the floor/modulo math
  // below, which produces nonsense like "-1 min -5 s" for negative input.
  if (seconds === null || seconds < 0) return "—";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ${seconds % 60} s`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

export const ProgressTab: React.FC = () => {
  const { t } = useTranslation();
  const { updateSetting } = useSettings();
  const [snapshot, setSnapshot] = useState<ProgressSnapshot | null>(null);
  const [status, setStatus] = useState("");

  const load = useCallback(async () => {
    const result = await commands.poe2State();
    if (result.status === "ok") setSnapshot(result.data);
  }, []);

  useEffect(() => {
    void load();
    const unlisten = listen(STATE_CHANGED_EVENT, () => {
      void load();
    });
    return () => {
      // unlisten() resolves asynchronously, so an event landing between
      // unmount and that resolution can still trigger one more load() /
      // setSnapshot() call on an unmounted component. Known and benign
      // under React 18 (matches the same pattern in ItemsPage.tsx).
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  const chooseLogPath = useCallback(async () => {
    const picked = await open({ multiple: false, directory: false });
    if (typeof picked === "string") {
      await updateSetting("poe2_log_path", picked);
      await load();
    }
  }, [updateSetting, load]);

  const resetLogPath = useCallback(async () => {
    await updateSetting("poe2_log_path", null);
    await load();
  }, [updateSetting, load]);

  const rebuild = useCallback(async () => {
    const result = await commands.poe2RebuildDerived();
    setStatus(
      result.status === "ok"
        ? t("poe2.progress.rebuilt", { count: result.data })
        : t("poe2.progress.rebuildError"),
    );
    await load();
  }, [load, t]);

  if (!snapshot) return null;

  const s = snapshot;
  const gap = snapshot.level_gap;
  const gapNote =
    gap === null
      ? t("poe2.progress.gapUnknown")
      : gap < -2
        ? t("poe2.progress.gapBehind")
        : gap >= 0
          ? t("poe2.progress.gapFine")
          : t("poe2.progress.gapSlightly");

  // Already narrowed to the active character by the backend.
  const rewards = snapshot.rewards;

  return (
    <div className="p-4 space-y-3">
      {snapshot.importing && (
        <p className="text-sm opacity-70">{t("poe2.progress.importing")}</p>
      )}
      {!snapshot.log_present && (
        <p className="text-sm opacity-70">
          {t("poe2.progress.noLog", { path: snapshot.log_path })}
        </p>
      )}
      {snapshot.log_present && !snapshot.debug_lines && (
        <p className="text-sm opacity-70">{t("poe2.progress.noDebug")}</p>
      )}

      <div>
        <h2 className="text-lg font-semibold">
          {s.character ?? t("poe2.progress.noCharacter")}
        </h2>
        <p className="text-sm opacity-60">{s.ascendancy ?? ""}</p>
        <p className="text-sm opacity-60">
          {s.character_confirmed_ts
            ? t("poe2.progress.confirmed", { when: s.character_confirmed_ts })
            : t("poe2.progress.unconfirmed")}
        </p>
      </div>

      <div className="rounded-md border border-mid-gray/30 p-3">
        <p className="text-sm opacity-60">{t("poe2.progress.gap")}</p>
        <p className="text-2xl font-semibold">
          {gap === null ? "—" : gap > 0 ? `+${gap}` : gap}
        </p>
        <p className="text-sm opacity-60">{gapNote}</p>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.level")}</p>
          <p className="text-xl">{s.level ?? "—"}</p>
        </div>
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.act")}</p>
          <p className="text-xl">{snapshot.act ?? "—"}</p>
        </div>
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.zoneLevel")}</p>
          <p className="text-xl">{s.zone_level ?? "—"}</p>
        </div>
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.inZone")}</p>
          <p className="text-xl">{formatDuration(snapshot.seconds_in_zone)}</p>
        </div>
      </div>

      <div className="rounded-md border border-mid-gray/30 p-3">
        <p className="text-sm opacity-60">{t("poe2.progress.zone")}</p>
        <p className="text-xl">{snapshot.zone_name ?? s.zone_code ?? "—"}</p>
        {snapshot.zone_name && <p className="text-sm opacity-60">{s.zone_code}</p>}
      </div>

      <div className="rounded-md border border-mid-gray/30 p-3">
        <p className="text-sm opacity-60">{t("poe2.progress.rewards")}</p>
        {rewards.length === 0 ? (
          <p className="text-sm opacity-60">{t("poe2.progress.noRewards")}</p>
        ) : (
          <ul className="list-disc pl-5 text-sm">
            {rewards.map((r, i) => (
              <li key={`${r}-${i}`}>{r}</li>
            ))}
          </ul>
        )}
      </div>

      <div className="space-y-2">
        <p className="text-sm opacity-60">{t("poe2.progress.logPath")}</p>
        <p className="text-sm break-all opacity-80">{snapshot.log_path}</p>
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={chooseLogPath}>{t("poe2.progress.choose")}</Button>
          <Button onClick={resetLogPath}>{t("poe2.progress.reset")}</Button>
          <Button onClick={rebuild}>{t("poe2.progress.rebuild")}</Button>
        </div>
        <p className="text-sm opacity-60">
          {t("poe2.progress.events", { count: snapshot.event_count })} {status}
        </p>
      </div>
    </div>
  );
};
