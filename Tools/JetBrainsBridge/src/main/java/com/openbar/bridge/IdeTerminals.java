// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import com.intellij.ide.impl.ProjectUtil;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ApplicationNamesInfo;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.diagnostic.Logger;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.project.ProjectManager;
import com.intellij.openapi.wm.ToolWindow;
import com.intellij.openapi.wm.ToolWindowManager;
import com.intellij.openapi.wm.WindowManager;
import com.intellij.terminal.ui.TerminalWidget;
import com.intellij.ui.content.Content;
import com.intellij.ui.content.ContentManager;
import com.jediterm.terminal.ProcessTtyConnector;
import com.jediterm.terminal.TtyConnector;
import java.awt.Frame;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Supplier;
import javax.swing.JComponent;
import javax.swing.JFrame;
import org.jetbrains.plugins.terminal.ShellTerminalWidget;
import org.jetbrains.plugins.terminal.TerminalToolWindowManager;
import org.jetbrains.plugins.terminal.ui.TerminalContainer;

/**
 * The terminal tabs of every project open in this IDE, whichever engine
 * runs them: the classic one (JediTerm, a pty the bridge writes to) or the
 * reworked one the IDE turns on by default since 2025.3 ({@link ReworkedTabs}).
 */
final class IdeTerminals implements Terminals {
    private static final Logger LOG = Logger.getInstance(IdeTerminals.class);
    static final String TOOL_WINDOW = "Terminal";

    @Override
    public Terminal find(Set<Long> shellPids) {
        if (shellPids.isEmpty()) return null;
        return onEdt(() -> {
            for (Project project : ProjectManager.getInstance().getOpenProjects()) {
                if (project.isDisposed()) continue;
                Terminal found = classic(project, shellPids);
                if (found == null) found = ReworkedTabs.find(project, shellPids, this);
                if (found != null) return found;
            }
            return null;
        });
    }

    @Override
    public List<String> projects() {
        return onEdt(() -> {
            List<String> folders = new ArrayList<>();
            for (Project project : ProjectManager.getInstance().getOpenProjects()) {
                if (!project.isDisposed() && project.getBasePath() != null) folders.add(project.getBasePath());
            }
            return folders;
        });
    }

    @Override
    public String ide() {
        return ApplicationNamesInfo.getInstance().getFullProductName();
    }

    /** A tab of the classic terminal, found by the pid of the process its pty runs. */
    private Terminal classic(Project project, Set<Long> shellPids) {
        TerminalToolWindowManager manager;
        try {
            manager = TerminalToolWindowManager.getInstance(project);
        } catch (RuntimeException | LinkageError unavailable) {
            return null;
        }
        for (TerminalWidget widget : manager.getTerminalWidgets()) {
            TtyConnector tty = widget.getTtyConnector();
            ProcessTtyConnector process = tty == null ? null : ShellTerminalWidget.getProcessTtyConnector(tty);
            if (process == null || !shellPids.contains(process.getProcess().pid())) continue;
            TerminalContainer container = manager.getContainer(widget);
            Content content = container != null ? container.getContent() : null;
            return new Tab(project, content, widget.getComponent()) {
                @Override
                public void type(String text) throws IOException {
                    tty.write(text);
                }

                @Override
                public void paste(String text) throws IOException {
                    tty.write(Protocol.ESC + "[200~" + text + Protocol.ESC + "[201~");
                }

                @Override
                void requestFocus() {
                    widget.requestFocus();
                }
            };
        }
        return null;
    }

    /** What every tab shares, whatever engine types into it. */
    abstract static class Tab implements Terminal {
        final Project project;
        /** The tab in the terminal tool window, or null when the terminal sits elsewhere (an editor tab). */
        final Content content;
        final JComponent component;

        Tab(Project project, Content content, JComponent component) {
            this.project = project;
            this.content = content;
            this.component = component;
        }

        abstract void requestFocus();

        @Override
        public void focus() {
            onEdt(() -> {
                ProjectUtil.focusProjectWindow(project, true);
                ToolWindow window = ToolWindowManager.getInstance(project).getToolWindow(TOOL_WINDOW);
                ContentManager tabs = content != null ? content.getManager() : null;
                if (tabs != null && tabs.getIndexOfContent(content) >= 0) tabs.setSelectedContent(content, true);
                if (window != null) window.activate(this::requestFocus, true, true);
                else requestFocus();
                return null;
            });
        }

        @Override
        public boolean shown() {
            return onEdt(() -> {
                JFrame frame = WindowManager.getInstance().getFrame(project);
                if (frame == null || !frame.isShowing() || (frame.getExtendedState() & Frame.ICONIFIED) != 0) return false;
                ToolWindow window = ToolWindowManager.getInstance(project).getToolWindow(TOOL_WINDOW);
                boolean active = frame.isActive() || (window != null && window.isActive());
                if (!active || window == null || !window.isVisible()) return false;
                ContentManager tabs = content != null ? content.getManager() : null;
                return tabs != null && tabs.getSelectedContent() == content && component.isShowing();
            });
        }
    }

    /** Runs on the IDE's UI thread, whatever dialog is open, and gives its result back. */
    static <T> T onEdt(Supplier<T> work) {
        if (ApplicationManager.getApplication().isDispatchThread()) return work.get();
        AtomicReference<T> result = new AtomicReference<>();
        AtomicReference<RuntimeException> failure = new AtomicReference<>();
        ApplicationManager.getApplication().invokeAndWait(() -> {
            try {
                result.set(work.get());
            } catch (RuntimeException error) {
                LOG.warn("Openbar bridge", error);
                failure.set(error);
            }
        }, ModalityState.any());
        if (failure.get() != null) throw failure.get();
        return result.get();
    }
}
