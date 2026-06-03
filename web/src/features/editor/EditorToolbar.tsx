import { useState, type ReactNode } from "react";

import type { Editor } from "@tiptap/react";

type ToolbarBtnProps = {
  testId: string;
  label: string;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
};

function Btn({ testId, label, active, disabled, onClick, children }: ToolbarBtnProps) {
  return (
    <button
      type="button"
      data-testid={testId}
      title={label}
      aria-label={label}
      aria-pressed={active}
      disabled={disabled}
      onClick={onClick}
      style={{
        padding: "4px 8px",
        border: "1px solid #e5e5e5",
        background: active ? "#e5e5ff" : "white",
        cursor: disabled ? "not-allowed" : "pointer",
        fontWeight: active ? 600 : 400,
        minWidth: 28,
      }}
    >
      {children}
    </button>
  );
}

function Sep() {
  return <span style={{ borderLeft: "1px solid #e5e5e5", margin: "0 4px" }} />;
}

export function EditorToolbar({ editor }: { editor: Editor | null }) {
  const [linkOpen, setLinkOpen] = useState(false);
  const [linkUrl, setLinkUrl] = useState("");

  if (!editor) return null;
  const c = () => editor.chain().focus();
  return (
    <div
      data-testid="editor-toolbar"
      style={{ display: "flex", gap: 4, flexWrap: "wrap", marginBottom: 8, position: "relative" }}
    >
      <Btn testId="toolbar-bold" label="Bold" active={editor.isActive("bold")}
        onClick={() => c().toggleBold().run()}>B</Btn>
      <Btn testId="toolbar-italic" label="Italic" active={editor.isActive("italic")}
        onClick={() => c().toggleItalic().run()}>I</Btn>
      <Btn testId="toolbar-strike" label="Strike" active={editor.isActive("strike")}
        onClick={() => c().toggleStrike().run()}>S</Btn>
      <Btn testId="toolbar-code" label="Inline code" active={editor.isActive("code")}
        onClick={() => c().toggleCode().run()}>{"</>"}</Btn>
      <Sep />
      <Btn testId="toolbar-h1" label="Heading 1" active={editor.isActive("heading", { level: 1 })}
        onClick={() => c().toggleHeading({ level: 1 }).run()}>H1</Btn>
      <Btn testId="toolbar-h2" label="Heading 2" active={editor.isActive("heading", { level: 2 })}
        onClick={() => c().toggleHeading({ level: 2 }).run()}>H2</Btn>
      <Btn testId="toolbar-h3" label="Heading 3" active={editor.isActive("heading", { level: 3 })}
        onClick={() => c().toggleHeading({ level: 3 }).run()}>H3</Btn>
      <Sep />
      <Btn testId="toolbar-bullet-list" label="Bullet list"
        active={editor.isActive("bulletList")} onClick={() => c().toggleBulletList().run()}>•</Btn>
      <Btn testId="toolbar-ordered-list" label="Ordered list"
        active={editor.isActive("orderedList")} onClick={() => c().toggleOrderedList().run()}>1.</Btn>
      <Btn testId="toolbar-blockquote" label="Quote"
        active={editor.isActive("blockquote")} onClick={() => c().toggleBlockquote().run()}>❝</Btn>
      <Btn testId="toolbar-code-block" label="Code block"
        active={editor.isActive("codeBlock")} onClick={() => c().toggleCodeBlock().run()}>{"{}"}</Btn>
      <Sep />
      <Btn
        testId="toolbar-link"
        label="Link"
        active={editor.isActive("link")}
        onClick={() => {
          const current = editor.getAttributes("link").href as string | undefined;
          setLinkUrl(current ?? "");
          setLinkOpen(true);
        }}
      >
        🔗
      </Btn>
      {linkOpen && (
        <div
          data-testid="link-popover"
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            marginTop: 4,
            background: "white",
            border: "1px solid #e5e5e5",
            borderRadius: 4,
            padding: 8,
            display: "flex",
            gap: 4,
            zIndex: 10,
            boxShadow: "0 4px 12px rgba(0,0,0,0.1)",
          }}
          onKeyDown={(e) => { if (e.key === "Escape") setLinkOpen(false); }}
        >
          <input
            data-testid="link-input"
            type="url"
            value={linkUrl}
            onChange={(e) => setLinkUrl(e.target.value)}
            placeholder="https://"
            style={{ padding: 4, minWidth: 240, border: "1px solid #e5e5e5" }}
            autoFocus
          />
          <button
            data-testid="link-apply"
            type="button"
            onClick={() => {
              if (linkUrl) c().extendMarkRange("link").setLink({ href: linkUrl }).run();
              else c().unsetLink().run();
              setLinkOpen(false);
            }}
          >Apply</button>
          <button
            data-testid="link-remove"
            type="button"
            onClick={() => {
              c().unsetLink().run();
              setLinkOpen(false);
            }}
          >Remove</button>
        </div>
      )}
    </div>
  );
}
