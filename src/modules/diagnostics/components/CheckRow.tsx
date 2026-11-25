import { CaretRightIcon, CopyIcon, ShieldCheckIcon } from "@phosphor-icons/react";
import { useState } from "react";

import { Button, IconButton, Tooltip, useToast } from "@/components";
import { errorSummary, m } from "@/i18n";
import { api, type Check, isErr } from "@/lib/tauri";
import { twMerge } from "@/utils";

import { checkFix, checkLabel, checkState } from "../checkCopy";
import { SeverityBadge } from "./SeverityBadge";

const VERBATIM_PREFIX = /^(\\\\\?\\|\/\/\?\/)/;

function normalizePath(value: string): string {
  return value.replace(VERBATIM_PREFIX, "");
}

export function CheckRow({ check }: { check: Check }) {
  const [open, setOpen] = useState(false);
  const toast = useToast();
  const fix = checkFix(check);
  const hasDetails = check.details.length > 0 || !!check.fixCommand;

  function copyCommand() {
    if (!check.fixCommand) return;
    navigator.clipboard
      .writeText(check.fixCommand)
      .then(() =>
        toast.success(
          m.diagnostics_command_copied_title(),
          m.diagnostics_command_copied_description(),
        ),
      )
      .catch(() =>
        toast.error(m.diagnostics_copy_failed_title(), m.diagnostics_copy_failed_description()),
      );
  }

  async function runAsAdmin() {
    if (!check.fixCommand) return;
    let clipboardOk = true;
    try {
      await navigator.clipboard.writeText(check.fixCommand);
    } catch {
      clipboardOk = false;
    }
    const result = await api.diagnostics.openElevatedTerminal(clipboardOk);
    if (isErr(result)) {
      toast.error(m.diagnostics_terminal_failed_title(), errorSummary(result.error));
      return;
    }
    if (clipboardOk) {
      toast.success(
        m.diagnostics_terminal_opened_title(),
        m.diagnostics_terminal_opened_description(),
      );
    } else {
      toast.warning(
        m.diagnostics_terminal_no_copy_title(),
        m.diagnostics_terminal_no_copy_description(),
      );
    }
  }

  return (
    <div className="border-t border-surface-700/60 first:border-t-0">
      <button
        type="button"
        onClick={() => hasDetails && setOpen((v) => !v)}
        disabled={!hasDetails}
        aria-expanded={hasDetails ? open : undefined}
        className={twMerge(
          "flex w-full items-start gap-3 px-4 py-2.5 text-left transition-colors",
          hasDetails && "cursor-pointer hover:bg-surface-800/60",
          !hasDetails && "cursor-default",
        )}
      >
        <SeverityBadge severity={check.severity} />
        <span className="min-w-0 flex-1">
          <span className="block text-sm font-medium text-surface-200">{checkLabel(check)}</span>
          {fix && (
            <span className="mt-0.5 block text-xs leading-relaxed text-surface-400">
              <span className="text-surface-500">{m.diagnostics_how_to_fix_label()}: </span>
              {fix}
            </span>
          )}
        </span>
        <span className="mt-0.5 max-w-[45%] shrink-0 truncate text-right text-xs text-surface-400">
          {normalizePath(checkState(check))}
        </span>
        {hasDetails && (
          <CaretRightIcon
            className={twMerge(
              "mt-0.5 h-4 w-4 shrink-0 text-surface-500 transition-transform",
              open && "rotate-90",
            )}
            weight="bold"
          />
        )}
      </button>

      {open && hasDetails && (
        <div className="space-y-3 border-t border-surface-700/60 bg-surface-950/40 px-4 py-3">
          {check.details.length > 0 && (
            <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-xs">
              {check.details.map((d, i) => (
                <div key={`${d.key}-${i}`} className="contents">
                  <dt className="text-surface-500">{d.key}</dt>
                  <dd className="font-mono break-all text-surface-300">{normalizePath(d.value)}</dd>
                </div>
              ))}
            </dl>
          )}
          {check.fixCommand && (
            <div className="rounded-md border border-surface-700/60 bg-surface-900/80">
              <div className="flex items-center justify-between gap-2 border-b border-surface-700/60 px-3 py-1.5">
                <span className="text-[0.625rem] tracking-wider text-surface-500 uppercase">
                  {m.diagnostics_fix_command_label()}
                </span>
                <div className="flex items-center gap-1">
                  <Tooltip content={m.diagnostics_copy_command_action()}>
                    <IconButton
                      icon={<CopyIcon className="h-3.5 w-3.5" />}
                      variant="ghost"
                      size="sm"
                      onClick={copyCommand}
                      aria-label={m.diagnostics_copy_command_action()}
                    />
                  </Tooltip>
                  <Button
                    variant="light"
                    size="xs"
                    left={<ShieldCheckIcon className="h-3.5 w-3.5" />}
                    onClick={runAsAdmin}
                  >
                    {m.diagnostics_open_terminal_action()}
                  </Button>
                </div>
              </div>
              <pre className="overflow-x-auto px-3 py-2 font-mono text-xs whitespace-pre text-surface-200">
                {check.fixCommand}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
