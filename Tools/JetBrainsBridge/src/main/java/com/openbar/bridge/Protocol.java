// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * The island's requests, the same the VS Code bridge answers
 * (Tools/AntigravityBridge, windows/tools/ide-bridge): one JSON line in, one
 * out. Every request names the shell of a Claude Code session, or the
 * session's ancestors, and only the IDE holding that shell's tab says yes.
 */
final class Protocol {
    static final String ESC = "\u001b";
    /** The longest pause a request may ask for between two steps. */
    static final long MAX_WAIT = 10_000;

    /** How the bridge pauses between steps; tests count the pauses instead. */
    interface Sleeper {
        void sleep(long milliseconds) throws InterruptedException;
    }

    private final Terminals terminals;
    private final String version;
    /** The key a request has to carry, or null where the socket alone guards it (the Mac). */
    private final String token;
    private final Sleeper sleeper;

    Protocol(Terminals terminals, String version, String token, Sleeper sleeper) {
        this.terminals = terminals;
        this.version = version;
        this.token = token;
        this.sleeper = sleeper;
    }

    /** The answer to one request line, never an exception. */
    String handle(String line) {
        try {
            return Json.write(answer(line));
        } catch (RuntimeException | InterruptedException error) {
            if (error instanceof InterruptedException) Thread.currentThread().interrupt();
            return Json.write(failure(String.valueOf(error.getMessage())));
        } catch (Exception error) {
            return Json.write(failure(String.valueOf(error.getMessage())));
        }
    }

    private Map<String, Object> answer(String line) throws Exception {
        Object parsed = Json.parse(line);
        if (!(parsed instanceof Map)) return failure("unknown");
        Map<?, ?> request = (Map<?, ?>) parsed;
        if (token != null && !token.equals(request.get("token"))) return failure("denied");
        Object command = request.get("command");
        if ("ping".equals(command)) {
            Map<String, Object> reply = ok();
            reply.put("version", version);
            reply.put("ide", terminals.ide());
            reply.put("projects", terminals.projects());
            return reply;
        }
        Terminals.Terminal terminal = terminals.find(shells(request));
        if (terminal == null) return failure("missing");
        if ("focus".equals(command)) {
            terminal.focus();
            return ok();
        }
        if ("shown".equals(command)) {
            Map<String, Object> reply = ok();
            reply.put("shown", terminal.shown());
            return reply;
        }
        if ("steps".equals(command) && request.get("steps") instanceof List) {
            // Any run of pastes, raw text, pauses and returns, so a new way of
            // typing into a session needs no new plugin.
            for (Object item : (List<?>) request.get("steps")) {
                if (!(item instanceof Map)) continue;
                Map<?, ?> step = (Map<?, ?>) item;
                if (step.get("paste") instanceof String) terminal.paste((String) step.get("paste"));
                else if (step.get("text") instanceof String) terminal.type((String) step.get("text"));
                else if (Boolean.TRUE.equals(step.get("enter"))) terminal.type("\r");
                if (step.get("wait") instanceof Number) pause(((Number) step.get("wait")).longValue());
            }
            return ok();
        }
        Object text = request.get("text");
        if ("send-images".equals(command) && text instanceof String && images(request) != null) {
            // Claude Code attaches an image when a paste is only its path, and
            // reads it a moment later, so the text and the return wait for the images.
            List<String> images = images(request);
            for (String image : images) {
                terminal.paste(image);
                pause(60);
            }
            pause(1200 + 300L * images.size());
            if (!((String) text).isEmpty()) terminal.paste((String) text);
            pause(80);
            terminal.type("\r");
            return ok();
        }
        if ("send".equals(command) && text instanceof String && !((String) text).isEmpty()) {
            // A paste keeps line breaks as text; the return after it sends the reply.
            terminal.paste((String) text);
            pause(80);
            terminal.type("\r");
            return ok();
        }
        return failure("unknown");
    }

    /** The pids a request names: {@code shellPid} (the Mac) and {@code shellPids} (Windows, Linux). */
    static Set<Long> shells(Map<?, ?> request) {
        Set<Long> pids = new LinkedHashSet<>();
        if (request.get("shellPid") instanceof Number) pids.add(((Number) request.get("shellPid")).longValue());
        if (request.get("shellPids") instanceof List) {
            for (Object pid : (List<?>) request.get("shellPids")) {
                if (pid instanceof Number) pids.add(((Number) pid).longValue());
            }
        }
        pids.removeIf(pid -> pid <= 1);
        return pids;
    }

    /** The image paths of a request, or null when it names none or names them badly. */
    private static List<String> images(Map<?, ?> request) {
        if (!(request.get("images") instanceof List)) return null;
        List<?> items = (List<?>) request.get("images");
        if (items.isEmpty()) return null;
        java.util.ArrayList<String> images = new java.util.ArrayList<>();
        for (Object item : items) {
            if (!(item instanceof String) || ((String) item).isEmpty()) return null;
            images.add((String) item);
        }
        return images;
    }

    private void pause(long milliseconds) throws InterruptedException {
        if (milliseconds > 0) sleeper.sleep(Math.min(milliseconds, MAX_WAIT));
    }

    private static Map<String, Object> ok() {
        Map<String, Object> reply = new LinkedHashMap<>();
        reply.put("ok", true);
        return reply;
    }

    private static Map<String, Object> failure(String error) {
        Map<String, Object> reply = new LinkedHashMap<>();
        reply.put("ok", false);
        reply.put("error", error);
        return reply;
    }
}
