// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Map;

/**
 * Where the bridge listens and where it tells the island so, the same
 * places the VS Code bridge uses on each system:
 * <ul>
 * <li>the Mac: a socket in {@code ~/.openbar/terminals}, a folder only its
 * owner can open, named after the IDE's process; the island asks every
 * socket there;</li>
 * <li>Linux: a socket in the user's runtime folder, named with a random key
 * in {@code ~/.local/share/Openbar/terminals/<pid>.json};</li>
 * <li>Windows: a port on the loopback address, with its key in
 * {@code %LOCALAPPDATA%\Openbar\terminals\<pid>.json}. Java has no named
 * pipe server, and only a request with the key is answered.</li>
 * </ul>
 */
final class Places {
    enum System { MAC, LINUX, WINDOWS }

    final System system;
    /** The socket to listen on, or null for a port (Windows). */
    final Path socket;
    /** The file that tells the island where to ask and with which key, or null (the Mac). */
    final Path registration;

    private Places(System system, Path socket, Path registration) {
        this.system = system;
        this.socket = socket;
        this.registration = registration;
    }

    static System system(String osName) {
        String name = osName.toLowerCase(java.util.Locale.ROOT);
        if (name.startsWith("mac") || name.startsWith("darwin")) return System.MAC;
        if (name.startsWith("windows")) return System.WINDOWS;
        return System.LINUX;
    }

    /** The places for this process, from the system's name, the home folder and the environment. */
    static Places of(System system, String home, Map<String, String> environment, String user, long pid) {
        switch (system) {
            case MAC:
                return new Places(system, Paths.get(home, ".openbar", "terminals", pid + ".sock"), null);
            case WINDOWS: {
                String local = nonEmpty(environment.get("LOCALAPPDATA"));
                Path base = local != null ? Paths.get(local) : Paths.get(home, "AppData", "Local");
                return new Places(system, null, base.resolve("Openbar").resolve("terminals").resolve(pid + ".json"));
            }
            default: {
                String data = nonEmpty(environment.get("XDG_DATA_HOME"));
                Path registry = (data != null ? Paths.get(data) : Paths.get(home, ".local", "share"))
                        .resolve("Openbar").resolve("terminals").resolve(pid + ".json");
                String runtime = nonEmpty(environment.get("XDG_RUNTIME_DIR"));
                Path folder = runtime != null ? Paths.get(runtime) : Paths.get(java.lang.System.getProperty("java.io.tmpdir"));
                String name = "openbar-terminals-" + plain(user) + "-" + pid + ".sock";
                return new Places(system, folder.resolve(name), registry);
            }
        }
    }

    /** A user name with only letters and digits, as the VS Code bridge writes it. */
    static String plain(String user) {
        String plain = user == null ? "" : user.replaceAll("[^A-Za-z0-9]", "");
        return plain.isEmpty() ? "user" : plain;
    }

    private static String nonEmpty(String value) {
        return value == null || value.trim().isEmpty() ? null : value;
    }
}
