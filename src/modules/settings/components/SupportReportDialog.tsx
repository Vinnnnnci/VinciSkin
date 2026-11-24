import { CopyIcon, DownloadSimpleIcon } from "@phosphor-icons/react";
import { save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import { Button, Dialog, RadioGroup, Spinner, Switch, TextareaField, useToast } from "@/components";
import { m } from "@/i18n";
import { errorSummary } from "@/i18n/errors";
import { formatBytes } from "@/utils";

import { useSaveSupportReport, useSupportReport } from "../api";

const PROBLEMS = [
  { value: "mod-not-showing", label: m.support_report_problem_mod_not_showing },
  { value: "game-crash", label: m.support_report_problem_game_crash },
  { value: "app-broken", label: m.support_report_problem_app_broken },
  { value: "other", label: m.support_report_problem_other },
] as const;

/** What one file of the zip is, for a reader who does not open JSON for a living. */
function fileRole(path: string): string {
  if (path === "report.md") return m.support_report_role_summary();
  if (path === "manifest.json") return m.support_report_role_manifest();
  if (path.startsWith("incidents/")) return m.support_report_role_incident();
  if (path.startsWith("game-log/")) return m.support_report_role_game_log();
  if (path.startsWith("log/")) return m.support_report_role_log();

  return m.support_report_role_machine();
}

interface SupportReportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * What a bug report sends, shown before it leaves the app.
 *
 * The report is built by the backend with no user input, so the preview is the only place a
 * value the redactor missed can be caught. The saved zip is built again from the same inputs,
 * which is why the problem and the notes travel with the save.
 */
export function SupportReportDialog({ open, onOpenChange }: SupportReportDialogProps) {
  return (
    <Dialog.Shell
      open={open}
      onClose={() => onOpenChange(false)}
      title={m.support_report_title()}
      description={m.support_report_description()}
      size="xl"
      className="max-w-4xl"
    >
      <SupportReportContent open={open} onClose={() => onOpenChange(false)} />
    </Dialog.Shell>
  );
}

function SupportReportContent({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [problem, setProblem] = useState<string>("other");
  const [notes, setNotes] = useState("");
  const [committedNotes, setCommittedNotes] = useState("");
  const [includeGameLog, setIncludeGameLog] = useState(false);
  const [selected, setSelected] = useState(0);

  const report = useSupportReport(problem, committedNotes, includeGameLog, open);
  const saveReport = useSaveSupportReport();
  const { toast } = useToast();

  const files = report.data?.files ?? [];
  const shown = files[selected];

  async function handleSave() {
    const path = await save({
      defaultPath: report.data?.suggestedName ?? "VinciSkin-report.zip",
      filters: [{ name: "Zip", extensions: ["zip"] }],
    });
    if (!path) return;

    try {
      const saved = await saveReport.mutateAsync({
        path,
        problem,
        notes: committedNotes,
        includeGameLog,
      });
      toast({ title: m.support_report_saved_title(), description: saved.path });
      onClose();
    } catch (error) {
      toast({ title: m.support_report_error_title(), description: String(error), type: "error" });
    }
  }

  async function handleCopy() {
    if (!report.data) return;

    await navigator.clipboard.writeText(report.data.summary);
    toast({ title: m.support_report_copied_title() });
  }

  return (
    <>
      <Dialog.Body className="flex h-[62vh] flex-col gap-4 overflow-hidden">
        <p className="text-sm text-surface-400">{m.support_report_intro()}</p>

        <div className="grid gap-4 sm:grid-cols-2">
          <RadioGroup.Root value={problem} onValueChange={setProblem}>
            <RadioGroup.Label>{m.support_report_problem_label()}</RadioGroup.Label>
            <RadioGroup.Options orientation="vertical">
              {PROBLEMS.map((option) => (
                <RadioGroup.Card key={option.value} value={option.value} title={option.label()} />
              ))}
            </RadioGroup.Options>
          </RadioGroup.Root>

          <TextareaField
            label={m.support_report_notes_label()}
            placeholder={m.support_report_notes_placeholder()}
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
            onBlur={() => setCommittedNotes(notes)}
            textareaClassName="h-[8.5rem] resize-none"
          />
        </div>

        <label className="flex items-start gap-2.5 rounded-lg border border-surface-700 px-3 py-2 select-none">
          <Switch checked={includeGameLog} onCheckedChange={setIncludeGameLog} />
          <span className="flex flex-col gap-0.5">
            <span className="text-sm text-surface-200">{m.support_report_game_log_label()}</span>
            <span className="text-xs text-surface-500">{m.support_report_game_log_hint()}</span>
          </span>
        </label>

        {report.isLoading && (
          <div className="flex flex-1 items-center justify-center">
            <Spinner size="lg" />
          </div>
        )}

        {report.isError && (
          <p className="text-sm text-danger">
            {m.support_report_error_title()} — {errorSummary(report.error)}
          </p>
        )}

        {report.data && (
          <div className="flex min-h-0 flex-1 gap-3">
            <ul
              className="w-64 shrink-0 overflow-y-auto rounded-lg border border-surface-700"
              data-ui="SupportReportDialog:files"
            >
              {files.map((file, index) => (
                <li key={file.path}>
                  <button
                    type="button"
                    onClick={() => setSelected(index)}
                    className={`flex w-full items-baseline justify-between gap-2 px-3 py-1.5 text-left text-sm select-none hover:bg-surface-700/60 ${
                      index === selected ? "bg-surface-700/80 text-surface-100" : "text-surface-400"
                    }`}
                  >
                    <span className="truncate">{file.path}</span>
                    <span className="shrink-0 text-xs text-surface-500">
                      {formatBytes(file.bytes)}
                    </span>
                  </button>
                </li>
              ))}
            </ul>

            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <p className="text-xs text-surface-500 select-none">{fileRole(shown?.path ?? "")}</p>
              <pre className="min-h-0 flex-1 overflow-auto rounded-lg border border-surface-700 bg-surface-900 p-3 text-xs whitespace-pre-wrap select-text">
                {shown?.text ?? ""}
              </pre>
              {shown?.truncated && (
                <p className="text-xs text-surface-500">{m.support_report_preview_truncated()}</p>
              )}
            </div>
          </div>
        )}
      </Dialog.Body>

      <Dialog.Footer>
        <span className="mr-auto text-xs text-surface-500">
          {m.support_report_total({ size: formatBytes(report.data?.totalBytes ?? 0) })}
        </span>
        <Button variant="ghost" size="sm" onClick={handleCopy} disabled={!report.data}>
          <CopyIcon className="h-4 w-4" weight="bold" />
          {m.support_report_copy_action()}
        </Button>
        <Button size="sm" onClick={handleSave} disabled={!report.data || saveReport.isPending}>
          <DownloadSimpleIcon className="h-4 w-4" weight="bold" />
          {m.support_report_save_action()}
        </Button>
      </Dialog.Footer>
    </>
  );
}
