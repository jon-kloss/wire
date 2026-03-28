import { useState, useEffect, useRef } from "react";

interface PromptModalProps {
  title: string;
  defaultValue: string;
  placeholder?: string;
  onConfirm: (value: string) => void;
  onCancel: () => void;
}

export function PromptModal({
  title,
  defaultValue,
  placeholder,
  onConfirm,
  onCancel,
}: PromptModalProps) {
  const [value, setValue] = useState(defaultValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const handleSubmit = () => {
    if (value.trim()) {
      onConfirm(value);
    }
  };

  return (
    <div
      className="prompt-backdrop"
      data-testid="prompt-backdrop"
      onClick={onCancel}
    >
      <div className="prompt-dialog" onClick={(e) => e.stopPropagation()}>
        <label className="prompt-title">{title}</label>
        <input
          ref={inputRef}
          className="prompt-input"
          type="text"
          value={value}
          placeholder={placeholder}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSubmit();
            if (e.key === "Escape") onCancel();
          }}
        />
        <div className="prompt-actions">
          <button className="prompt-btn prompt-cancel" onClick={onCancel}>
            Cancel
          </button>
          <button className="prompt-btn prompt-ok" onClick={handleSubmit}>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
