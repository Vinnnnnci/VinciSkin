import { FileTextIcon, LifebuoyIcon, ScrollIcon } from "@phosphor-icons/react";
import { useState } from "react";

import { Button, ExternalLink, SectionCard } from "@/components";
import { m } from "@/i18n";
import { type AppInfo, revealPath } from "@/lib/tauri";

import { LicensesDialog } from "./LicensesDialog";
import { SupportReportDialog } from "./SupportReportDialog";

const SPONSOR_URL = "https://ifdian.net/a/Vinnnnnci";

interface AboutSectionProps {
  appInfo: AppInfo | undefined;
}

export function AboutSection({ appInfo }: AboutSectionProps) {
  const [licensesOpen, setLicensesOpen] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);

  return (
    <SectionCard title={m.settings_about_title()}>
      <div>
        <div className="flex items-center justify-between">
          <div>
            <h4 className="font-medium text-surface-100">{m.settings_about_app_name()}</h4>
            {appInfo && (
              <p className="text-sm text-surface-500">
                {m.settings_about_version_label({ version: appInfo.version })}
              </p>
            )}
          </div>
          <div className="flex items-center gap-2">
            {appInfo?.logFilePath && (
              <Button variant="outline" size="sm" onClick={() => revealPath(appInfo.logFilePath!)}>
                <FileTextIcon className="h-4 w-4" weight="bold" />
                {m.settings_about_open_log_action()}
              </Button>
            )}
            <Button variant="outline" size="sm" onClick={() => setReportOpen(true)}>
              <LifebuoyIcon className="h-4 w-4" weight="bold" />
              {m.settings_about_report_action()}
            </Button>
          </div>
        </div>
        <p className="mt-3 text-sm text-surface-400">{m.settings_about_description()}</p>
        <div className="mt-4 flex items-center gap-4 border-t border-surface-600 pt-4">
          <p className="text-xs text-surface-500 select-none">
            {m.settings_about_free_copy_hint()}
          </p>
          <div className="ml-auto flex shrink-0 items-center gap-4">
            <ExternalLink href={SPONSOR_URL} className="text-sm">
              {m.settings_about_support_action()}
            </ExternalLink>
            <Button variant="ghost" size="sm" onClick={() => setLicensesOpen(true)}>
              <ScrollIcon className="h-4 w-4" weight="bold" />
              {m.settings_about_licenses_action()}
            </Button>
          </div>
        </div>
      </div>
      <LicensesDialog open={licensesOpen} onOpenChange={setLicensesOpen} />
      <SupportReportDialog open={reportOpen} onOpenChange={setReportOpen} />
    </SectionCard>
  );
}
