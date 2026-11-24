import { useMutation, useQuery } from "@tanstack/react-query";

import { api } from "@/lib/tauri";
import { mutationFn } from "@/utils/query";

import { settingsQueries } from "./queries";

export type { SavedSupportReport, SupportReportFile, SupportReportPreview } from "@/lib/tauri";

/** The report as it stands now, built only once the dialog that shows it opens. */
export function useSupportReport(
  problem: string,
  notes: string,
  includeGameLog: boolean,
  enabled: boolean,
) {
  return useQuery({
    ...settingsQueries.supportReport(problem, notes, includeGameLog, enabled),
    enabled,
  });
}

/** Writes the report the preview showed, with whatever the user typed in the meantime. */
export function useSaveSupportReport() {
  return useMutation({
    mutationFn: mutationFn(({ path, problem, notes, includeGameLog }: SaveReportArgs) =>
      api.saveSupportReport(path, problem, notes, includeGameLog),
    ),
  });
}

export interface SaveReportArgs {
  path: string;
  problem: string;
  notes: string;
  includeGameLog: boolean;
}
