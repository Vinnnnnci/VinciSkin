import { m } from "@/i18n";
import type { Check, Severity } from "@/lib/tauri";

/**
 * What one check says in the reader's language, over the ids the backend reports.
 *
 * The backend writes its summary and suggestion in English, and its ids are the stable half of
 * a check, so the copy lives here keyed by id. A check this table does not know still draws,
 * with the backend's own words.
 */
interface Copy {
  label: () => string;
  /** The row's text for a severity, where the backend's summary is a sentence. */
  state?: Partial<Record<Severity, () => string>>;
  /** What to do about a row that is not ok. */
  fix?: () => string;
}

const COPY: Record<string, Copy> = {
  "windows.version": {
    label: m.diagnostics_check_version_label,
    state: { bad: m.diagnostics_check_version_bad },
    fix: m.diagnostics_check_version_fix,
  },
  "windows.long_paths": {
    label: m.diagnostics_check_long_paths_label,
    state: { warn: m.diagnostics_check_long_paths_warn, ok: m.diagnostics_check_long_paths_ok },
    fix: m.diagnostics_check_long_paths_fix,
  },
  "windows.uac": {
    label: m.diagnostics_check_uac_label,
    state: { bad: m.diagnostics_check_uac_bad, ok: m.diagnostics_check_uac_ok },
    fix: m.diagnostics_check_uac_fix,
  },
  "windows.webview2": {
    label: m.diagnostics_check_webview2_label,
    state: {
      bad: m.diagnostics_check_webview2_bad,
      warn: m.diagnostics_check_webview2_warn,
    },
    fix: m.diagnostics_check_webview2_fix,
  },
  "system.arch": {
    label: m.diagnostics_check_arch_label,
    state: { bad: m.diagnostics_check_arch_bad },
    fix: m.diagnostics_check_arch_fix,
  },
  "system.dotnet_framework": {
    label: m.diagnostics_check_dotnet_label,
    state: { warn: m.diagnostics_check_dotnet_warn },
    fix: m.diagnostics_check_dotnet_fix,
  },
  "process.manager_not_admin": {
    label: m.diagnostics_check_manager_admin_label,
    state: {
      bad: m.diagnostics_check_manager_admin_bad,
      ok: m.diagnostics_check_manager_admin_ok,
    },
    fix: m.diagnostics_check_manager_admin_fix,
  },
  "process.league_not_running": {
    label: m.diagnostics_check_league_running_label,
    state: {
      warn: m.diagnostics_check_league_running_warn,
      ok: m.diagnostics_check_league_running_ok,
    },
    fix: m.diagnostics_check_league_running_fix,
  },
  "compat_flags.layers": {
    label: m.diagnostics_check_compat_flags_label,
    state: {
      bad: m.diagnostics_check_compat_flags_bad,
      warn: m.diagnostics_check_compat_flags_warn,
      ok: m.diagnostics_check_compat_flags_ok,
    },
    fix: m.diagnostics_check_compat_flags_fix,
  },
  "paths.league.exists": {
    label: m.diagnostics_check_league_path_label,
    state: { bad: m.diagnostics_check_league_path_bad },
    fix: m.diagnostics_check_league_path_fix,
  },
  "paths.league.writable": {
    label: m.diagnostics_check_league_writable_label,
    fix: m.diagnostics_check_league_writable_fix,
  },
  "paths.storage.exists": {
    label: m.diagnostics_check_storage_path_label,
    state: {
      bad: m.diagnostics_check_storage_path_bad,
      warn: m.diagnostics_check_storage_path_warn,
    },
    fix: m.diagnostics_check_storage_path_fix,
  },
  "paths.storage.writable": {
    label: m.diagnostics_check_storage_writable_label,
    fix: m.diagnostics_check_storage_writable_fix,
  },
  "paths.storage.not_in_league": {
    label: m.diagnostics_check_storage_in_league_label,
    state: { bad: m.diagnostics_check_storage_in_league_bad },
    fix: m.diagnostics_check_storage_in_league_fix,
  },
  "paths.free_space": {
    label: m.diagnostics_check_free_space_label,
    state: {
      bad: m.diagnostics_check_free_space_bad,
      warn: m.diagnostics_check_free_space_warn,
    },
    fix: m.diagnostics_check_free_space_fix,
  },
  "storage.medium": {
    label: m.diagnostics_check_storage_medium_label,
    state: { warn: m.diagnostics_check_storage_medium_warn },
    fix: m.diagnostics_check_storage_medium_fix,
  },
  "library.index": {
    label: m.diagnostics_check_library_index_label,
    state: { bad: m.diagnostics_check_library_index_bad, ok: m.diagnostics_check_library_index_ok },
    fix: m.diagnostics_check_library_index_fix,
  },
};

/** What a row is called. */
export function checkLabel(check: Check): string {
  return COPY[check.id]?.label() ?? check.label;
}

/** The row's right-hand text, in the reader's language where this table has it. */
export function checkState(check: Check): string {
  return COPY[check.id]?.state?.[check.severity]?.() ?? check.summary;
}

/** What to do about a check that is not ok, or `null` when it passed. */
export function checkFix(check: Check): string | null {
  if (check.severity === "ok") return null;

  return COPY[check.id]?.fix?.() ?? check.suggestion ?? null;
}
