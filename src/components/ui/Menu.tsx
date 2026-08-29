import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import type { ReactNode } from "react";

export interface MenuItem {
  label: string;
  disabled?: boolean;
  onSelect?: () => void;
}

export function Menu({
  trigger,
  label,
  items,
}: {
  trigger: ReactNode;
  label: string;
  items: MenuItem[];
}) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger aria-label={label} asChild>
        {trigger}
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content align="end" className="ui-menu-content" sideOffset={5}>
          {items.map((item) => (
            <DropdownMenu.Item
              className="ui-menu-item"
              disabled={item.disabled}
              key={item.label}
              onSelect={item.onSelect}
            >
              {item.label}
            </DropdownMenu.Item>
          ))}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
