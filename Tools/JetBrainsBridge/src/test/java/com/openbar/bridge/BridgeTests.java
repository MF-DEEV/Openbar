// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import java.io.IOException;
import java.net.InetAddress;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.attribute.PosixFilePermissions;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * The bridge's own tests, run by test.sh with nothing but a JDK: the IDE's
 * classes stay out, behind {@link Terminals}.
 */
public final class BridgeTests {
    private static int passed;
    private static final List<String> failed = new ArrayList<>();

    public static void main(String[] args) throws Exception {
        if (args.length == 2 && args[0].equals("--built-from")) {
            // The hash build.sh wrote into a jar, read with the JDK alone.
            try (java.util.zip.ZipFile jar = new java.util.zip.ZipFile(args[1])) {
                java.util.zip.ZipEntry entry = jar.getEntry("META-INF/openbar-source.sha256");
                System.out.println(entry == null ? "" : new String(jar.getInputStream(entry).readAllBytes(), StandardCharsets.UTF_8).trim());
            }
            return;
        }
        json();
        protocol();
        places();
        server();
        System.out.println(passed + " passed, " + failed.size() + " failed");
        for (String failure : failed) System.out.println("FAILED: " + failure);
        if (!failed.isEmpty()) System.exit(1);
    }

    private static void expect(boolean condition, String what) {
        if (condition) passed++;
        else failed.add(what);
    }

    /** A terminal that records what reaches it, in order. */
    static final class Recorder implements Terminals.Terminal {
        final List<String> events = new ArrayList<>();
        boolean shown;

        @Override
        public void focus() {
            events.add("focus");
        }

        @Override
        public boolean shown() {
            return shown;
        }

        @Override
        public void type(String text) {
            events.add("type:" + text);
        }

        @Override
        public void paste(String text) {
            events.add("paste:" + text);
        }
    }

    static final class FakeTerminals implements Terminals {
        final Recorder terminal = new Recorder();
        final long shell;
        Set<Long> asked;

        FakeTerminals(long shell) {
            this.shell = shell;
        }

        @Override
        public Terminal find(Set<Long> shellPids) {
            asked = shellPids;
            return shellPids.contains(shell) ? terminal : null;
        }

        @Override
        public List<String> projects() {
            return List.of("/work/alpha", "/work/beta");
        }

        @Override
        public String ide() {
            return "PyCharm";
        }
    }

    private static void json() {
        Object parsed = Json.parse("{\"command\":\"send\",\"text\":\"a\\nb \\\"q\\\" \\u00e9\",\"shellPids\":[12, 3],\"x\":null,\"t\":true,\"f\":-1.5}");
        expect(parsed instanceof Map, "an object parses to a map");
        Map<?, ?> object = (Map<?, ?>) parsed;
        expect("a\nb \"q\" \u00e9".equals(object.get("text")), "escapes in strings are read");
        expect(List.of(12L, 3L).equals(object.get("shellPids")), "whole numbers come back as longs");
        expect(object.containsKey("x") && object.get("x") == null, "null is kept");
        expect(Boolean.TRUE.equals(object.get("t")), "true is read");
        expect(Double.valueOf(-1.5).equals(object.get("f")), "a fraction is a double");
        expect("{\"ok\":true,\"s\":\"a\\nb\\u001b[200~\\\"\"}".equals(Json.write(orderedOkAnd("s", "a\nb\u001b[200~\""))), "a reply line escapes line breaks, quotes and ESC");
        expect(Json.parse(Json.write(orderedOkAnd("s", "\u2028 x"))) instanceof Map, "what is written reads back");
        for (String bad : new String[] {"{", "{\"a\" 1}", "[1,]", "\"open", "tru", "{} x", "{\"a\":\"\\u12\"}"}) {
            boolean threw = false;
            try {
                Json.parse(bad);
            } catch (IllegalArgumentException expected) {
                threw = true;
            }
            expect(threw, "bad JSON is refused: " + bad);
        }
        StringBuilder deep = new StringBuilder();
        for (int i = 0; i < 40; i++) deep.append('[');
        boolean threw = false;
        try {
            Json.parse(deep.toString());
        } catch (IllegalArgumentException expected) {
            threw = true;
        }
        expect(threw, "nesting past 32 levels is refused");
    }

    private static Map<String, Object> orderedOkAnd(String key, Object value) {
        Map<String, Object> map = new java.util.LinkedHashMap<>();
        map.put("ok", true);
        map.put(key, value);
        return map;
    }

    private static void protocol() {
        List<Long> pauses = new ArrayList<>();
        FakeTerminals terminals = new FakeTerminals(222);
        Protocol open = new Protocol(terminals, "1.0.0", null, pauses::add);

        Map<?, ?> ping = (Map<?, ?>) Json.parse(open.handle("{\"command\":\"ping\"}"));
        expect(Boolean.TRUE.equals(ping.get("ok")) && "1.0.0".equals(ping.get("version")) && "PyCharm".equals(ping.get("ide")),
                "ping names the version and the IDE");
        expect(List.of("/work/alpha", "/work/beta").equals(ping.get("projects")), "ping lists the open projects");

        expect(reply(open, "{\"command\":\"focus\",\"shellPid\":111}").get("error").equals("missing"),
                "a shell no tab runs is missing");
        expect(Boolean.TRUE.equals(reply(open, "{\"command\":\"focus\",\"shellPid\":222}").get("ok"))
                && terminals.terminal.events.equals(List.of("focus")), "the tab with the shell comes forward (Mac request)");
        terminals.terminal.events.clear();
        expect(Boolean.TRUE.equals(reply(open, "{\"command\":\"focus\",\"shellPids\":[900,222,1]}").get("ok")),
                "any ancestor naming the shell finds it (Windows and Linux request)");
        expect(!terminals.asked.contains(1L), "pid 1 is never asked for");

        terminals.terminal.events.clear();
        pauses.clear();
        expect(Boolean.TRUE.equals(reply(open, "{\"command\":\"send\",\"shellPid\":222,\"text\":\"line one\\nline two\"}").get("ok")),
                "a reply is sent");
        expect(terminals.terminal.events.equals(List.of("paste:line one\nline two", "type:\r")) && pauses.equals(List.of(80L)),
                "a reply is one paste, a pause and a return");
        expect("unknown".equals(reply(open, "{\"command\":\"send\",\"shellPid\":222,\"text\":\"\"}").get("error")),
                "an empty reply is refused");

        terminals.terminal.events.clear();
        pauses.clear();
        reply(open, "{\"command\":\"send-images\",\"shellPid\":222,\"text\":\"Look\",\"images\":[\"/tmp/a.png\",\"/tmp/b.png\"]}");
        expect(terminals.terminal.events.equals(List.of("paste:/tmp/a.png", "paste:/tmp/b.png", "paste:Look", "type:\r")),
                "images are pasted as paths before the text");
        expect(pauses.equals(List.of(60L, 60L, 1800L, 80L)), "Claude Code gets time to read the images");
        terminals.terminal.events.clear();
        reply(open, "{\"command\":\"send-images\",\"shellPid\":222,\"text\":\"\",\"images\":[\"/tmp/a.png\"]}");
        expect(terminals.terminal.events.equals(List.of("paste:/tmp/a.png", "type:\r")), "images go without text too");
        expect("unknown".equals(reply(open, "{\"command\":\"send-images\",\"shellPid\":222,\"text\":\"x\",\"images\":[\"\"]}").get("error")),
                "an empty image path is refused");

        terminals.terminal.events.clear();
        pauses.clear();
        reply(open, "{\"command\":\"steps\",\"shellPid\":222,\"steps\":[{\"paste\":\"hi\"},{\"wait\":99999},{\"text\":\"\\u001b[B\"},{\"enter\":true},7]}");
        expect(terminals.terminal.events.equals(List.of("paste:hi", "type:\u001b[B", "type:\r")), "steps type in order");
        expect(pauses.equals(List.of(Protocol.MAX_WAIT)), "a pause is capped at ten seconds");

        terminals.terminal.shown = true;
        expect(Boolean.TRUE.equals(reply(open, "{\"command\":\"shown\",\"shellPid\":222}").get("shown")), "a tab in front says so");
        terminals.terminal.shown = false;
        expect(Boolean.FALSE.equals(reply(open, "{\"command\":\"shown\",\"shellPid\":222}").get("shown")), "a tab behind says so");
        expect("unknown".equals(reply(open, "{\"command\":\"delete-everything\",\"shellPid\":222}").get("error")),
                "an unknown command does nothing");
        expect(Boolean.FALSE.equals(reply(open, "not json").get("ok")), "a broken request gets an answer, not a crash");
        expect("unknown".equals(reply(open, "[1,2]").get("error")), "a request that isn't an object is unknown");

        Protocol keyed = new Protocol(terminals, "1.0.0", "secret", pauses::add);
        terminals.terminal.events.clear();
        expect("denied".equals(reply(keyed, "{\"command\":\"focus\",\"shellPid\":222}").get("error")), "a request without the key is refused");
        expect("denied".equals(reply(keyed, "{\"command\":\"ping\",\"token\":\"wrong\"}").get("error")), "a wrong key is refused");
        expect(terminals.terminal.events.isEmpty(), "a refused request touches nothing");
        expect(Boolean.TRUE.equals(reply(keyed, "{\"command\":\"focus\",\"shellPid\":222,\"token\":\"secret\"}").get("ok")),
                "a request with the key is answered");
    }

    private static Map<?, ?> reply(Protocol protocol, String line) {
        return (Map<?, ?>) Json.parse(protocol.handle(line));
    }

    private static void places() {
        Places mac = Places.of(Places.System.MAC, "/Users/me", Map.of(), "me", 42);
        expect(mac.socket.equals(Paths.get("/Users/me/.openbar/terminals/42.sock")) && mac.registration == null,
                "the Mac listens where the island's Mac client looks");
        Places linux = Places.of(Places.System.LINUX, "/home/me", Map.of("XDG_RUNTIME_DIR", "/run/user/1000"), "me.x", 42);
        expect(linux.socket.equals(Paths.get("/run/user/1000/openbar-terminals-mex-42.sock")), "Linux listens in the runtime folder");
        expect(linux.registration.equals(Paths.get("/home/me/.local/share/Openbar/terminals/42.json")),
                "Linux registers where the island reads");
        Places xdg = Places.of(Places.System.LINUX, "/home/me", Map.of("XDG_DATA_HOME", "/data"), "", 7);
        expect(xdg.registration.equals(Paths.get("/data/Openbar/terminals/7.json")), "XDG_DATA_HOME moves the registry");
        expect(xdg.socket.getFileName().toString().equals("openbar-terminals-user-7.sock"), "an empty user name still makes a name");
        Places windows = Places.of(Places.System.WINDOWS, "C:\\Users\\me", Map.of("LOCALAPPDATA", "/local"), "me", 9);
        expect(windows.socket == null && windows.registration.equals(Paths.get("/local", "Openbar", "terminals", "9.json")),
                "Windows registers a port in the user's local app data");
        expect(Places.system("Mac OS X") == Places.System.MAC && Places.system("Windows 11") == Places.System.WINDOWS
                && Places.system("Linux") == Places.System.LINUX, "the system is told by its name");
    }

    private static void server() throws Exception {
        // A socket's path has to stay short (104 bytes on the Mac), shorter than the Mac's own temporary folder.
        Path short_ = Paths.get("/tmp");
        Path folder = Files.isDirectory(short_) ? Files.createTempDirectory(short_, "obb") : Files.createTempDirectory("obb");
        Path home = folder.resolve("home");
        FakeTerminals terminals = new FakeTerminals(5);

        // The Mac: a socket in ~/.openbar/terminals, no key.
        Places mac = Places.of(Places.System.MAC, home.toString(), Map.of(), "me", 77);
        Protocol open = new Protocol(terminals, "1.0.0", null, ms -> { });
        BridgeServer macServer = new BridgeServer(mac, "1.0.0", "PyCharm", 77, null, open::handle);
        macServer.start();
        expect(Files.exists(mac.socket), "the Mac socket is there once started");
        expect("rw-------".equals(PosixFilePermissions.toString(Files.getPosixFilePermissions(mac.socket))),
                "only the owner can open the socket");
        expect("rwx------".equals(PosixFilePermissions.toString(Files.getPosixFilePermissions(mac.socket.getParent()))),
                "only the owner can open its folder");
        String answer = exchange(SocketChannel.open(UnixDomainSocketAddress.of(mac.socket)), "{\"command\":\"focus\",\"shellPid\":5}\n");
        expect("{\"ok\":true}".equals(answer), "a request over the socket is answered");
        String split = exchange(SocketChannel.open(UnixDomainSocketAddress.of(mac.socket)), "{\"command\":\"pi", "ng\"}\n");
        expect(split != null && split.startsWith("{\"ok\":true,\"version\":\"1.0.0\""), "a request in pieces is read whole");
        macServer.stop();
        expect(!Files.exists(mac.socket), "the socket goes when the bridge stops");

        // Linux: a socket in the runtime folder, its key in a registration file.
        Places linux = Places.of(Places.System.LINUX, home.toString(), Map.of("XDG_RUNTIME_DIR", folder.toString()), "me", 78);
        Protocol keyed = new Protocol(terminals, "1.0.0", "k1", ms -> { });
        BridgeServer linuxServer = new BridgeServer(linux, "1.0.0", "PyCharm", 78, "k1", keyed::handle);
        linuxServer.start();
        Map<?, ?> registration = (Map<?, ?>) Json.parse(Files.readString(linux.registration));
        expect(linux.socket.toString().equals(registration.get("pipe")) && "k1".equals(registration.get("token"))
                && Long.valueOf(78).equals(registration.get("pid")) && "jetbrains".equals(registration.get("kind")),
                "Linux registers its socket, key and pid");
        expect("rw-------".equals(PosixFilePermissions.toString(Files.getPosixFilePermissions(linux.registration))),
                "only the owner can read the key");
        String denied = exchange(SocketChannel.open(UnixDomainSocketAddress.of(linux.socket)), "{\"command\":\"ping\"}\n");
        expect(denied != null && denied.contains("denied"), "Linux refuses a request without the key");
        linuxServer.stop();
        expect(!Files.exists(linux.registration) && !Files.exists(linux.socket), "Linux takes its files away on stop");

        // Windows: a loopback port, its key in a registration file.
        Places windows = Places.of(Places.System.WINDOWS, home.toString(), Map.of("LOCALAPPDATA", folder.toString()), "me", 79);
        Protocol windowsKeyed = new Protocol(terminals, "1.0.0", "k2", ms -> { });
        BridgeServer windowsServer = new BridgeServer(windows, "1.0.0", "PyCharm", 79, "k2", windowsKeyed::handle);
        windowsServer.start();
        Map<?, ?> port = (Map<?, ?>) Json.parse(Files.readString(windows.registration));
        int number = ((Number) port.get("port")).intValue();
        expect(number > 0 && !port.containsKey("pipe"), "Windows registers a port");
        SocketChannel tcp = SocketChannel.open(new java.net.InetSocketAddress(InetAddress.getLoopbackAddress(), number));
        String sent = exchange(tcp, "{\"command\":\"send\",\"shellPids\":[5],\"text\":\"hi\",\"token\":\"k2\"}\n");
        expect("{\"ok\":true}".equals(sent), "a request with the key over the port is answered");
        windowsServer.stop();
        expect(!Files.exists(windows.registration), "Windows takes its registration away on stop");

        // Too much before a line break is dropped without an answer.
        BridgeServer again = new BridgeServer(mac, "1.0.0", "PyCharm", 77, null, open::handle);
        again.start();
        String flood = exchange(SocketChannel.open(UnixDomainSocketAddress.of(mac.socket)), "x".repeat(BridgeServer.MAX_REQUEST + 10));
        expect(flood == null, "a request over 64 KiB is dropped");
        again.stop();
        try (java.util.stream.Stream<Path> paths = Files.walk(folder)) {
            paths.sorted(java.util.Comparator.reverseOrder()).forEach(path -> path.toFile().delete());
        }
    }

    /** Writes the pieces, then reads one answer line (null when the bridge closes without one). */
    private static String exchange(SocketChannel channel, String... pieces) throws IOException {
        try (channel) {
            for (String piece : pieces) {
                ByteBuffer out = ByteBuffer.wrap(piece.getBytes(StandardCharsets.UTF_8));
                while (out.hasRemaining()) channel.write(out);
            }
            StringBuilder reply = new StringBuilder();
            ByteBuffer in = ByteBuffer.allocate(4096);
            while (true) {
                in.clear();
                int count;
                try {
                    count = channel.read(in);
                } catch (IOException reset) {
                    return reply.length() == 0 ? null : reply.toString();
                }
                if (count < 0) break;
                reply.append(new String(in.array(), 0, count, StandardCharsets.UTF_8));
                int end = reply.indexOf("\n");
                if (end >= 0) return reply.substring(0, end);
            }
            return reply.length() == 0 ? null : reply.toString();
        }
    }
}
