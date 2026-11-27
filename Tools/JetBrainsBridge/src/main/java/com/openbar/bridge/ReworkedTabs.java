// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import com.intellij.openapi.project.Project;
import com.intellij.openapi.wm.ToolWindow;
import com.intellij.openapi.wm.ToolWindowManager;
import com.intellij.ui.content.Content;
import java.lang.reflect.Method;
import java.util.List;
import java.util.Set;
import javax.swing.JComponent;

/**
 * The tabs of the reworked terminal (2025.3 and later). Its classes live in
 * a module of the Terminal plugin with a class loader of its own, which a
 * plugin outside JetBrains can't name, so they are reached through the
 * loader of a tab that is open: a session runs in one, or there is nothing
 * to find. Each IDE that lacks them, or names them otherwise, simply has
 * none of these tabs.
 */
final class ReworkedTabs {
    private static final String MANAGER = "com.intellij.terminal.frontend.toolwindow.TerminalToolWindowTabsManager";
    private static final String TAB = "com.intellij.terminal.frontend.toolwindow.TerminalToolWindowTab";
    private static final String VIEW = "com.intellij.terminal.frontend.view.TerminalView";
    private static final String OPTIONS = "org.jetbrains.plugins.terminal.session.TerminalStartupOptions";
    private static final String BUILDER = "org.jetbrains.plugins.terminal.view.TerminalSendTextBuilder";
    private static final String DEFERRED = "kotlinx.coroutines.Deferred";
    private static final String JOB = "kotlinx.coroutines.Job";

    private ReworkedTabs() {
    }

    /** The tab whose shell is one of {@code shellPids}. On the UI thread. */
    static Terminals.Terminal find(Project project, Set<Long> shellPids, IdeTerminals owner) {
        ToolWindow window = ToolWindowManager.getInstance(project).getToolWindow(IdeTerminals.TOOL_WINDOW);
        if (window == null) return null;
        ClassLoader loader = null;
        for (Content content : window.getContentManager().getContents()) {
            loader = loader(content.getComponent(), 0);
            if (loader != null) break;
        }
        // One loader serves every tab of the reworked terminal.
        return loader == null ? null : find(project, shellPids, loader);
    }

    /** The loader of the first component of the reworked terminal in a tab, looking a few levels down. */
    static ClassLoader loader(java.awt.Component component, int depth) {
        if (component == null || depth > 8) return null;
        if (component.getClass().getName().startsWith("com.intellij.terminal.frontend.")) {
            ClassLoader loader = component.getClass().getClassLoader();
            try {
                Class.forName(MANAGER, false, loader);
                return loader;
            } catch (ClassNotFoundException | LinkageError other) {
                return null;
            }
        }
        if (component instanceof java.awt.Container) {
            for (java.awt.Component child : ((java.awt.Container) component).getComponents()) {
                ClassLoader found = loader(child, depth + 1);
                if (found != null) return found;
            }
        }
        return null;
    }

    private static Terminals.Terminal find(Project project, Set<Long> shellPids, ClassLoader loader) {
        try {
            Class<?> managerClass = Class.forName(MANAGER, true, loader);
            Class<?> tabClass = Class.forName(TAB, true, loader);
            Class<?> viewClass = Class.forName(VIEW, true, loader);
            Class<?> optionsClass = Class.forName(OPTIONS, true, loader);
            Class<?> deferredClass = Class.forName(DEFERRED, true, loader);
            Class<?> jobClass = Class.forName(JOB, true, loader);
            Object manager = managerClass.getMethod("getInstance", Project.class).invoke(null, project);
            List<?> tabs = (List<?>) managerClass.getMethod("getTabs").invoke(manager);
            Method view = tabClass.getMethod("getView");
            Method content = tabClass.getMethod("getContent");
            Method startup = viewClass.getMethod("getStartupOptionsDeferred");
            Method completed = jobClass.getMethod("isCompleted");
            Method value = deferredClass.getMethod("getCompleted");
            Method pid = optionsClass.getMethod("getPid");
            for (Object tab : tabs) {
                Object terminal = view.invoke(tab);
                Object options = startup.invoke(terminal);
                if (!Boolean.TRUE.equals(completed.invoke(options))) continue;
                Object started = value.invoke(options);
                Object shell = started == null ? null : pid.invoke(started);
                if (!(shell instanceof Long) || !shellPids.contains(shell)) continue;
                return tab(project, (Content) content.invoke(tab), terminal, viewClass, Class.forName(BUILDER, true, loader));
            }
        } catch (ReflectiveOperationException | LinkageError | ClassCastException | IllegalArgumentException absent) {
            return null;
        }
        return null;
    }

    private static Terminals.Terminal tab(Project project, Content content, Object view, Class<?> viewClass, Class<?> builderClass)
            throws ReflectiveOperationException {
        Method sendText = viewClass.getMethod("sendText", String.class);
        Method createBuilder = viewClass.getMethod("createSendTextBuilder");
        Method bracketed = builderClass.getMethod("useBracketedPasteMode");
        Method send = builderClass.getMethod("send", String.class);
        Method component = viewClass.getMethod("getComponent");
        Method focusable = viewClass.getMethod("getPreferredFocusableComponent");
        return new IdeTerminals.Tab(project, content, (JComponent) component.invoke(view)) {
            @Override
            public void type(String text) {
                call(() -> sendText.invoke(view, text));
            }

            @Override
            public void paste(String text) {
                // Bracketed when the program asked for it, as a paste from the clipboard is.
                call(() -> send.invoke(bracketed.invoke(createBuilder.invoke(view)), text));
            }

            @Override
            void requestFocus() {
                call(() -> {
                    Object target = focusable.invoke(view);
                    if (target instanceof JComponent) ((JComponent) target).requestFocusInWindow();
                    return null;
                });
            }
        };
    }

    private interface Reflective {
        Object run() throws ReflectiveOperationException;
    }

    private static void call(Reflective work) {
        IdeTerminals.onEdt(() -> {
            try {
                work.run();
            } catch (ReflectiveOperationException error) {
                throw new IllegalStateException(error);
            }
            return null;
        });
    }
}
