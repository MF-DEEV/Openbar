// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import com.intellij.ide.plugins.IdeaPluginDescriptor;
import com.intellij.ide.plugins.PluginManagerCore;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.components.Service;
import com.intellij.openapi.diagnostic.Logger;
import com.intellij.openapi.extensions.PluginId;
import java.io.IOException;

/**
 * One bridge per IDE process, for all its project windows: it starts with
 * the IDE and stops with it, or when the plugin is taken out.
 */
@Service(Service.Level.APP)
public final class BridgeService implements Disposable {
    static final String PLUGIN_ID = "com.openbar.terminal-bridge";
    private static final Logger LOG = Logger.getInstance(BridgeService.class);
    private BridgeServer server;

    static BridgeService getInstance() {
        return ApplicationManager.getApplication().getService(BridgeService.class);
    }

    synchronized void start() {
        if (server != null) return;
        long pid = ProcessHandle.current().pid();
        Places.System system = Places.system(System.getProperty("os.name"));
        Places places = Places.of(system, System.getProperty("user.home"), System.getenv(), System.getProperty("user.name"), pid);
        IdeTerminals terminals = new IdeTerminals();
        // The Mac's socket sits in a folder only its owner can open, which is
        // the key there, as for the VS Code bridge.
        String token = system == Places.System.MAC ? null : BridgeServer.newToken();
        Protocol protocol = new Protocol(terminals, version(), token, Thread::sleep);
        BridgeServer created = new BridgeServer(places, version(), terminals.ide(), pid, token, protocol::handle);
        try {
            created.start();
            server = created;
            LOG.info("Openbar bridge listening for " + terminals.ide());
        } catch (IOException | RuntimeException error) {
            created.stop();
            LOG.warn("Openbar bridge could not start", error);
        }
    }

    synchronized void stop() {
        if (server != null) server.stop();
        server = null;
    }

    static String version() {
        IdeaPluginDescriptor plugin = PluginManagerCore.getPlugin(PluginId.getId(PLUGIN_ID));
        return plugin != null && plugin.getVersion() != null ? plugin.getVersion() : "0";
    }

    @Override
    public void dispose() {
        stop();
    }
}
