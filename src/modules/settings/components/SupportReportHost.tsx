import { supportReportDialog, useDialog } from "@/stores";

import { SupportReportDialog } from "./SupportReportDialog";

/**
 * The report, raised from anywhere by the store rather than by the card that usually opens it.
 *
 * Mounted at the app root, because the failure that sends someone looking for it happens on
 * whichever screen they were on, not only in Settings.
 */
export function SupportReportHost() {
  const { isOpen, close } = useDialog(supportReportDialog);

  return (
    <SupportReportDialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    />
  );
}
