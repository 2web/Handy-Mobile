import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type StoredItem } from "../../bindings";
import { Button } from "../ui/Button";
import { ItemCard } from "./ItemCard";

// Must match `NOT_AN_ITEM_ERROR` in src-tauri/src/poe2/commands.rs exactly:
// the backend returns this fixed string for a parse failure specifically so
// the frontend can tell it apart from a genuine storage failure without
// parsing arbitrary error prose.
const NOT_AN_ITEM_ERROR =
  "That does not look like an item. Hover it in the game and press Ctrl+C.";

// Emitted by the clipboard watcher (src-tauri/src/poe2/watcher.rs) after a
// background capture is successfully stored. No payload: we just refetch.
const ITEM_CAPTURED_EVENT = "poe2://item-captured";

export const ItemsPage: React.FC = () => {
  const { t } = useTranslation();
  const [items, setItems] = useState<StoredItem[]>([]);
  const [status, setStatus] = useState("");

  const loadItems = useCallback(async () => {
    const result = await commands.poe2ListItems();
    // Data is awaited before the list is replaced: clearing first makes the list
    // blink empty on every refresh.
    if (result.status === "ok") setItems(result.data);
  }, []);

  useEffect(() => {
    void loadItems();
  }, [loadItems]);

  // A background capture from the clipboard watcher has no other way to tell
  // this page a new item landed, so without this the list only picks it up
  // after navigating away and back.
  useEffect(() => {
    const unlisten = listen(ITEM_CAPTURED_EVENT, () => {
      void loadItems();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadItems]);

  const onPaste = useCallback(
    async (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const text = event.clipboardData.getData("text");
      event.preventDefault();
      if (!text.trim()) return;

      const result = await commands.poe2AddItem(text);
      if (result.status === "error") {
        setStatus(
          result.error === NOT_AN_ITEM_ERROR
            ? t("poe2.items.notAnItem")
            : t("poe2.items.failed"),
        );
        return;
      }
      setStatus(result.data.created ? t("poe2.items.saved") : t("poe2.items.duplicate"));
      await loadItems();
    },
    [loadItems, t],
  );

  const onRebuild = useCallback(async () => {
    const result = await commands.poe2RebuildItems();
    if (result.status === "ok") {
      const message = t("poe2.items.rebuilt", { count: result.data.reparsed });
      setStatus(
        result.data.failed > 0
          ? `${message} ${t("poe2.items.rebuildFailed", { count: result.data.failed })}`
          : message,
      );
      await loadItems();
    } else {
      setStatus(t("poe2.items.rebuildError"));
    }
  }, [loadItems, t]);

  return (
    <div className="p-4 space-y-3">
      <h2 className="text-lg font-semibold">{t("poe2.items.title")}</h2>

      <textarea
        rows={3}
        value=""
        onChange={() => {}}
        onPaste={onPaste}
        placeholder={t("poe2.items.placeholder")}
        className="w-full rounded-md border border-mid-gray/40 bg-transparent p-2 font-mono text-sm"
      />

      <div className="flex items-center gap-3">
        <Button onClick={onRebuild}>{t("poe2.items.rebuild")}</Button>
        <span className="text-sm opacity-60">{status}</span>
      </div>

      {items.length === 0 ? (
        <p className="text-sm opacity-60">{t("poe2.items.empty")}</p>
      ) : (
        <ul className="list-none p-0">
          {items.map((item) => (
            <ItemCard key={item.id} item={item} />
          ))}
        </ul>
      )}
    </div>
  );
};
