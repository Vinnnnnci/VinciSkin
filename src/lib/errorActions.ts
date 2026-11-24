import { m } from "@/i18n";
import { errorSummary } from "@/i18n/errors";
import type { AppError } from "@/lib/tauri";
import { supportReportDialog } from "@/stores/supportReportDialog";

/** What a toast offers beside a failure: its details, and a way to send them. */
export interface ErrorAction {
  label: string;
  onClick: () => void;
}

/**
 * The two actions every failure toast carries.
 *
 * The code is what a maintainer greps for and the summary is what the reader saw, so copying
 * both is what makes a report reproducible in a chat. The second opens the support report with
 * the failure still on screen.
 */
export function errorActions(error: AppError): ErrorAction[] {
  return [
    {
      label: m.support_report_copy_detail_action(),
      onClick: () => {
        void navigator.clipboard.writeText(errorDetail(error));
      },
    },
    {
      label: m.support_report_action(),
      onClick: () => supportReportDialog.getState().open(),
    },
  ];
}

/** An error as the text a clipboard carries: the code first, then what it meant. */
export function errorDetail(error: AppError): string {
  return `${error.code}\n${errorSummary(error)}`;
}
