import { DotsThreeVerticalIcon, FolderOpenIcon } from "@phosphor-icons/react";
import { useState } from "react";

import { Menu, Tooltip, useToast } from "@/components";
import { m } from "@/i18n";
import { api, unwrap } from "@/lib/tauri";
import { twMerge } from "@/utils";

import { cellActive, cellInactive, cellLabeled } from "./cells";

/** The titlebar actions that report no state of their own, behind one cell. */
export function AppMenu() {
  const toast = useToast();
  const [isOpen, setIsOpen] = useState(false);

  async function handleOpenStorageDirectory() {
    try {
      const result = await api.getStorageDirectory();
      const path = unwrap(result);
      await api.revealInExplorer(path);
    } catch (error: unknown) {
      toast.error(
        m.shell_menu_storage_error_title(),
        error instanceof Error ? error.message : String(error),
      );
    }
  }

  return (
    <Menu.Root open={isOpen} onOpenChange={setIsOpen}>
      <Tooltip content={m.shell_menu_label()}>
        <Menu.Trigger
          aria-label={m.shell_menu_label()}
          data-ui="TitleBar:appMenu"
          className={twMerge(cellLabeled, isOpen ? cellActive : cellInactive)}
        >
          <DotsThreeVerticalIcon weight="bold" className="h-4 w-4" />
          <span className="hidden text-xs md:inline">{m.shell_menu_label()}</span>
        </Menu.Trigger>
      </Tooltip>

      <Menu.Portal>
        <Menu.Positioner sideOffset={0}>
          <Menu.Popup className="min-w-52">
            <Menu.Item
              icon={<FolderOpenIcon className="h-4 w-4" />}
              onClick={handleOpenStorageDirectory}
            >
              {m.shell_menu_storage_action()}
            </Menu.Item>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
