// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// The Scratchpad section (NotchScratchpadView.swift; spec 05 §11): the pad's
// plain text fills the page on a control surface, or its formatted reading;
// the top row holds the tabs, a new-pad button and the pad's actions.
// Ctrl+T opens a pad, Ctrl+W closes one, Ctrl+Enter flips the reading.

import { Check, ChevronDown, Copy, Eye, Pencil, Plus, TriangleAlert, X } from "lucide-react";
import { type KeyboardEvent, type MouseEvent, useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useIsland } from "../island/store";
import type { PageProps } from "../island/views/Expanded";
import { EmptyState } from "../island/views/Controls";
import { bridge } from "../shared/bridge";
import { Surface } from "../shared/ui/controls";
import { type MenuEntry, useMenu } from "../shared/ui/Menu";
import { useReorder } from "../shared/ui/reorder";
import { Sheet } from "../tools/Sheet";
import {
  MAX_NAME,
  MAX_PADS,
  addPad,
  closeNeedsConfirmation,
  exportFileName,
  listContinuation,
  removePad,
  renamePad,
  reorderPads,
  sanitizedTextSize,
  selectedPad,
  toggleTaskAt,
  withSelectedText,
} from "./model";
import { S } from "./strings";
import { flush, openScratchpad, update, useScratchpad } from "./store";
import type { Pad } from "./types";

/** The editor, so tab changes can aim the caret and Clear can go through its undo. */
const editor: { current: HTMLTextAreaElement | null } = { current: null };

const reduceMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function createPad() {
  const { doc } = useScratchpad.getState();
  const next = doc && addPad(doc, S.pageTitle);
  if (next) update(next);
}

function selectPadById(id: string) {
  const { doc } = useScratchpad.getState();
  if (doc && doc.selectedId !== id) update({ ...doc, selectedId: id });
}

/** An empty pad closes at once; one with text asks first. */
function requestClose(pad: Pad) {
  const { doc } = useScratchpad.getState();
  if (!doc || doc.pads.length < 2) return;
  if (closeNeedsConfirmation(pad)) useScratchpad.setState({ closing: pad.id });
  else update(removePad(doc, pad.id) ?? doc);
}

/** Through the editor's undo, so Ctrl+Z brings the text back; from the reading, back to the editor first. */
function clearText() {
  const clear = () => {
    const area = editor.current;
    if (!area) return;
    area.focus();
    area.select();
    document.execCommand("delete");
  };
  if (!useScratchpad.getState().previewing) return clear();
  useScratchpad.setState({ previewing: false });
  requestAnimationFrame(clear);
}

async function exportText() {
  const { doc } = useScratchpad.getState();
  const pad = doc && selectedPad(doc);
  if (!pad?.text) return;
  try {
    await bridge.scratchpad.export(exportFileName(pad.name, new Date()), pad.text);
  } catch {
    useIsland.getState().showNotice({
      id: `scratchpad-export:${Date.now()}`,
      kind: "scratchpad",
      title: S.exportFailed,
      priority: 1,
      duration: 3,
    });
  }
}

function padMenu(pad: Pad, count: number): MenuEntry[] {
  return [
    { label: count >= MAX_PADS ? S.padLimit(MAX_PADS) : S.newPad, disabled: count >= MAX_PADS, onSelect: createPad },
    { label: S.renamePad, onSelect: () => useScratchpad.setState({ renaming: pad.id }) },
    { label: S.closePad, disabled: count < 2, destructive: true, onSelect: () => requestClose(pad) },
    { kind: "separator" },
    { label: S.exportAction, disabled: !pad.text, onSelect: () => void exportText() },
    { label: S.clearAction, disabled: !pad.text, onSelect: clearText },
  ];
}

// MARK: Page

export function ScratchpadPage({ height }: PageProps) {
  useLocale();
  const doc = useScratchpad((s) => s.doc);
  const loadFailed = useScratchpad((s) => s.loadFailed);
  const saveFailed = useScratchpad((s) => s.saveFailed);
  const previewing = useScratchpad((s) => s.previewing);
  const renaming = useScratchpad((s) => s.renaming);
  const closing = useScratchpad((s) => s.closing);
  const textSize = useIsland((s) => sanitizedTextSize(s.settings.scratchpadTextSize));
  const pad = doc ? selectedPad(doc) : undefined;

  useEffect(() => {
    void openScratchpad();
    // Quitting with an edit still waiting on its 0.8 s save.
    const leaving = () => void flush();
    window.addEventListener("pagehide", leaving);
    return () => {
      window.removeEventListener("pagehide", leaving);
      void flush();
    };
  }, []);

  // A new or chosen pad puts the caret at the end of its text.
  useEffect(() => {
    const area = editor.current;
    if (!area || previewing) return;
    area.focus();
    area.setSelectionRange(area.value.length, area.value.length);
    area.scrollTop = area.scrollHeight;
  }, [doc?.selectedId, previewing]);

  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey || document.querySelector(".tl-sheet")) return;
      const s = useScratchpad.getState();
      const current = s.doc && selectedPad(s.doc);
      if (e.key.toLowerCase() === "t") {
        e.preventDefault();
        createPad();
      } else if (e.key.toLowerCase() === "w" && current) {
        e.preventDefault();
        // With one pad left, Ctrl+W puts the pad away, as on the Mac.
        if ((s.doc?.pads.length ?? 0) > 1) requestClose(current);
        else useIsland.getState().collapse();
      } else if (e.key === "Enter" && current?.text) {
        e.preventDefault();
        useScratchpad.setState({ previewing: !s.previewing });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (loadFailed) return <EmptyState icon={TriangleAlert} title={S.loadFailed} />;
  if (!doc || !pad) return null;

  const onEditorKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key !== "Enter" || e.ctrlKey || e.shiftKey || e.altKey || e.nativeEvent.isComposing) return;
    const area = e.currentTarget;
    if (area.selectionStart !== area.selectionEnd) return;
    const edit = listContinuation(area.value, area.selectionStart);
    if (!edit) return;
    e.preventDefault();
    area.setSelectionRange(edit.from, edit.to);
    // execCommand keeps the edit on the editor's own undo stack.
    document.execCommand(edit.insert ? "insertText" : "delete", false, edit.insert);
  };

  return (
    <div className="sp-page" style={{ height }}>
      {saveFailed ? (
        <div className="sp-save-failed">
          <TriangleAlert size={11} aria-hidden />
          {S.saveFailed}
        </div>
      ) : null}
      <Surface radius={12} className="sp-surface" style={{ fontSize: textSize }}>
        {previewing ? (
          <Preview text={pad.text} />
        ) : (
          <textarea
            ref={(el) => {
              editor.current = el;
            }}
            className="sp-editor"
            value={pad.text}
            placeholder={S.placeholder}
            aria-label={pad.name}
            spellCheck
            onChange={(e) => update(withSelectedText(doc, e.target.value, Date.now()))}
            onKeyDown={onEditorKey}
          />
        )}
      </Surface>
      {renaming ? (
        <Sheet
          title={S.renamePad}
          confirm={S.saveName}
          cancel={S.cancel}
          field={{ value: doc.pads.find((p) => p.id === renaming)?.name ?? "", label: S.name, maxLength: MAX_NAME }}
          onConfirm={(name) => {
            const next = renamePad(doc, renaming, name);
            if (next) update(next);
            useScratchpad.setState({ renaming: null });
          }}
          onCancel={() => useScratchpad.setState({ renaming: null })}
        />
      ) : null}
      {closing ? (
        <Sheet
          title={S.closePad}
          message={S.deletePad(doc.pads.find((p) => p.id === closing)?.name ?? "")}
          confirm={S.closePad}
          cancel={S.cancel}
          destructive
          onConfirm={() => {
            update(removePad(doc, closing) ?? doc);
            useScratchpad.setState({ closing: null });
          }}
          onCancel={() => useScratchpad.setState({ closing: null })}
        />
      ) : null}
    </div>
  );
}

/** The formatted reading; its task boxes edit the text they were drawn from. */
function Preview({ text }: { text: string }) {
  useLocale();
  const toggle = (offset: number) => {
    const { doc } = useScratchpad.getState();
    if (doc) update(withSelectedText(doc, toggleTaskAt(text, offset), Date.now()));
  };
  return (
    <div className="sp-preview">
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          li({ node, className, children }) {
            const offset = node?.position?.start.offset;
            const onClick = (e: MouseEvent<HTMLLIElement>) => {
              const box = e.target as HTMLElement;
              if (box.tagName !== "INPUT" || box.closest("li") !== e.currentTarget || offset === undefined) return;
              e.preventDefault();
              toggle(offset);
            };
            return (
              <li className={className} onClick={onClick}>
                {children}
              </li>
            );
          },
          input({ checked }) {
            return (
              <input type="checkbox" checked={!!checked} readOnly aria-label={checked ? S.taskDone : S.taskNotDone} />
            );
          },
        }}
      >
        {text}
      </Markdown>
    </div>
  );
}

// MARK: Top row

export function ScratchpadLeading() {
  useLocale();
  const doc = useScratchpad((s) => s.doc);
  const loadFailed = useScratchpad((s) => s.loadFailed);
  const menu = useMenu();
  const [hovered, setHovered] = useState<string | null>(null);
  const strip = useRef<HTMLDivElement>(null);
  const ids = doc?.pads.map((p) => p.id) ?? [];
  const reorder = useReorder<string>({
    ids,
    axis: "x",
    onReorder(next) {
      const current = useScratchpad.getState().doc;
      if (current) update(reorderPads(current, next));
    },
  });

  // The chosen tab scrolls to the middle of the strip.
  useEffect(() => {
    const tab = strip.current?.querySelector<HTMLElement>(".sp-tab.is-selected");
    tab?.scrollIntoView({ inline: "center", block: "nearest", behavior: reduceMotion() ? "auto" : "smooth" });
  }, [doc?.selectedId]);

  if (!doc || loadFailed) return null;
  const canCreate = doc.pads.length < MAX_PADS;
  const canClose = doc.pads.length > 1;

  return (
    <div className="sp-leading">
      <div ref={strip} className="sp-tabs" role="tablist" aria-label={S.pageTitle}>
        {doc.pads.map((pad) => {
          const selected = pad.id === doc.selectedId;
          const showsClose = !selected && canClose && hovered === pad.id;
          return (
            <div
              key={pad.id}
              className={`sp-tab${selected ? " is-selected" : ""}${reorder.moving === pad.id ? " is-moving" : ""}`}
              {...reorder.itemProps(pad.id)}
              onPointerEnter={() => setHovered(pad.id)}
              onPointerLeave={() => setHovered((h) => (h === pad.id ? null : h))}
              onContextMenu={(e) => {
                e.preventDefault();
                menu.open({ x: e.clientX, y: e.clientY }, [
                  { label: S.renamePad, onSelect: () => useScratchpad.setState({ renaming: pad.id }) },
                  { label: S.closePad, disabled: !canClose, destructive: true, onSelect: () => requestClose(pad) },
                ]);
              }}
            >
              <button
                type="button"
                role="tab"
                aria-selected={selected}
                className="sp-tab-label"
                title={selected ? S.padActions : pad.name}
                aria-haspopup={selected ? "menu" : undefined}
                onClick={(e) => {
                  if (selected) menu.openBelow(e.currentTarget, padMenu(pad, doc.pads.length), false);
                  else selectPadById(pad.id);
                }}
              >
                <span className="sp-tab-name">{pad.name}</span>
                {selected ? <ChevronDown size={9} strokeWidth={2.75} aria-hidden /> : null}
              </button>
              {showsClose ? (
                <button
                  type="button"
                  className="sp-tab-close"
                  aria-label={`${S.closePad}: ${pad.name}`}
                  title={S.closePad}
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={() => requestClose(pad)}
                >
                  <X size={9} strokeWidth={2.75} />
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
      <button
        type="button"
        className="n-icon-button"
        aria-label={canCreate ? S.newPad : S.padLimit(MAX_PADS)}
        title={canCreate ? S.newPad : S.padLimit(MAX_PADS)}
        disabled={!canCreate}
        onClick={createPad}
      >
        <Plus size={14} strokeWidth={2} />
      </button>
      {menu.menu}
    </div>
  );
}

export function ScratchpadTrailing() {
  useLocale();
  const doc = useScratchpad((s) => s.doc);
  const previewing = useScratchpad((s) => s.previewing);
  const loadFailed = useScratchpad((s) => s.loadFailed);
  const [copied, setCopied] = useState(false);
  const pad = doc ? selectedPad(doc) : undefined;

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1600);
    return () => clearTimeout(timer);
  }, [copied]);

  if (!pad || loadFailed) return null;
  const empty = !pad.text;
  return (
    <div className="sp-trailing">
      <button
        type="button"
        className={`n-icon-button${previewing ? " is-selected" : ""}`}
        aria-label={previewing ? S.editText : S.previewFormatting}
        title={previewing ? S.editText : S.previewFormatting}
        aria-pressed={previewing}
        disabled={empty && !previewing}
        onClick={() => useScratchpad.setState({ previewing: !previewing })}
      >
        {previewing ? <Pencil size={14} strokeWidth={2} /> : <Eye size={14} strokeWidth={2} />}
      </button>
      <button
        type="button"
        className="n-icon-button"
        aria-label={copied ? S.copied : S.copyAll}
        title={copied ? S.copied : S.copyAll}
        disabled={empty}
        onClick={() => {
          bridge.tools.copyText(pad.text).then(
            () => setCopied(true),
            () => setCopied(false),
          );
        }}
      >
        {copied ? <Check size={14} strokeWidth={2} /> : <Copy size={14} strokeWidth={2} />}
      </button>
    </div>
  );
}
