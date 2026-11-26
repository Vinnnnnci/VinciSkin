import { m } from "@/i18n";

/**
 * The backend's patcher messages, as the sentence a reader gets.
 *
 * These arrive as plain text rather than an `AppError` code, because the loader's failures are
 * its own sentences. The few that a user can actually reach are named here; anything else is
 * shown as it came, which is better than a generic sentence that hides what happened.
 */
export function patcherErrorText(detail: string): string {
  const text = detail.toLowerCase();

  if (text.includes("the game is already running")) return m.cn_error_game_already_running();
  if (text.includes("the game is still running")) return m.cn_error_game_still_running();
  if (text.includes("the administrator prompt was cancelled")) return m.cn_error_uac_cancelled();
  if (text.includes("is missing beside the loader")) return m.cn_error_loader_dll_missing();
  if (text.includes("the patcher is not running")) return m.cn_error_loader_not_running();
  if (text.includes("already running")) return m.cn_error_loader_running();

  /* What the host reports for itself, before the DLL is in the game. */
  if (text.includes("no game window answered")) return m.cn_error_game_window_missing();
  if (text.includes("timeout out while waiting for init")) return m.cn_error_hook_timeout();
  if (text.includes("failed to create hook")) return m.cn_error_hook_refused();
  if (text.includes("cannot load") || text.includes("does not export")) {
    return m.cn_error_injector_load();
  }

  return detail;
}
