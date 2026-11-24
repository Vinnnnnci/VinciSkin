import { useEffect, useState } from "react";

import { Button, useToast } from "@/components";
import { errorMessage, m } from "@/i18n";
import { api, type CnAutoStatus, type CnPatcherStatus, isOk, unwrap } from "@/lib/tauri";

/** What the loader is doing, as the words a reader sees. */
function injectLabel(status: CnPatcherStatus | null): string {
  if (!status) return m.cn_inject_idle_label();
  if (status.error) return m.cn_inject_error_label({ detail: status.error });
  if (!status.running) return m.cn_inject_idle_label();
  if (status.state === "injected") return m.cn_inject_injected_label();
  if (status.state === "attached") return m.cn_inject_attached_label();
  return m.cn_inject_scanning_label();
}

/** What the champ-select watcher is doing. */
function followLabel(status: CnAutoStatus | null): string {
  if (!status?.running) return m.cn_auto_off_label();
  if (status.error) return m.cn_auto_error_label({ detail: status.error });
  if (status.needsClientRestart) return m.cn_auto_restart_label();
  if (!status.applied) return m.cn_auto_waiting_label();
  return m.cn_auto_applied_label({ name: status.applied.character });
}

/**
 * The skin session's two halves, one button each.
 *
 * The loader serves the overlay to the game, and the watcher keeps that overlay
 * on the champion the client picked. Nothing else ties the two together.
 */
export function CnSessionControl() {
  const toast = useToast();
  const [patcher, setPatcher] = useState<CnPatcherStatus | null>(null);
  const [auto, setAuto] = useState<CnAutoStatus | null>(null);
  const [injectBusy, setInjectBusy] = useState(false);
  const [skinBusy, setSkinBusy] = useState(false);
  const [restoreBusy, setRestoreBusy] = useState(false);

  useEffect(() => {
    let alive = true;

    async function tick() {
      const [load, follow] = await Promise.all([api.cn.patcherStatus(), api.cn.autoStatus()]);
      if (!alive) return;
      if (isOk(load)) setPatcher(load.value);
      if (isOk(follow)) setAuto(follow.value);
    }

    void tick();
    const id = setInterval(() => void tick(), 2000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const injectRunning = patcher?.running ?? false;
  const skinRunning = auto?.running ?? false;

  /* The poll verifies this within seconds, but without it a second press
     inside that window acts on the state from before the command. */
  async function toggleInject() {
    setInjectBusy(true);
    try {
      if (injectRunning) {
        unwrap(await api.cn.stopPatcher());
        setPatcher({ running: false, state: null, error: null, logPath: null, gameRunning: false });
      } else {
        unwrap(await api.cn.startPatcher());
        setPatcher({ running: true, state: null, error: null, logPath: null, gameRunning: false });
      }
    } catch (error: unknown) {
      const title = injectRunning ? m.cn_inject_stop_failed_title() : m.cn_inject_failed_title();
      toast.error(title, errorMessage(error));
    } finally {
      setInjectBusy(false);
    }
  }

  async function toggleSkin() {
    setSkinBusy(true);
    try {
      if (skinRunning) {
        unwrap(await api.cn.stopAutoSwap());
        setAuto({
          running: false,
          currentChampion: null,
          applied: null,
          error: null,
          needsClientRestart: false,
        });
      } else {
        /* The follow has nothing to show without the host, so turning it on
           starts the host first. A host already up answers with its own
           "already running" error, which is not a failure here. */
        if (!injectRunning) {
          try {
            unwrap(await api.cn.startPatcher());
            setPatcher({
              running: true,
              state: null,
              error: null,
              logPath: null,
              gameRunning: false,
            });
          } catch (error: unknown) {
            const detail = errorMessage(error);
            if (!detail.includes("the patcher is already running")) {
              toast.error(m.cn_inject_failed_title(), detail);
              return;
            }
          }
        }

        unwrap(await api.cn.startAutoSwap());
        setAuto({
          running: true,
          currentChampion: null,
          applied: null,
          error: null,
          needsClientRestart: false,
        });
      }
    } catch (error: unknown) {
      const title = skinRunning ? m.cn_auto_stop_failed_title() : m.cn_auto_failed_title();
      toast.error(title, errorMessage(error));
    } finally {
      setSkinBusy(false);
    }
  }

  async function restore() {
    setRestoreBusy(true);
    try {
      unwrap(await api.cn.restoreClient());
      toast.success(m.cn_auto_restore_done());
    } catch (error: unknown) {
      toast.error(m.cn_auto_restore_failed_title(), errorMessage(error));
    } finally {
      setRestoreBusy(false);
    }
  }

  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-2" data-ui="CnSessionControl">
      <span className="text-sm text-surface-400">{injectLabel(patcher)}</span>
      <span className="text-sm text-surface-400">{followLabel(auto)}</span>

      <div className="ml-auto flex items-center gap-2">
        <Button size="sm" variant="ghost" disabled={restoreBusy} onClick={restore}>
          {m.cn_auto_restore_action()}
        </Button>
        <Button
          size="sm"
          variant={injectRunning ? "outline" : "default"}
          disabled={injectBusy}
          loading={injectBusy}
          onClick={toggleInject}
        >
          {injectRunning ? m.cn_inject_stop_action() : m.cn_inject_start_action()}
        </Button>
        <Button
          size="sm"
          variant={skinRunning ? "outline" : "default"}
          disabled={skinBusy}
          loading={skinBusy}
          onClick={toggleSkin}
        >
          {skinRunning ? m.cn_auto_stop_action() : m.cn_auto_start_action()}
        </Button>
      </div>
    </div>
  );
}
