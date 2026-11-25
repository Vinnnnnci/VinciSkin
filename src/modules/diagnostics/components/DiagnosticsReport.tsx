import { useMemo } from "react";

import { m } from "@/i18n";
import type { Category, Check, DiagnosticReport, Severity } from "@/lib/tauri";

import { CheckRow } from "./CheckRow";

const CATEGORY_ORDER: Category[] = ["system", "manager", "league", "patcher", "storage", "library"];

/** What a section of the report is called, in the reader's language. */
function categoryCopy(category: Category): { title: string; description: string } {
  switch (category) {
    case "system":
      return {
        title: m.diagnostics_category_system_title(),
        description: m.diagnostics_category_system_description(),
      };
    case "manager":
      return {
        title: m.diagnostics_category_manager_title(),
        description: m.diagnostics_category_manager_description(),
      };
    case "league":
      return {
        title: m.diagnostics_category_league_title(),
        description: m.diagnostics_category_league_description(),
      };
    case "patcher":
      return {
        title: m.diagnostics_category_patcher_title(),
        description: m.diagnostics_category_patcher_description(),
      };
    case "storage":
      return {
        title: m.diagnostics_category_storage_title(),
        description: m.diagnostics_category_storage_description(),
      };
    case "library":
      return {
        title: m.diagnostics_category_library_title(),
        description: m.diagnostics_category_library_description(),
      };
  }
}

const SEVERITY_ORDER: Severity[] = ["bad", "warn", "info", "ok"];

interface DiagnosticsReportProps {
  report: DiagnosticReport;
}

export function DiagnosticsReportView({ report }: DiagnosticsReportProps) {
  const grouped = useMemo(() => groupChecks(report.checks), [report.checks]);
  return (
    <div className="space-y-6">
      <CountsHeader checks={report.checks} />
      {CATEGORY_ORDER.map((cat) => {
        const checks = grouped.get(cat);
        if (!checks || checks.length === 0) return null;
        const labels = categoryCopy(cat);
        return (
          <section key={cat} className="space-y-2">
            <header className="px-1">
              <h3 className="text-sm font-semibold text-surface-100">{labels.title}</h3>
              <p className="text-xs text-surface-500">{labels.description}</p>
            </header>
            <div className="overflow-hidden rounded-xl border border-surface-700/50 bg-surface-900/95">
              {checks.map((c) => (
                <CheckRow key={c.id} check={c} />
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}

function groupChecks(checks: Check[]): Map<Category, Check[]> {
  const map = new Map<Category, Check[]>();
  for (const c of checks) {
    if (!map.has(c.category)) map.set(c.category, []);
    map.get(c.category)!.push(c);
  }
  for (const arr of map.values()) {
    arr.sort((a, b) => SEVERITY_ORDER.indexOf(a.severity) - SEVERITY_ORDER.indexOf(b.severity));
  }
  return map;
}

function CountsHeader({ checks }: { checks: Check[] }) {
  const counts: Record<Severity, number> = { ok: 0, info: 0, warn: 0, bad: 0 };
  for (const c of checks) counts[c.severity]++;

  const items: { sev: Severity; label: string; cls: string }[] = [
    { sev: "bad", label: m.diagnostics_counts_bad(), cls: "text-danger-text" },
    { sev: "warn", label: m.diagnostics_counts_warn(), cls: "text-warning-text" },
    { sev: "ok", label: m.diagnostics_counts_ok(), cls: "text-success-text" },
    { sev: "info", label: m.diagnostics_counts_info(), cls: "text-info-text" },
  ];

  return (
    <div className="grid grid-cols-2 gap-2 rounded-xl border border-surface-700/50 bg-surface-900/95 p-3 sm:grid-cols-4">
      {items.map((it) => (
        <div
          key={it.sev}
          className="flex flex-col items-baseline gap-1 rounded-md border border-surface-700/40 bg-surface-950/40 px-3 py-2"
        >
          <span className="text-[0.625rem] font-medium tracking-wider text-surface-500 uppercase">
            {it.label}
          </span>
          <span className={`text-xl font-semibold tabular-nums ${it.cls}`}>{counts[it.sev]}</span>
        </div>
      ))}
    </div>
  );
}
