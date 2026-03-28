import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { PromptModal } from "./PromptModal";

describe("PromptModal", () => {
  it("shows default value in input", () => {
    render(
      <PromptModal
        title="Name:"
        defaultValue="my-request"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(screen.getByRole("textbox")).toHaveProperty("value", "my-request");
  });

  it("calls onConfirm with input value when OK clicked", () => {
    const onConfirm = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "My API" },
    });
    fireEvent.click(screen.getByText("OK"));
    expect(onConfirm).toHaveBeenCalledWith("My API");
  });

  it("calls onCancel when Cancel clicked", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
    fireEvent.click(screen.getByText("Cancel"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("calls onConfirm when Enter pressed", () => {
    const onConfirm = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "test" },
    });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(onConfirm).toHaveBeenCalledWith("test");
  });

  it("calls onCancel when Escape pressed", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("calls onCancel when backdrop clicked", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
    fireEvent.click(screen.getByTestId("prompt-backdrop"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("does not dismiss when clicking inside dialog", () => {
    const onCancel = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />
    );
    fireEvent.click(screen.getByText("Name:"));
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("does not call onConfirm with empty value", () => {
    const onConfirm = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText("OK"));
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("does not call onConfirm with whitespace-only value", () => {
    const onConfirm = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "   " },
    });
    fireEvent.click(screen.getByText("OK"));
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("does not call onConfirm when Enter pressed with empty input", () => {
    const onConfirm = vi.fn();
    render(
      <PromptModal
        title="Name:"
        defaultValue=""
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
