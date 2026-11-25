import {
  ArrowClockwiseIcon,
  ClipboardTextIcon,
  LifebuoyIcon,
  StethoscopeIcon,
} from "@phosphor-icons/react";
import { getRouteApi, useNavigate } from "@tanstack/react-router";

import { AlertBox, Button, Separator, Spinner, Tabs, useToast } from "@/components";
import { errorSummary, m } from "@/i18n";
import type { DiagnosticReport } from "@/lib/tauri";
import { DiagnosticsReportView, GamesTab, useDiagnostics } from "@/modules/diagnostics";
import { SuspectModAction } from "@/modules/library";
import { supportReportDialog } from "@/stores/supportReportDialog";

import type { DiagnosticsTab } from "../routes/diagnostics";

const routeApi = getRouteApi("/diagnostics");

function formatGeneratedAt(iso: string) {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

function reportToText(report: DiagnosticReport): string {
  const lines: string[] = [];
  lines.push(`# VinciSkin diagnostics`);
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`App version: ${report.appVersion}`);
  lines.push("");
  for (const c of report.checks) {
    lines.push(`[${c.severity.toUpperCase()}] ${c.label} — ${c.summary}`);
    for (const d of c.details) {
      lines.push(`    ${d.key}: ${d.value}`);
    }
    if (c.suggestion) {
      lines.push(`    note: ${c.suggestion}`);
    }
    if (c.fixCommand) {
      for (const cmdLine of c.fixCommand.split("\n")) {
        lines.push(`    > ${cmdLine}`);
      }
    }
    lines.push("");
  }
  return lines.join("\n");
}

export function Diagnostics() {
  const { tab } = routeApi.useSearch();
  const navigate = useNavigate({ from: "/diagnostics" });
  const value: DiagnosticsTab = tab ?? "games";

  function setTab(next: DiagnosticsTab) {
    navigate({ search: (prev) => ({ ...prev, tab: next }), replace: true });
  }

  return (
    <div data-ui="Diagnostics" className="flex h-full flex-col">
      <Tabs.Root
        value={value}
        onValueChange={(next) => setTab(next as DiagnosticsTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        {/* The tab strip is the page's only chrome, so the title shares its row
            rather than standing over it in a band of its own. */}
        <header className="flex shrink-0 items-center border-b border-surface-700/50 px-4 select-none">
          <h1 className="flex shrink-0 items-center gap-2 text-sm font-semibold text-surface-200">
            <StethoscopeIcon className="h-4 w-4 text-accent-400" />
            {m.shell_diagnostics_label()}
          </h1>
          <Separator orientation="vertical" className="mx-2 h-4" />
          <Tabs.List className="border-b-0">
            <Tabs.Tab value="games">{m.diagnostics_tab_games()}</Tabs.Tab>
            <Tabs.Tab value="system">{m.diagnostics_tab_system()}</Tabs.Tab>
          </Tabs.List>
        </header>

        <Tabs.Panel value="games" className="mt-0 flex min-h-0 flex-1 flex-col">
          <GamesTab modAction={(modId) => <SuspectModAction modId={modId} />} />
        </Tabs.Panel>
        <Tabs.Panel value="system" className="mt-0 min-h-0 flex-1 overflow-y-auto">
          <SystemTab />
        </Tabs.Panel>
      </Tabs.Root>
    </div>
  );
}

function SystemTab() {
  const diagnostics = useDiagnostics();
  const toast = useToast();
  const report = diagnostics.data;

  function copyReport() {
    if (!report) return;
    navigator.clipboard
      .writeText(reportToText(report))
      .then(() =>
        toast.success(
          m.diagnostics_copy_report_copied_title(),
          m.diagnostics_copy_report_copied_description(),
        ),
      )
      .catch(() =>
        toast.error(m.diagnostics_copy_failed_title(), m.diagnostics_copy_failed_description()),
      );
  }

  return (
    <div data-ui="SystemTab" className="mx-auto w-full max-w-5xl space-y-6 p-6">
      <header className="flex items-start justify-between gap-4 select-none">
        <div>
          <p className="text-sm text-surface-400">{m.diagnostics_system_intro()}</p>
          <p className="mt-1 text-sm text-surface-400">{m.support_report_diagnostics_hint()}</p>
          {report && (
            <p className="mt-1 text-xs text-surface-500">
              {m.diagnostics_last_run_label({
                time: formatGeneratedAt(report.generatedAt),
                version: report.appVersion,
              })}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => supportReportDialog.getState().open()}
            left={<LifebuoyIcon weight="bold" className="h-4 w-4" />}
          >
            {m.support_report_action()}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={copyReport}
            disabled={!report}
            left={<ClipboardTextIcon weight="bold" className="h-4 w-4" />}
          >
            {m.diagnostics_copy_report_action()}
          </Button>
          <Button
            variant="filled"
            size="sm"
            onClick={() => diagnostics.refetch()}
            loading={diagnostics.isFetching}
            left={<ArrowClockwiseIcon weight="bold" className="h-4 w-4" />}
          >
            {diagnostics.isFetching ? m.diagnostics_running_action() : m.diagnostics_rerun_action()}
          </Button>
        </div>
      </header>

      {diagnostics.isError && (
        <AlertBox variant="error" title={m.diagnostics_failed_title()}>
          {diagnostics.error ? errorSummary(diagnostics.error) : m.diagnostics_unknown_error()}
        </AlertBox>
      )}

      {!report && diagnostics.isFetching && (
        <div className="flex items-center justify-center rounded-xl border border-surface-700/50 bg-surface-900/50 py-16">
          <Spinner size="lg" />
        </div>
      )}

      {report && <DiagnosticsReportView report={report} />}
    </div>
  );
}
