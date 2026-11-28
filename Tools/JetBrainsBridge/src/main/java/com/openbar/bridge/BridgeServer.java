// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.attribute.PosixFilePermissions;
import java.security.SecureRandom;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.Semaphore;
import java.util.concurrent.TimeUnit;
import java.util.function.Function;

/**
 * Listens where {@link Places} says, one JSON line per connection, and
 * answers it with what {@link Protocol} makes of it. Nothing is read from
 * the terminals and nothing leaves the machine.
 */
final class BridgeServer {
    static final int MAX_REQUEST = 64 * 1024;
    /** The time a client has to send its request. */
    static final long READ_TIMEOUT_MS = 5_000;
    /** The longest one answer may take, pauses included. */
    static final long ANSWER_TIMEOUT_MS = 120_000;

    private final Places places;
    private final String version;
    private final String ide;
    private final long pid;
    private final String token;
    private final Function<String, String> handler;
    private ServerSocketChannel server;
    private ExecutorService workers;
    private ScheduledExecutorService timers;
    private final Semaphore slots = new Semaphore(8);

    BridgeServer(Places places, String version, String ide, long pid, String token, Function<String, String> handler) {
        this.places = places;
        this.version = version;
        this.ide = ide;
        this.pid = pid;
        this.token = token;
        this.handler = handler;
    }

    /** A random key for the registration file, 48 hex characters as the VS Code bridge makes them. */
    static String newToken() {
        byte[] bytes = new byte[24];
        new SecureRandom().nextBytes(bytes);
        StringBuilder hex = new StringBuilder();
        for (byte b : bytes) hex.append(String.format("%02x", b));
        return hex.toString();
    }

    synchronized void start() throws IOException {
        if (server != null) return;
        workers = Executors.newCachedThreadPool(daemon("Openbar bridge"));
        timers = Executors.newSingleThreadScheduledExecutor(daemon("Openbar bridge timer"));
        int port = 0;
        if (places.socket != null) {
            Path folder = places.socket.getParent();
            Files.createDirectories(folder);
            if (places.system == Places.System.MAC) ownerOnly(folder, "rwx------");
            Files.deleteIfExists(places.socket);
            server = ServerSocketChannel.open(StandardProtocolFamily.UNIX);
            server.bind(UnixDomainSocketAddress.of(places.socket));
            // The folder already keeps everyone else out on the Mac; the
            // socket itself is the owner's alone everywhere.
            ownerOnly(places.socket, "rw-------");
        } else {
            server = ServerSocketChannel.open(StandardProtocolFamily.INET);
            server.bind(new InetSocketAddress(InetAddress.getLoopbackAddress(), 0));
            port = ((InetSocketAddress) server.getLocalAddress()).getPort();
        }
        if (places.registration != null) register(port);
        ServerSocketChannel listening = server;
        Thread accept = new Thread(() -> acceptLoop(listening), "Openbar bridge accept");
        accept.setDaemon(true);
        accept.start();
    }

    synchronized void stop() {
        if (server == null) return;
        try {
            server.close();
        } catch (IOException ignored) {
        }
        server = null;
        workers.shutdownNow();
        timers.shutdownNow();
        try {
            if (places.socket != null) Files.deleteIfExists(places.socket);
        } catch (IOException ignored) {
        }
        try {
            if (places.registration != null) Files.deleteIfExists(places.registration);
        } catch (IOException ignored) {
        }
    }

    /** Where the island finds this IDE and the key it has to bring, written so it is never read half done. */
    private void register(int port) throws IOException {
        Path file = places.registration;
        Files.createDirectories(file.getParent());
        Map<String, Object> body = new LinkedHashMap<>();
        if (places.socket != null) body.put("pipe", places.socket.toString());
        else body.put("port", port);
        body.put("token", token);
        body.put("version", version);
        body.put("ide", ide);
        body.put("kind", "jetbrains");
        body.put("pid", pid);
        Path temporary = file.resolveSibling(file.getFileName() + ".tmp");
        Files.write(temporary, Json.write(body).getBytes(StandardCharsets.UTF_8));
        ownerOnly(temporary, "rw-------");
        Files.move(temporary, file, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
    }

    private static void ownerOnly(Path path, String permissions) {
        try {
            Files.setPosixFilePermissions(path, PosixFilePermissions.fromString(permissions));
        } catch (UnsupportedOperationException | IOException ignored) {
            // Windows keeps the user's own app data to the user.
        }
    }

    private void acceptLoop(ServerSocketChannel listening) {
        while (listening.isOpen()) {
            SocketChannel connection;
            try {
                connection = listening.accept();
            } catch (IOException closed) {
                return;
            }
            if (!slots.tryAcquire()) {
                close(connection);
                continue;
            }
            try {
                workers.execute(() -> {
                    try {
                        serve(connection);
                    } finally {
                        slots.release();
                    }
                });
            } catch (RuntimeException stopped) {
                slots.release();
                close(connection);
            }
        }
    }

    private void serve(SocketChannel connection) {
        ScheduledFuture<?> deadline = timers.schedule(() -> close(connection), READ_TIMEOUT_MS, TimeUnit.MILLISECONDS);
        try (connection) {
            String line = readLine(connection);
            deadline.cancel(false);
            if (line == null) return;
            deadline = timers.schedule(() -> close(connection), ANSWER_TIMEOUT_MS, TimeUnit.MILLISECONDS);
            String reply = handler.apply(line) + "\n";
            ByteBuffer out = ByteBuffer.wrap(reply.getBytes(StandardCharsets.UTF_8));
            while (out.hasRemaining()) connection.write(out);
        } catch (IOException | RuntimeException ignored) {
        } finally {
            deadline.cancel(false);
        }
    }

    /** The first line the client sends, or null when it sends none or too much. */
    static String readLine(SocketChannel connection) throws IOException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        ByteBuffer chunk = ByteBuffer.allocate(4096);
        while (bytes.size() <= MAX_REQUEST) {
            chunk.clear();
            int count = connection.read(chunk);
            if (count < 0) return null;
            for (int i = 0; i < count; i++) {
                byte b = chunk.get(i);
                if (b == '\n') return bytes.toString(StandardCharsets.UTF_8);
                bytes.write(b);
            }
        }
        return null;
    }

    private static void close(SocketChannel connection) {
        try {
            connection.close();
        } catch (IOException ignored) {
        }
    }

    private static java.util.concurrent.ThreadFactory daemon(String name) {
        return runnable -> {
            Thread thread = new Thread(runnable, name);
            thread.setDaemon(true);
            return thread;
        };
    }
}
