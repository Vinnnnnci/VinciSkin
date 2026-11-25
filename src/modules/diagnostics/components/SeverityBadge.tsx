import { CircleAlert, CircleCheck, CircleX, Info } from "lucide-react";
import { match } from "ts-pattern";

import { m } from "@/i18n";
import type { Severity } from "@/lib/tauri";
import { twMerge } from "@/utils";

const styles: Record<Severity, { bg: string; text: string }> = {
  ok: { bg: "border-success/40 bg-success/10", text: "text-success-text" },
  info: { bg: "border-info/40 bg-info/10", text: "text-info-text" },
  warn: { bg: "border-warning/40 bg-warning/10", text: "text-warning-text" },
  bad: { bg: "border-danger/40 bg-danger/10", text: "text-danger-text" },
};

function labelOf(severity: Severity): string {
  switch (severity) {
    case "ok":
      return m.diagnostics_severity_ok();
    case "info":
      return m.diagnostics_severity_info();
    case "warn":
      return m.diagnostics_severity_warn();
    case "bad":
      return m.diagnostics_severity_bad();
  }
}

export function SeverityBadge({ severity }: { severity: Severity }) {
  const s = styles[severity];
  return (
    <span
      className={twMerge(
        "inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-semibold tracking-wider whitespace-nowrap",
        s.bg,
        s.text,
      )}
    >
      <SeverityIcon severity={severity} className="h-3 w-3" />
      {labelOf(severity)}
    </span>
  );
}

export function SeverityIcon({ severity, className }: { severity: Severity; className?: string }) {
  return match(severity)
    .with("ok", () => <CircleCheck className={className} />)
    .with("info", () => <Info className={className} />)
    .with("warn", () => <CircleAlert className={className} />)
    .with("bad", () => <CircleX className={className} />)
    .exhaustive();
}
