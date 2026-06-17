/* Wire dark Monaco theme — derived from the design tokens' code-syntax palette.
   Monaco themes can't read CSS vars, so the fixed (non-accent) token hexes are
   inlined here. Call defineWireMonacoTheme(monaco) before mounting an editor,
   then set theme="wire-dark". */

export const WIRE_MONACO_THEME = "wire-dark";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function defineWireMonacoTheme(monaco: any): void {
  monaco.editor.defineTheme(WIRE_MONACO_THEME, {
    base: "vs-dark",
    inherit: true,
    rules: [
      // JSON
      { token: "string.key.json", foreground: "7fafe0" }, // --syn-key
      { token: "string.value.json", foreground: "9fcf86" }, // --syn-string
      { token: "number.json", foreground: "d8a657" }, // --syn-number
      { token: "keyword.json", foreground: "d8a657" }, // true/false/null
      { token: "delimiter", foreground: "5b6068" }, // --syn-punct
      { token: "delimiter.bracket.json", foreground: "5b6068" },
      { token: "delimiter.array.json", foreground: "5b6068" },
      { token: "delimiter.colon.json", foreground: "5b6068" },
      // YAML / generic
      { token: "type", foreground: "7fafe0" }, // yaml keys
      { token: "key", foreground: "7fafe0" },
      { token: "string", foreground: "9fcf86" },
      { token: "string.yaml", foreground: "9fcf86" },
      { token: "number", foreground: "d8a657" },
      { token: "keyword", foreground: "d8a657" },
      { token: "comment", foreground: "5a5f67", fontStyle: "italic" },
    ],
    colors: {
      "editor.background": "#0c0d10", // --panel
      "editor.foreground": "#edeef0", // --text-hi
      "editorLineNumber.foreground": "#3a3d43", // --text-faintest
      "editorLineNumber.activeForeground": "#787e87", // --text-dim
      "editor.lineHighlightBackground": "#ffffff08",
      "editor.lineHighlightBorder": "#00000000",
      "editor.selectionBackground": "#33414d",
      "editor.inactiveSelectionBackground": "#262d33",
      "editorCursor.foreground": "#edeef0",
      "editorIndentGuide.background1": "#ffffff0d",
      "editorIndentGuide.activeBackground1": "#ffffff1a",
      "editorWidget.background": "#0a0b0e", // --panel-deep
      "editorWidget.border": "#ffffff12",
      "editorWhitespace.foreground": "#ffffff10",
      "scrollbarSlider.background": "#ffffff14",
      "scrollbarSlider.hoverBackground": "#ffffff22",
      "scrollbarSlider.activeBackground": "#ffffff2e",
    },
  });
}
