import { FileTextIcon, LifebuoyIcon } from "@phosphor-icons/react";
import { type KeyboardEvent, useEffect, useState } from "react";

import { Button, FieldControl, SectionCard, Switch } from "@/components";
import { m } from "@/i18n";
import type { Settings } from "@/lib/tauri";
import { supportReportDialog } from "@/stores/supportReportDialog";

import { SettingGroup } from "./SettingGroup";
import { SettingRow } from "./SettingRow";

interface IncidentSectionProps {
  settings: Settings;
  onSave: (settings: Settings) => void;
}

/**
 * What the incident reporter reads and how much it keeps.
 *
 * These two rows were on the patching tab. Everything else on it went with the
 * patcher (ADR-0058); an incident still records how the game ended, so these
 * stay, under ids that moved to the general namespace.
 */
export function IncidentSection({ settings, onSave }: IncidentSectionProps) {
  return (
    <SectionCard title="Diagnostics" icon={<FileTextIcon className="h-5 w-5" />}>
      <SettingGroup id="general.incidents" title="Incidents">
        <SettingRow
          setting="readGameLog"
          description="The incident reporter reads the game log to see what went wrong."
          hint="Turn this off to keep the manager from opening anything under the League install. An incident still records how the game ended."
          control={
            <Switch
              checked={settings.readGameLog}
              onCheckedChange={(checked) => onSave({ ...settings, readGameLog: checked })}
            />
          }
        />

        <SettingRow
          setting="keepIncidents"
          description="How many incidents to keep"
          hint="The newest are kept, under 1 MB together, and the oldest goes first."
          control={
            <KeepIncidentsField
              value={settings.keepIncidents}
              onCommit={(keepIncidents) => onSave({ ...settings, keepIncidents })}
            />
          }
        />

        <SettingRow
          setting="verbosePatcherLogging"
          description="The injection loader's full output goes into the log"
          hint="For a skin that says it applied while the game looks unchanged: turn this on, reproduce it once, then send a support report."
          control={
            <Switch
              checked={settings.verbosePatcherLogging}
              onCheckedChange={(checked) => onSave({ ...settings, verbosePatcherLogging: checked })}
            />
          }
        />
      </SettingGroup>

      <SettingGroup id="general.supportReport" title="Support report">
        <SettingRow
          kind="action"
          title="Send what went wrong"
          description={m.support_report_section_description()}
          control={
            <Button
              variant="outline"
              size="sm"
              onClick={() => supportReportDialog.getState().open()}
            >
              <LifebuoyIcon className="h-4 w-4" weight="bold" />
              {m.support_report_action()}
            </Button>
          }
        />
      </SettingGroup>
    </SectionCard>
  );
}

const MIN_KEPT_INCIDENTS = 1;
const MAX_KEPT_INCIDENTS = 200;

interface KeepIncidentsFieldProps {
  value: number;
  onCommit: (value: number) => void;
}

/**
 * A count that commits on blur or Enter, clamped to the store's range.
 *
 * A keystroke is not a save, because typing `150` would otherwise write `1`
 * and `15` on the way there.
 */
function KeepIncidentsField({ value, onCommit }: KeepIncidentsFieldProps) {
  const [draft, setDraft] = useState(String(value));

  useEffect(() => {
    setDraft(String(value));
  }, [value]);

  function commit() {
    const parsed = draft.trim() === "" ? Number.NaN : Math.round(Number(draft));
    if (!Number.isFinite(parsed)) {
      setDraft(String(value));
      return;
    }
    const next = Math.min(MAX_KEPT_INCIDENTS, Math.max(MIN_KEPT_INCIDENTS, parsed));
    setDraft(String(next));
    if (next !== value) onCommit(next);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") event.currentTarget.blur();
    if (event.key === "Escape") setDraft(String(value));
  }

  return (
    <FieldControl
      type="number"
      inputMode="numeric"
      min={MIN_KEPT_INCIDENTS}
      max={MAX_KEPT_INCIDENTS}
      step={1}
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={handleKeyDown}
      aria-label="Keep incidents"
      className="w-20 px-2.5 text-right tabular-nums"
    />
  );
}
