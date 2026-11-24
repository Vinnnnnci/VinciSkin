import { PackageIcon } from "@phosphor-icons/react";
import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import { AlertBox, Button, FormField, useConfirm, useToast } from "@/components";
import { errorSummary, m } from "@/i18n";
import { errorActions } from "@/lib/errorActions";
import type { CnPackProgress, PackSwap } from "@/lib/tauri";

import {
  useCnPackProgress,
  useCnSkinPacks,
  useDownloadCnSkinPack,
  useForgetCnSkinPack,
  useImportCnSkinPack,
  useRestoreCnSkinPack,
} from "../api";

/**
 * Third-party skin packs, written into the game's own champion archives.
 *
 * A pack is what an overlay would have mounted: whole champion archives. This panel writes them
 * to disk instead, and every archive it covers is backed up beside the manager until it is put
 * back, so the page is where a reader sees what is on the game and undoes it.
 */
export function CnSkinPacks() {
  const packs = useCnSkinPacks();
  const progress = useCnPackProgress();
  const importPack = useImportCnSkinPack();
  const downloadPack = useDownloadCnSkinPack();
  const restorePack = useRestoreCnSkinPack();
  const forgetPack = useForgetCnSkinPack();
  const toast = useToast();
  const confirm = useConfirm();

  const [url, setUrl] = useState("");

  const pending =
    importPack.isPending || downloadPack.isPending || restorePack.isPending || forgetPack.isPending;

  async function pickFile() {
    if (pending) return;

    const chosen = await open({
      multiple: false,
      filters: [{ name: m.cn_pack_file_filter(), extensions: ["fantome", "wad.client"] }],
    });
    if (typeof chosen !== "string") return;

    /* Third-party archives go over the game's own, so every import is worth a confirmation
       rather than only the first. */
    const accepted = await confirm({
      title: m.cn_pack_import_confirm_title(),
      description: m.cn_pack_import_confirm_description(),
      confirmLabel: m.cn_pack_import_action(),
      tone: "warning",
    });
    if (!accepted) return;

    importPack.mutate(chosen, {
      onSuccess: (written) =>
        toast.success(
          m.cn_pack_applied_title(),
          m.cn_pack_applied_description({ count: written.length }),
        ),
      onError: (error) =>
        toast.error(m.cn_pack_import_failed_title(), errorSummary(error), {
          actions: errorActions(error),
        }),
    });
  }

  async function download() {
    const address = url.trim();
    if (pending || address === "") return;

    const accepted = await confirm({
      title: m.cn_pack_import_confirm_title(),
      description: m.cn_pack_import_confirm_description(),
      confirmLabel: m.cn_pack_download_action(),
      tone: "warning",
    });
    if (!accepted) return;

    downloadPack.mutate(address, {
      onSuccess: (written) => {
        setUrl("");
        toast.success(
          m.cn_pack_applied_title(),
          m.cn_pack_applied_description({ count: written.length }),
        );
      },
      onError: (error) =>
        toast.error(m.cn_pack_download_failed_title(), errorSummary(error), {
          actions: errorActions(error),
        }),
    });
  }

  async function restore(target: PackSwap) {
    if (pending) return;

    restorePack.mutate(
      { archive: target.archive },
      {
        onSuccess: () => toast.success(m.cn_pack_restored_title()),
        onError: (error) =>
          toast.error(m.cn_pack_restore_failed_title(), errorSummary(error), {
            actions: errorActions(error),
          }),
      },
    );
  }

  async function forget(target: PackSwap) {
    if (pending) return;

    const accepted = await confirm({
      title: m.cn_pack_forget_confirm_title(),
      description: m.cn_pack_forget_confirm_description(),
      confirmLabel: m.cn_pack_forget_action(),
      tone: "warning",
    });
    if (!accepted) return;

    forgetPack.mutate(
      { archive: target.archive },
      {
        onSuccess: () => toast.success(m.cn_pack_forgotten()),
        onError: (error) =>
          toast.error(m.cn_pack_forget_failed_title(), errorSummary(error), {
            actions: errorActions(error),
          }),
      },
    );
  }

  const rows = packs.data ?? [];
  const blocked = rows.some((row) => row.state === "drifted" || row.state === "backupMissing");

  return (
    <section
      className="flex shrink-0 flex-col gap-3 rounded-lg border border-surface-700/40 px-4 py-3"
      data-ui="CnSkinPacks"
    >
      <div className="flex items-center gap-2">
        <PackageIcon className="h-4 w-4 shrink-0 text-surface-400" />
        <h2 className="text-row font-medium text-surface-200">{m.cn_packs_title()}</h2>
        <Button
          size="sm"
          className="ml-auto shrink-0"
          disabled={pending}
          loading={importPack.isPending}
          onClick={pickFile}
        >
          {m.cn_pack_import_action()}
        </Button>
      </div>

      <p className="text-fine text-surface-500">{m.cn_packs_description()}</p>

      {/* A pack that is not on this machine yet is fetched from the host it is published on,
          which the backend bounds to a short list before anything is requested. */}
      <div className="flex items-center gap-2">
        <FormField
          className="flex-1"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          placeholder={m.cn_pack_url_placeholder()}
          aria-label={m.cn_pack_url_placeholder()}
        />
        <Button
          size="sm"
          className="shrink-0"
          disabled={pending || url.trim() === ""}
          loading={downloadPack.isPending}
          onClick={download}
        >
          {m.cn_pack_download_action()}
        </Button>
      </div>

      {/* What the write is doing right now, which is worth saying for a pack that is a hundred
          megabytes over a slow link or a whole archive of chunks. */}
      {progress !== null && (
        <p className="text-fine text-surface-400" data-ui="cn-pack-progress">
          {stageLabel(progress)}
        </p>
      )}

      {rows.length === 0 ? (
        <p className="text-meta text-surface-500" data-ui="cn-packs-empty">
          {m.cn_packs_empty()}
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {rows.map((row) => {
            const stuck = row.state === "drifted" || row.state === "backupMissing";

            return (
              <li
                key={row.archive}
                data-ui="cn-pack-row"
                className="flex flex-wrap items-center gap-x-2 gap-y-1 text-meta"
              >
                <span className="font-mono text-code break-all text-surface-300 select-text">
                  {row.archive}
                </span>
                <span className="text-surface-500">
                  {m.cn_pack_source_label({ name: row.pack })}
                </span>
                <span className="ml-auto flex shrink-0 items-center gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={pending || stuck}
                    onClick={() => restore(row)}
                  >
                    {m.cn_pack_restore_action()}
                  </Button>
                  <Button variant="ghost" size="sm" disabled={pending} onClick={() => forget(row)}>
                    {m.cn_pack_forget_action()}
                  </Button>
                </span>
              </li>
            );
          })}
        </ul>
      )}

      {blocked && (
        <AlertBox variant="warning" className="mt-1">
          {m.cn_pack_drifted_notice()}
        </AlertBox>
      )}
    </section>
  );
}

/** What the write is doing, as a page reads it. */
function stageLabel(progress: CnPackProgress): string {
  if (progress.stage === "fetching") return m.cn_pack_stage_fetching();
  if (progress.stage === "reading") return m.cn_pack_stage_reading();
  if (progress.stage === "writing") {
    return m.cn_pack_stage_writing({ archive: progress.currentArchive ?? "" });
  }
  return m.cn_pack_stage_complete();
}
