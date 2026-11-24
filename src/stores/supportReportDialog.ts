import { createDialogStore } from "./createDialogStore";

/**
 * The support report, raised from the About card or from an error toast.
 *
 * It is a store rather than the card's own state because a toast outlives the screen that
 * raised it: the failure can happen on the library screen, and the report still has to open
 * over it.
 */
export const supportReportDialog = createDialogStore();
