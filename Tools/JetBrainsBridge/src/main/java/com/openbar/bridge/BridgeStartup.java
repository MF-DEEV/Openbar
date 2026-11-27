// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import com.intellij.ide.AppLifecycleListener;
import com.intellij.ide.plugins.DynamicPluginListener;
import com.intellij.ide.plugins.IdeaPluginDescriptor;
import java.util.List;

/**
 * Starts the bridge when the IDE has started, or when the plugin is loaded
 * into an IDE that already runs, and stops it before the IDE closes.
 */
public final class BridgeStartup implements AppLifecycleListener, DynamicPluginListener {
    @Override
    public void appFrameCreated(List<String> commandLineArgs) {
        BridgeService.getInstance().start();
    }

    @Override
    public void appStarted() {
        BridgeService.getInstance().start();
    }

    @Override
    public void appWillBeClosed(boolean isRestart) {
        BridgeService.getInstance().stop();
    }

    @Override
    public void pluginLoaded(IdeaPluginDescriptor pluginDescriptor) {
        if (BridgeService.PLUGIN_ID.equals(pluginDescriptor.getPluginId().getIdString())) BridgeService.getInstance().start();
    }

    @Override
    public void beforePluginUnload(IdeaPluginDescriptor pluginDescriptor, boolean isUpdate) {
        if (BridgeService.PLUGIN_ID.equals(pluginDescriptor.getPluginId().getIdString())) BridgeService.getInstance().stop();
    }
}
