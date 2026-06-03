import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ContextMenu } from "./ContextMenu";

describe("ContextMenu", () => {
  it("renders items + calls onSelect + onClose on click", () => {
    const onClose = vi.fn();
    const onSelect = vi.fn();
    render(
      <ContextMenu
        x={10}
        y={20}
        onClose={onClose}
        items={[{ label: "Rename", testId: "ctx-rename", onSelect }]}
      />,
    );
    fireEvent.click(screen.getByTestId("ctx-rename"));
    expect(onSelect).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[]} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("closes on outside mousedown", () => {
    const onClose = vi.fn();
    render(
      <div>
        <button data-testid="outside">outside</button>
        <ContextMenu x={0} y={0} onClose={onClose} items={[]} />
      </div>,
    );
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(onClose).toHaveBeenCalled();
  });
});
